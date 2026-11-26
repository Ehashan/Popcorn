//! mpv's OpenGL render API on our own CGL context. No AppKit in here, so it
//! can be exercised without a window.
//!
//! Lock order, everywhere: the CGL context lock, then `render`. Core Animation
//! may already hold the context lock when it asks us to draw.

use std::ffi::{c_char, c_int, c_void};
use std::ptr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};

use super::ffi;
use super::mpv::Mpv;
use crate::error::{AppError, AppResult};
use crate::state::lock;

pub type CglContext = *mut c_void;
pub type CglPixelFormat = *mut c_void;

#[link(name = "OpenGL", kind = "framework")]
extern "C" {
    fn CGLChoosePixelFormat(
        attributes: *const c_int,
        format: *mut CglPixelFormat,
        count: *mut c_int,
    ) -> c_int;
    fn CGLCreateContext(
        format: CglPixelFormat,
        share: CglContext,
        context: *mut CglContext,
    ) -> c_int;
    pub fn CGLRetainContext(context: CglContext) -> CglContext;
    fn CGLReleaseContext(context: CglContext);
    pub fn CGLRetainPixelFormat(format: CglPixelFormat) -> CglPixelFormat;
    fn CGLReleasePixelFormat(format: CglPixelFormat);
    fn CGLGetCurrentContext() -> CglContext;
    fn CGLSetCurrentContext(context: CglContext) -> c_int;
    fn CGLLockContext(context: CglContext) -> c_int;
    fn CGLUnlockContext(context: CglContext) -> c_int;
    fn glGetIntegerv(name: u32, value: *mut c_int);
    fn glFlush();
}

const ATTRIBUTE_ACCELERATED: c_int = 73;
const ATTRIBUTE_DOUBLE_BUFFER: c_int = 5;
const ATTRIBUTE_OFFLINE_RENDERERS: c_int = 96;
const ATTRIBUTE_AUTOMATIC_SWITCHING: c_int = 101;
const ATTRIBUTE_PROFILE: c_int = 99;
const PROFILE_3_2_CORE: c_int = 0x3200;

const GL_VIEWPORT: u32 = 0x0BA2;
const GL_DRAW_FRAMEBUFFER_BINDING: u32 = 0x8CA6;
const RTLD_DEFAULT: *mut c_void = -2_isize as *mut c_void;

/// How mpv's update callback wakes the render thread.
#[derive(Default)]
pub struct Signal {
    state: Mutex<SignalState>,
    wake: Condvar,
}

#[derive(Default)]
struct SignalState {
    frame: bool,
    stop: bool,
}

impl Signal {
    fn frame(&self) {
        lock(&self.state).frame = true;
        self.wake.notify_one();
    }

    pub fn stop(&self) {
        lock(&self.state).stop = true;
        self.wake.notify_all();
    }

