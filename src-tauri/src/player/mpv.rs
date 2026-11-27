//! A thin safe wrapper around one libmpv core.

use std::ffi::{c_char, CStr, CString};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::ffi::{self, Api};
use super::options;
use crate::error::{AppError, AppResult};

/// One mpv core. Dropping the last reference shuts it down, so everything that
/// uses the core (the event reader, the renderer) holds an `Arc<Mpv>`.
pub struct Mpv {
    api: &'static Api,
    handle: NonNull<ffi::Handle>,
    reader_taken: AtomicBool,
}

// SAFETY: the libmpv client API is documented as thread-safe. The one
// exception, `mpv_wait_event`, is only reachable through `EventReader`, of
// which there is at most one per core.
unsafe impl Send for Mpv {}
// SAFETY: as above.
unsafe impl Sync for Mpv {}

/// What the event thread cares about, copied out of mpv's memory.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    /// The wait timed out.
    Idle,
    Shutdown,
    FileLoaded,
    /// A file stopped because it could not be played.
    Failed(String),
    Seek,
    /// Playback (re)started after loading or seeking.
    PlaybackRestart,
    PropertyChange(String),
    Other,
}

fn c_string(value: &str) -> AppResult<CString> {
    CString::new(value).map_err(|_| AppError::Player("invalid text for the video engine".into()))
}

/// # Safety
/// `text` must be null or point to a NUL-terminated string.
unsafe fn owned(text: *const c_char) -> String {
    if text.is_null() {
        String::new()
    } else {
        CStr::from_ptr(text).to_string_lossy().into_owned()
    }
}

impl Mpv {
    /// Creates and initializes a core with our fixed options, plus `extra`.
    pub fn new(extra: &[(&str, &str)]) -> AppResult<Arc<Self>> {
        let api = ffi::api().map_err(|error| {
            AppError::Player(format!("the video engine could not be loaded ({error})"))
        })?;
        // SAFETY: mpv_create only needs the C numeric locale, which `ffi::api`
        // sets. A null result is handled.
        let handle = NonNull::new(unsafe { (api.mpv_create)() })
            .ok_or_else(|| AppError::Player("the video engine could not be created".into()))?;
        let mpv = Self {
            api,
            handle,
            reader_taken: AtomicBool::new(false),
        };
        for option in options::OPTIONS {
            if let Err(error) = mpv.set_option(option.name, option.value) {
                if option.required {
                    log::error!("mpv rejected {}={}", option.name, option.value);
                    return Err(error);
                }
                // Builds without Lua or ytdl do not know some options.
                log::debug!("mpv option {} not available", option.name);
            }
        }
        for (name, value) in extra {
            mpv.set_option(name, value)?;
        }
        // SAFETY: the handle is valid and not yet initialized.
        mpv.check(unsafe { (api.mpv_initialize)(mpv.raw()) })?;
        // SAFETY: valid handle and C string.
        unsafe { (api.mpv_request_log_messages)(mpv.raw(), c"warn".as_ptr()) };
        Ok(Arc::new(mpv))
    }

    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn api(&self) -> &'static Api {
        self.api
    }

    /// The raw handle, valid for as long as `self` is.
    pub fn raw(&self) -> *mut ffi::Handle {
        self.handle.as_ptr()
    }

    pub fn check(&self, code: i32) -> AppResult<()> {
        if code >= 0 {
            return Ok(());
        }
        Err(AppError::Player(self.error_text(code)))
    }

    fn error_text(&self, code: i32) -> String {
        // SAFETY: mpv_error_string returns a static C string for any code.
        unsafe { owned((self.api.mpv_error_string)(code)) }
    }

    fn set_option(&self, name: &str, value: &str) -> AppResult<()> {
        let (name, value) = (c_string(name)?, c_string(value)?);
        // SAFETY: valid handle; both strings outlive the call.
        self.check(unsafe {
            (self.api.mpv_set_option_string)(self.raw(), name.as_ptr(), value.as_ptr())
        })
    }

    pub fn set(&self, name: &str, value: &str) -> AppResult<()> {
        let (name, value) = (c_string(name)?, c_string(value)?);
        // SAFETY: valid handle; both strings outlive the call.
        self.check(unsafe {
            (self.api.mpv_set_property_string)(self.raw(), name.as_ptr(), value.as_ptr())
        })
    }

    /// A property formatted as text. Structured properties come as JSON.
    pub fn get_string(&self, name: &str) -> Option<String> {
        let name = c_string(name).ok()?;
        // SAFETY: valid handle and name. A non-null result is a NUL-terminated
        // string that we own; it is copied and then released with mpv_free.
        unsafe {
            let value = (self.api.mpv_get_property_string)(self.raw(), name.as_ptr());
            if value.is_null() {
                return None;
            }
            let copy = owned(value);
            (self.api.mpv_free)(value.cast());
            Some(copy)
        }
    }

    pub fn get_f64(&self, name: &str) -> Option<f64> {
        let name = c_string(name).ok()?;
        let mut value = 0.0_f64;
        // SAFETY: with FORMAT_DOUBLE, mpv writes one f64 to the pointer.
        let code = unsafe {
            (self.api.mpv_get_property)(
                self.raw(),
                name.as_ptr(),
                ffi::FORMAT_DOUBLE,
                std::ptr::from_mut(&mut value).cast(),
            )
        };
        (code >= 0 && value.is_finite()).then_some(value)
    }

    pub fn get_flag(&self, name: &str) -> bool {
        self.get_string(name).is_some_and(|value| value == "yes")
    }

    fn with_args(args: &[&str], run: impl FnOnce(*mut *const c_char) -> i32) -> AppResult<i32> {
        let owned: Vec<CString> = args.iter().map(|a| c_string(a)).collect::<AppResult<_>>()?;
        let mut pointers: Vec<*const c_char> = owned.iter().map(|a| a.as_ptr()).collect();
        pointers.push(std::ptr::null());
        Ok(run(pointers.as_mut_ptr()))
    }

    /// Runs a command. Each argument is passed on its own, so nothing in a file
    /// name is ever parsed as part of the command.
    pub fn command(&self, args: &[&str]) -> AppResult<()> {
        let code = Self::with_args(args, |pointers| {
            // SAFETY: a null-terminated array of C strings that outlive the call.
            unsafe { (self.api.mpv_command)(self.raw(), pointers) }
        })?;
        self.check(code)
    }

    /// Queues a command without waiting, which is safe on the main thread.
    pub fn command_async(&self, args: &[&str]) -> AppResult<()> {
        let code = Self::with_args(args, |pointers| {
            // SAFETY: as in `command`; mpv copies the arguments before returning.
            unsafe { (self.api.mpv_command_async)(self.raw(), 0, pointers) }
        })?;
        self.check(code)
    }

    /// Asks for a `PropertyChange` event whenever the property changes.
    pub fn observe(&self, name: &str) -> AppResult<()> {
        let name = c_string(name)?;
        // SAFETY: valid handle; mpv copies the name.
        self.check(unsafe {
            (self.api.mpv_observe_property)(self.raw(), 0, name.as_ptr(), ffi::FORMAT_NONE)
        })
    }

    /// Makes a pending `EventReader::wait` return early.
    pub fn wakeup(&self) {
        // SAFETY: valid handle; mpv_wakeup is thread-safe.
        unsafe { (self.api.mpv_wakeup)(self.raw()) }
    }

    /// The single reader of this core's events. `None` if already taken.
    pub fn event_reader(self: &Arc<Self>) -> Option<EventReader> {
        (!self.reader_taken.swap(true, Ordering::SeqCst)).then(|| EventReader(Arc::clone(self)))
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        // SAFETY: this is the last reference, so no other call can be running,
        // and the render context (which holds a reference) is already freed.
        unsafe { (self.api.mpv_terminate_destroy)(self.raw()) }
        log::debug!("video engine shut down");
    }
}

