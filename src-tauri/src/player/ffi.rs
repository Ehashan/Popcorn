//! The part of the libmpv C API we use, resolved at runtime.
//!
//! There is no link-time dependency on libmpv, so the app builds without it.
//! Types and constants follow `mpv/client.h`, `render.h` and `render_gl.h`.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::PathBuf;
use std::sync::OnceLock;

/// `mpv_handle`, only ever used behind a pointer.
#[repr(C)]
pub struct Handle {
    _private: [u8; 0],
}

/// `mpv_render_context`, only ever used behind a pointer.
#[repr(C)]
pub struct RenderHandle {
    _private: [u8; 0],
}

#[repr(C)]
pub struct Event {
    pub event_id: c_int,
    pub error: c_int,
    pub reply_userdata: u64,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct EventProperty {
    pub name: *const c_char,
    pub format: c_int,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct EventLogMessage {
    pub prefix: *const c_char,
    pub level: *const c_char,
    pub text: *const c_char,
    pub log_level: c_int,
}

/// Only the leading fields, which are all we read.
#[repr(C)]
pub struct EventEndFile {
    pub reason: c_int,
    pub error: c_int,
}

pub const EVENT_NONE: c_int = 0;
pub const EVENT_SHUTDOWN: c_int = 1;
pub const EVENT_LOG_MESSAGE: c_int = 2;
pub const EVENT_END_FILE: c_int = 7;
pub const EVENT_FILE_LOADED: c_int = 8;
pub const EVENT_SEEK: c_int = 20;
pub const EVENT_PLAYBACK_RESTART: c_int = 21;
pub const EVENT_PROPERTY_CHANGE: c_int = 22;

pub const END_FILE_REASON_ERROR: c_int = 4;

pub const FORMAT_NONE: c_int = 0;
pub const FORMAT_DOUBLE: c_int = 5;

#[repr(C)]
pub struct RenderParam {
    pub kind: c_int,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct OpenGlInitParams {
    pub get_proc_address:
        unsafe extern "C" fn(context: *mut c_void, name: *const c_char) -> *mut c_void,
    pub get_proc_address_ctx: *mut c_void,
}

#[repr(C)]
pub struct OpenGlFbo {
    pub fbo: c_int,
    pub w: c_int,
    pub h: c_int,
    pub internal_format: c_int,
}

pub const RENDER_PARAM_INVALID: c_int = 0;
pub const RENDER_PARAM_API_TYPE: c_int = 1;
pub const RENDER_PARAM_OPENGL_INIT_PARAMS: c_int = 2;
pub const RENDER_PARAM_OPENGL_FBO: c_int = 3;
pub const RENDER_PARAM_FLIP_Y: c_int = 4;
pub const RENDER_PARAM_BLOCK_FOR_TARGET_TIME: c_int = 12;
pub const RENDER_PARAM_SKIP_RENDERING: c_int = 13;
pub const RENDER_API_TYPE_OPENGL: &CStr = c"opengl";

pub type UpdateCallback = unsafe extern "C" fn(context: *mut c_void);

macro_rules! api {
    ($($name:ident: fn($($arg:ty),*) $(-> $ret:ty)?;)*) => {
        /// The resolved entry points.
        pub struct Api {
            $(pub $name: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)*
        }

        impl Api {
            /// # Safety
            /// `library` must be a live handle to a libmpv 2.x dylib, which
            /// is never unloaded.
            unsafe fn resolve(library: *mut c_void) -> Result<Self, String> {
                Ok(Self {
                    $($name: {
                        let symbol = symbol(library, concat!(stringify!($name), "\0"))?;
                        // SAFETY: the symbol is non-null and the signature
                        // above is the one declared in the libmpv headers.
                        std::mem::transmute::<
                            *mut c_void,
                            unsafe extern "C" fn($($arg),*) $(-> $ret)?,
                        >(symbol)
                    },)*
                })
            }
        }
    };
}

api! {
    mpv_create: fn() -> *mut Handle;
    mpv_initialize: fn(*mut Handle) -> c_int;
    mpv_terminate_destroy: fn(*mut Handle);
    mpv_set_option_string: fn(*mut Handle, *const c_char, *const c_char) -> c_int;
    mpv_set_property_string: fn(*mut Handle, *const c_char, *const c_char) -> c_int;
    mpv_get_property_string: fn(*mut Handle, *const c_char) -> *mut c_char;
    mpv_get_property: fn(*mut Handle, *const c_char, c_int, *mut c_void) -> c_int;
    mpv_command: fn(*mut Handle, *mut *const c_char) -> c_int;
    mpv_command_async: fn(*mut Handle, u64, *mut *const c_char) -> c_int;
    mpv_observe_property: fn(*mut Handle, u64, *const c_char, c_int) -> c_int;
    mpv_wait_event: fn(*mut Handle, f64) -> *mut Event;
    mpv_wakeup: fn(*mut Handle);
    mpv_free: fn(*mut c_void);
    mpv_error_string: fn(c_int) -> *const c_char;
    mpv_request_log_messages: fn(*mut Handle, *const c_char) -> c_int;
    mpv_render_context_create: fn(*mut *mut RenderHandle, *mut Handle, *mut RenderParam) -> c_int;
    mpv_render_context_set_update_callback: fn(*mut RenderHandle, Option<UpdateCallback>, *mut c_void);
    mpv_render_context_render: fn(*mut RenderHandle, *mut RenderParam) -> c_int;
    mpv_render_context_free: fn(*mut RenderHandle);
}

extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    pub fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *const c_char;
    fn setlocale(category: c_int, locale: *const c_char) -> *const c_char;
}

pub const RTLD_NOW: c_int = 0x2;
pub const RTLD_LOCAL: c_int = 0x4;
/// macOS value of `LC_NUMERIC`.
const LC_NUMERIC: c_int = 4;

const LIBRARY_NAME: &str = "libmpv.2.dylib";

/// Opens a dynamic library, which then stays loaded for the life of the app.
pub fn open_library(path: &CStr) -> Result<*mut c_void, String> {
    // SAFETY: the path is NUL-terminated. Loading runs the library's
    // initializers, so callers only pass trusted paths.
    let library = unsafe { dlopen(path.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
    if !library.is_null() {
        return Ok(library);
    }
    // SAFETY: dlerror returns null or a NUL-terminated string that stays valid
    // until the next dl call on this thread; it is copied at once.
    unsafe {
        let message = dlerror();
        Err(if message.is_null() {
            "unknown loader error".to_owned()
        } else {
            CStr::from_ptr(message).to_string_lossy().into_owned()
        })
    }
}

/// # Safety
/// `library` must be a live dlopen handle and `name` must end in a NUL.
unsafe fn symbol(library: *mut c_void, name: &str) -> Result<*mut c_void, String> {
    let found = dlsym(library, name.as_ptr().cast());
    if found.is_null() {
        Err(format!("libmpv is missing {}", name.trim_end_matches('\0')))
    } else {
        Ok(found)
    }
}

/// Where libmpv may be loaded from, in order. Release builds only ever look
/// inside the app bundle (`Contents/MacOS/popcorn` -> `Contents/Frameworks`).
fn candidates() -> Vec<PathBuf> {
    let bundled = std::env::current_exe().ok().and_then(|exe| {
        Some(
            exe.parent()?
                .parent()?
                .join("Frameworks")
                .join(LIBRARY_NAME),
        )
    });
    #[cfg(not(debug_assertions))]
    {
        bundled.into_iter().collect()
    }
    #[cfg(debug_assertions)]
    {
        std::env::var_os("POPCORN_LIBMPV")
            .map(PathBuf::from)
            .into_iter()
            .chain(bundled)
            .chain([PathBuf::from("/opt/homebrew/lib").join(LIBRARY_NAME)])
            .collect()
    }
}

fn load() -> Result<Api, String> {
    let mut problem = format!("{LIBRARY_NAME} was not found");
    for path in candidates() {
        if !path.is_file() {
            continue;
        }
        let Ok(c_path) = CString::new(path.as_os_str().as_encoded_bytes()) else {
            continue;
        };
        // The candidates are the app bundle or, in debug builds, a copy the
        // developer chose.
        match open_library(&c_path) {
            Ok(library) => {
                // SAFETY: `library` was just opened and is never closed.
                let api = unsafe { Api::resolve(library) }?;
                log::info!("loaded {}", path.display());
                return Ok(api);
            }
            Err(error) => problem = error,
        }
    }
    Err(problem)
}

/// Loads libmpv once. A failure is not cached, so it can be retried.
pub fn api() -> Result<&'static Api, String> {
    static API: OnceLock<Api> = OnceLock::new();
    if let Some(api) = API.get() {
        return Ok(api);
    }
    let loaded = load()?;
    // SAFETY: mpv requires the C numeric locale. The argument is a valid C
    // string, and "C" is the default every program starts with.
    unsafe { setlocale(LC_NUMERIC, c"C".as_ptr()) };
    Ok(API.get_or_init(|| loaded))
}