    /// Blocks until there is something to draw. False means stop.
    pub fn wait(&self) -> bool {
        let mut state = lock(&self.state);
        while !state.frame && !state.stop {
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        state.frame = false;
        !state.stop
    }
}

unsafe extern "C" fn on_update(context: *mut c_void) {
    // SAFETY: `context` is the `Signal` owned by the renderer, which removes
    // this callback and frees the render context before the signal drops. mpv
    // forbids calling back into it from here, and we do not.
    if let Some(signal) = context.cast::<Signal>().as_ref() {
        signal.frame();
    }
}

unsafe extern "C" fn proc_address(_: *mut c_void, name: *const c_char) -> *mut c_void {
    // SAFETY: mpv passes a NUL-terminated symbol name. OpenGL.framework is
    // linked, so its functions are found in the default search order.
    ffi::dlsym(RTLD_DEFAULT, name)
}

pub struct Renderer {
    /// Keeps the core alive until the render context is gone.
    mpv: Arc<Mpv>,
    format: CglPixelFormat,
    context: CglContext,
    /// Null once shut down.
    render: Mutex<*mut ffi::RenderHandle>,
    signal: Box<Signal>,
    draws: AtomicU64,
    errors: AtomicU64,
}

// SAFETY: the render context pointer is only used while holding both locks
// described at the top of this file, so mpv_render_* calls never overlap and
// always run with this CGL context current. CGL objects may be used from any
// thread under the context lock.
unsafe impl Send for Renderer {}
// SAFETY: as above.
unsafe impl Sync for Renderer {}

fn choose_format() -> AppResult<CglPixelFormat> {
    let attributes = [
        ATTRIBUTE_PROFILE,
        PROFILE_3_2_CORE,
        ATTRIBUTE_ACCELERATED,
        ATTRIBUTE_DOUBLE_BUFFER,
        ATTRIBUTE_OFFLINE_RENDERERS,
        ATTRIBUTE_AUTOMATIC_SWITCHING,
        0,
    ];
    let mut format = ptr::null_mut();
    let mut count = 0;
    // SAFETY: the attribute list is zero-terminated and both outputs are valid.
    let code = unsafe { CGLChoosePixelFormat(attributes.as_ptr(), &mut format, &mut count) };
    if code != 0 || format.is_null() {
        return Err(AppError::Player(format!(
            "no usable OpenGL pixel format ({code})"
        )));
    }
    Ok(format)
}

impl Renderer {
    /// Must be called before a file is loaded, or mpv has nowhere to draw.
    pub fn new(mpv: Arc<Mpv>) -> AppResult<Arc<Self>> {
        let format = choose_format()?;
        let mut context = ptr::null_mut();
        // SAFETY: `format` is valid and `context` is a valid output.
        let code = unsafe { CGLCreateContext(format, ptr::null_mut(), &mut context) };
        if code != 0 || context.is_null() {
            // SAFETY: we own the only reference to `format`.
            unsafe { CGLReleasePixelFormat(format) };
            return Err(AppError::Player(format!(
                "could not create an OpenGL context ({code})"
            )));
        }
        // From here on, dropping `renderer` releases everything.
        let renderer = Self {
            mpv,
            format,
            context,
            render: Mutex::new(ptr::null_mut()),
            signal: Box::default(),
            draws: AtomicU64::new(0),
            errors: AtomicU64::new(0),
        };
        renderer.create_render_context()?;
        Ok(Arc::new(renderer))
    }

    fn create_render_context(&self) -> AppResult<()> {
        let mut init = ffi::OpenGlInitParams {
            get_proc_address: proc_address,
            get_proc_address_ctx: ptr::null_mut(),
        };
        let mut params = [
            param(
                ffi::RENDER_PARAM_API_TYPE,
                ffi::RENDER_API_TYPE_OPENGL.as_ptr().cast_mut().cast(),
            ),
            param(
                ffi::RENDER_PARAM_OPENGL_INIT_PARAMS,
                ptr::from_mut(&mut init).cast(),
            ),
            param(ffi::RENDER_PARAM_INVALID, ptr::null_mut()),
        ];
        self.with_context(|render| {
            let api = self.mpv.api();
            // SAFETY: the CGL context is current, the handle is live, and the
            // parameter array is terminated and outlives the call.
            let code = unsafe {
                (api.mpv_render_context_create)(render, self.mpv.raw(), params.as_mut_ptr())
            };
            self.mpv.check(code)?;
            // SAFETY: the signal outlives the render context (see `shutdown`).
            unsafe {
                (api.mpv_render_context_set_update_callback)(
                    *render,
                    Some(on_update),
                    ptr::from_ref::<Signal>(&self.signal).cast_mut().cast(),
                );
            }
            Ok(())
        })
    }

    pub fn signal(&self) -> &Signal {
        &self.signal
    }

    pub fn pixel_format(&self) -> CglPixelFormat {
        self.format
    }

    pub fn context(&self) -> CglContext {
        self.context
    }

    /// How many frames have been rendered successfully.
    pub fn draws(&self) -> u64 {
        self.draws.load(Ordering::Relaxed)
    }

    pub fn errors(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }

    /// Runs with our context locked and current, then restores the previous one.
    fn with_context<T>(&self, run: impl FnOnce(&mut *mut ffi::RenderHandle) -> T) -> T {
        // SAFETY: the context is valid for the lifetime of `self`. The CGL
        // lock is recursive, and every call is paired below.
        let previous = unsafe {
            CGLLockContext(self.context);
            let previous = CGLGetCurrentContext();
            CGLSetCurrentContext(self.context);
            previous
        };
        let result = run(&mut lock(&self.render));
        // SAFETY: undoes the calls above, on the same thread.
        unsafe {
            CGLSetCurrentContext(previous);
            CGLUnlockContext(self.context);
        }
        result
    }