pub struct EventReader(Arc<Mpv>);

impl EventReader {
    pub fn wait(&mut self, timeout_secs: f64) -> Event {
        let mpv = &self.0;
        // SAFETY: only this unique reader calls mpv_wait_event. The returned
        // event and its data stay valid until the next call, and everything
        // needed is copied out before returning. `data` has the type that
        // belongs to `event_id`.
        unsafe {
            let event = (mpv.api.mpv_wait_event)(mpv.raw(), timeout_secs);
            let Some(event) = event.as_ref() else {
                return Event::Idle;
            };
            match event.event_id {
                ffi::EVENT_NONE => Event::Idle,
                ffi::EVENT_SHUTDOWN => Event::Shutdown,
                ffi::EVENT_FILE_LOADED => Event::FileLoaded,
                ffi::EVENT_SEEK => Event::Seek,
                ffi::EVENT_PLAYBACK_RESTART => Event::PlaybackRestart,
                ffi::EVENT_END_FILE => match event.data.cast::<ffi::EventEndFile>().as_ref() {
                    Some(end) if end.reason == ffi::END_FILE_REASON_ERROR => {
                        Event::Failed(mpv.error_text(end.error))
                    }
                    _ => Event::Other,
                },
                ffi::EVENT_PROPERTY_CHANGE => {
                    match event.data.cast::<ffi::EventProperty>().as_ref() {
                        Some(property) => Event::PropertyChange(owned(property.name)),
                        None => Event::Other,
                    }
                }
                ffi::EVENT_LOG_MESSAGE => {
                    if let Some(entry) = event.data.cast::<ffi::EventLogMessage>().as_ref() {
                        log::warn!(
                            "mpv {} {}: {}",
                            owned(entry.level),
                            owned(entry.prefix),
                            owned(entry.text).trim_end()
                        );
                    }
                    Event::Other
                }
                _ => Event::Other,
            }
        }
    }
}