    /// Draws the current frame into a framebuffer of the given size.
    #[cfg(test)]
    pub fn render(&self, framebuffer: c_int, width: c_int, height: c_int, wait: bool) {
        self.with_context(|render| self.render_locked(*render, framebuffer, width, height, wait));
    }

    /// Draws into whatever framebuffer and viewport the caller has set up.
    pub fn render_to_current(&self, wait: bool) {
        self.with_context(|render| {
            let mut viewport = [0; 4];
            let mut framebuffer = 0;
            // SAFETY: the context is current; the outputs have the sizes that
            // these two queries write (four and one integers).
            unsafe {
                glGetIntegerv(GL_VIEWPORT, viewport.as_mut_ptr());
                glGetIntegerv(GL_DRAW_FRAMEBUFFER_BINDING, &mut framebuffer);
            }
            self.render_locked(*render, framebuffer, viewport[2], viewport[3], wait);
        });
    }

    fn render_locked(
        &self,
        render: *mut ffi::RenderHandle,
        framebuffer: c_int,
        width: c_int,
        height: c_int,
        wait: bool,
    ) {
        if render.is_null() || width <= 0 || height <= 0 {
            return;
        }
        let mut target = ffi::OpenGlFbo {
            fbo: framebuffer,
            w: width,
            h: height,
            internal_format: 0,
        };
        let mut flip: c_int = 1;
        let mut block: c_int = c_int::from(wait);
        let mut params = [
            param(
                ffi::RENDER_PARAM_OPENGL_FBO,
                ptr::from_mut(&mut target).cast(),
            ),
            param(ffi::RENDER_PARAM_FLIP_Y, ptr::from_mut(&mut flip).cast()),
            param(
                ffi::RENDER_PARAM_BLOCK_FOR_TARGET_TIME,
                ptr::from_mut(&mut block).cast(),
            ),
            param(ffi::RENDER_PARAM_INVALID, ptr::null_mut()),
        ];
        // SAFETY: the render context is live and locked, its GL context is
        // current, and every parameter outlives the call.
        let code = unsafe {
            let code = (self.mpv.api().mpv_render_context_render)(render, params.as_mut_ptr());
            glFlush();
            code
        };
        let counter = if code >= 0 { &self.draws } else { &self.errors };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Consumes a frame without drawing it, for when the view is not on
    /// screen. Otherwise mpv would wait for us and fall behind.
    pub fn skip(&self) {
        let mut skip: c_int = 1;
        let mut params = [
            param(
                ffi::RENDER_PARAM_SKIP_RENDERING,
                ptr::from_mut(&mut skip).cast(),
            ),
            param(ffi::RENDER_PARAM_INVALID, ptr::null_mut()),
        ];
        self.with_context(|render| {
            if !render.is_null() {
                // SAFETY: as in `render_locked`.
                unsafe {
                    (self.mpv.api().mpv_render_context_render)(*render, params.as_mut_ptr());
                }
            }
        });
    }

    /// Frees the render context. Later draws do nothing. Safe to repeat.
    pub fn shutdown(&self) {
        self.signal.stop();
        self.with_context(|render| {
            if !render.is_null() {
                let api = self.mpv.api();
                // SAFETY: both locks are held, so nothing is rendering, and the
                // pointer is cleared so it can never be used again.
                unsafe {
                    (api.mpv_render_context_set_update_callback)(*render, None, ptr::null_mut());
                    (api.mpv_render_context_free)(*render);
                }
                *render = ptr::null_mut();
            }
        });
    }
}

fn param(kind: c_int, data: *mut c_void) -> ffi::RenderParam {
    ffi::RenderParam { kind, data }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        // Runs before the fields drop, so the core is still alive.
        self.shutdown();
        // SAFETY: we release the references we created. Anyone else, such as
        // Core Animation, holds its own.
        unsafe {
            CGLReleaseContext(self.context);
            CGLReleasePixelFormat(self.format);
        }
    }
}
