//! Where the video appears on macOS.
//!
//! Both ways use a `CAOpenGLLayer` drawn through mpv's render API:
//! * embedded: a view in the main window, below the transparent `WKWebView`;
//! * fallback: our own borderless window, whose keys go to mpv's bindings.
//!
//! (mpv's `wid` option and its own window both need Vulkan on macOS, which
//! the slim build leaves out.)
//!
//! All AppKit calls happen on the main thread. Drawing happens on a render
//! thread per surface, which Core Animation allows for layers.

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CStr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use super::gl::{self, CglContext, CglPixelFormat, Renderer};
use super::mpv::Mpv;
use super::objc::{self, msg, Id, Method, Rect, Sel, NIL};
use crate::error::{AppError, AppResult};
use crate::state::lock;

#[link(name = "QuartzCore", kind = "framework")]
extern "C" {}

#[link(name = "AppKit", kind = "framework")]
extern "C" {}

extern "C" {
    fn pthread_main_np() -> c_int;
}

const MAIN_THREAD_TIMEOUT: Duration = Duration::from_secs(5);
/// `NSViewWidthSizable | NSViewHeightSizable`
const AUTORESIZE_BOTH: usize = 2 | 16;
/// `NSWindowBelow`
const ORDER_BELOW: isize = -1;
const STYLE_BORDERLESS: usize = 0;
const BACKING_BUFFERED: usize = 2;
/// `NSWindowCollectionBehaviorFullScreenAuxiliary`
const FULLSCREEN_AUXILIARY: usize = 1 << 8;
/// `NSApplicationPresentationAutoHideDock | ...AutoHideMenuBar`
const HIDE_DOCK_AND_MENU: usize = 1 | 4;
/// `NSActivityUserInitiated | NSActivityIdleDisplaySleepDisabled`
const KEEP_DISPLAY_AWAKE: u64 = 0x00FF_FFFF | (1 << 40);

/// Receives key presses in the fallback window: key code, characters,
/// modifier flags. Called on the main thread, so it must not block.
pub type KeyHandler = Arc<dyn Fn(u16, &str, usize) + Send + Sync>;

static KEY_HANDLER: Mutex<Option<KeyHandler>> = Mutex::new(None);

/// Which renderer each of our layers draws with.
fn layers() -> &'static Mutex<HashMap<usize, Arc<Renderer>>> {
    static LAYERS: OnceLock<Mutex<HashMap<usize, Arc<Renderer>>>> = OnceLock::new();
    LAYERS.get_or_init(Mutex::default)
}

fn renderer_of(layer: Id) -> Option<Arc<Renderer>> {
    lock(layers()).get(&(layer as usize)).cloned()
}

/// Forgets a layer's renderer. The renderer is dropped only after the map is
/// unlocked: dropping it takes the CGL lock, which a drawing thread may hold
/// while it waits for the map.
fn unregister(layer: usize) {
    let removed = lock(layers()).remove(&layer);
    drop(removed);
}

fn is_main_thread() -> bool {
    // SAFETY: no preconditions.
    unsafe { pthread_main_np() != 0 }
}

/// Runs on the main thread and waits for the result. Never blocks forever: if
/// the main thread does not answer in time, this fails instead.
pub fn on_main<T: Send + 'static>(
    app: &AppHandle,
    run: impl FnOnce() -> T + Send + 'static,
) -> AppResult<T> {
    if is_main_thread() {
        return Ok(objc::autorelease_pool(run));
    }
    let (sender, receiver) = mpsc::channel();
    app.run_on_main_thread(move || {
        // The receiver is only gone if we gave up waiting.
        let _ = sender.send(objc::autorelease_pool(run));
    })?;
    receiver
        .recv_timeout(MAIN_THREAD_TIMEOUT)
        .map_err(|_| AppError::Player("the window did not respond".into()))
}

// ---- The layer class -----------------------------------------------------

extern "C" fn can_draw(
    layer: Id,
    _: Sel,
    _: CglContext,
    _: CglPixelFormat,
    _: f64,
    _: *const c_void,
) -> bool {
    renderer_of(layer).is_some()
}

extern "C" fn draw(layer: Id, _: Sel, _: CglContext, _: CglPixelFormat, _: f64, _: *const c_void) {
    if let Some(renderer) = renderer_of(layer) {
        // Core Animation also draws on the main thread while resizing, which
        // must not wait for the frame's display time.
        renderer.render_to_current(!is_main_thread());
    }
}

extern "C" fn copy_pixel_format(layer: Id, _: Sel, _: u32) -> CglPixelFormat {
    match renderer_of(layer) {
        // SAFETY: the format is live; the layer releases the added reference.
        Some(renderer) => unsafe { gl::CGLRetainPixelFormat(renderer.pixel_format()) },
        None => std::ptr::null_mut(),
    }
}

extern "C" fn copy_context(layer: Id, _: Sel, _: CglPixelFormat) -> CglContext {
    match renderer_of(layer) {
        // SAFETY: the context is live; the layer releases the added reference.
        Some(renderer) => unsafe { gl::CGLRetainContext(renderer.context()) },
        None => std::ptr::null_mut(),
    }
}

fn layer_class() -> Id {
    static CLASS: OnceLock<usize> = OnceLock::new();
    *CLASS.get_or_init(|| {
        let methods = [
            Method {
                name: c"canDrawInCGLContext:pixelFormat:forLayerTime:displayTime:",
                types: c"B@:^v^vd^v",
                imp: can_draw as *const c_void,
            },
            Method {
                name: c"drawInCGLContext:pixelFormat:forLayerTime:displayTime:",
                types: c"v@:^v^vd^v",
                imp: draw as *const c_void,
            },
            Method {
                name: c"copyCGLPixelFormatForDisplayMask:",
                types: c"^v@:I",
                imp: copy_pixel_format as *const c_void,
            },
            Method {
                name: c"copyCGLContextForPixelFormat:",
                types: c"^v@:^v",
                imp: copy_context as *const c_void,
            },
        ];
        // SAFETY: each function above matches the CAOpenGLLayer method it
        // overrides (CFTimeInterval is f64, the other arguments are pointers
        // or a u32 mask).
        unsafe { objc::subclass(c"CAOpenGLLayer", c"PopcornVideoLayer", &methods) as usize }
    }) as Id
}

// ---- The fallback window class -------------------------------------------

extern "C" fn yes(_: Id, _: Sel) -> bool {
    true
}

extern "C" fn key_down(_: Id, _: Sel, event: Id) {
    // SAFETY: AppKit calls keyDown: on the main thread with a live key event;
    // these three getters are valid on key events. `UTF8String` returns a
    // string that lives as long as the autorelease pool, and is copied.
    let (code, characters, modifiers) = unsafe {
        let code = msg!(event, c"keyCode", u16);
        let modifiers = msg!(event, c"modifierFlags", usize);
        let text = msg!(event, c"charactersIgnoringModifiers", Id);
        let utf8 = msg!(text, c"UTF8String", *const c_char);
        let characters = if utf8.is_null() {
            String::new()
        } else {
            CStr::from_ptr(utf8).to_string_lossy().into_owned()
        };
        (code, characters, modifiers)
    };
    let handler = lock(&KEY_HANDLER).clone();
    if let Some(handler) = handler {
        handler(code, &characters, modifiers);
    }
}

fn window_class() -> Id {
    static CLASS: OnceLock<usize> = OnceLock::new();
    *CLASS.get_or_init(|| {
        let methods = [
            // Borderless windows refuse keyboard focus unless they say so.
            Method {
                name: c"canBecomeKeyWindow",
                types: c"B@:",
                imp: yes as *const c_void,
            },
            Method {
                name: c"canBecomeMainWindow",
                types: c"B@:",
                imp: yes as *const c_void,
            },
            Method {
                name: c"keyDown:",
                types: c"v@:@",
                imp: key_down as *const c_void,
            },
        ];
        // SAFETY: the functions match the NSWindow methods they override.
        unsafe { objc::subclass(c"NSWindow", c"PopcornVideoWindow", &methods) as usize }
    }) as Id
}

// ---- Views ---------------------------------------------------------------

/// A view that shows `renderer` through a video layer. Both objects are
/// returned with one reference each, which the caller releases.
///
/// # Safety
/// Must run on the main thread.
unsafe fn video_view(renderer: &Arc<Renderer>, frame: Rect, scale: f64) -> AppResult<(Id, Id)> {
    let class = layer_class();
    let layer = msg!(msg!(class, c"alloc", Id), c"init", Id);
    if layer.is_null() {
        return Err(AppError::Player("could not create the video layer".into()));
    }
    // Registered before the layer is shown, which is when it asks for its
    // OpenGL context.
    lock(layers()).insert(layer as usize, Arc::clone(renderer));
    msg!(layer, c"setAsynchronous:", (); false => bool);
    msg!(layer, c"setNeedsDisplayOnBoundsChange:", (); true => bool);
    msg!(layer, c"setContentsScale:", (); scale => f64);

    let view =
        msg!(msg!(objc::class(c"NSView"), c"alloc", Id), c"initWithFrame:", Id; frame => Rect);
    if view.is_null() {
        unregister(layer as usize);
        msg!(layer, c"release", ());
        return Err(AppError::Player("could not create the video view".into()));
    }
    // Setting the layer before wantsLayer makes this a layer-hosting view.
    msg!(view, c"setLayer:", (); layer => Id);
    msg!(view, c"setWantsLayer:", (); true => bool);
    msg!(view, c"setAutoresizingMask:", (); AUTORESIZE_BOTH => usize);
    Ok((view, layer))
}

/// Draws whenever mpv has a new frame, until the renderer's signal stops.
fn spawn_render_thread(renderer: Arc<Renderer>, layer: Id) {
    // SAFETY: the layer is live here; this reference belongs to the thread.
    unsafe { msg!(layer, c"retain", Id) };
    let layer = layer as usize;
    thread::spawn(move || {
        let layer = layer as Id;
        while renderer.signal().wait() {
            let before = renderer.draws() + renderer.errors();
            objc::autorelease_pool(|| {
                let transaction = objc::class(c"CATransaction");
                // SAFETY: the thread holds a reference to the layer, and
                // CATransaction and CALayer display may be used off the main
                // thread inside an explicit transaction.
                unsafe {
                    msg!(transaction, c"begin", ());
                    msg!(transaction, c"setDisableActions:", (); true => bool);
                    msg!(layer, c"display", ());
                    msg!(transaction, c"commit", ());
                    msg!(transaction, c"flush", ());
                }
            });
            if renderer.draws() + renderer.errors() == before {
                renderer.skip();
            }
        }
        // SAFETY: releases the reference taken above.
        unsafe { msg!(layer, c"release", ()) };
    });
}

#[derive(Clone, Copy)]
enum Kind {
    Embedded {
        webview: usize,
        /// The webview's own under-page colour, retained, to put back.
        under_page: usize,
    },
    Window {
        window: usize,
        /// The app's presentation options before we changed them.
        presentation: usize,
        screen_frame: Rect,
    },
}

/// The AppKit objects of one surface. Pointers are kept as integers so the
/// struct can travel to the main thread, the only place they are used.
#[derive(Clone, Copy)]
struct Parts {
    view: usize,
    layer: usize,
    kind: Kind,
}

/// A handle to a surface. Clones refer to the same AppKit objects; after one
/// of them closes the surface, the others do nothing.
#[derive(Clone)]
pub struct VideoSurface {
    renderer: Arc<Renderer>,
    parts: Parts,
    windowed: Arc<AtomicBool>,
}

impl VideoSurface {
    pub fn is_embedded(&self) -> bool {
        matches!(self.parts.kind, Kind::Embedded { .. })
    }

    /// Puts the video into the main window, underneath the webview.
    pub fn embed(app: &AppHandle, mpv: &Arc<Mpv>) -> AppResult<Self> {
        let renderer = Renderer::new(Arc::clone(mpv))?;
        match Self::attach(app, &renderer) {
            Ok(parts) => Ok(Self::start(renderer, parts)),
            Err(error) => {
                // mpv allows one render context per core, and the fallback
                // is about to create another.
                renderer.shutdown();
                Err(error)
            }
        }
    }

    fn attach(app: &AppHandle, renderer: &Arc<Renderer>) -> AppResult<Parts> {
        #[cfg(debug_assertions)]
        if std::env::var_os("POPCORN_FORCE_FALLBACK").is_some() {
            return Err(AppError::Player("POPCORN_FORCE_FALLBACK is set".into()));
        }
        let webview = app
            .get_webview_window("main")
            .ok_or_else(|| AppError::Player("the main window is gone".into()))?;
        let (sender, receiver) = mpsc::channel();
        let for_main = Arc::clone(renderer);
        webview.with_webview(move |platform| {
            let webview = platform.inner().cast::<c_void>();
            // SAFETY: with_webview runs on the main thread and `inner` is the
            // live WKWebView.
            let result = objc::autorelease_pool(|| unsafe { insert_below(&for_main, webview) });
            if let Err(mpsc::SendError(Ok(parts))) = sender.send(result) {
                // We gave up waiting; do not leave a stray view behind.
                // SAFETY: still on the main thread.
                unsafe { remove(parts) };
            }
        })?;
        receiver
            .recv_timeout(MAIN_THREAD_TIMEOUT)
            .map_err(|_| AppError::Player("the window did not respond".into()))?
    }

    /// Opens the fallback: a borderless full-screen window of our own.
    pub fn open_window(app: &AppHandle, mpv: &Arc<Mpv>, keys: KeyHandler) -> AppResult<Self> {
        let renderer = Renderer::new(Arc::clone(mpv))?;
        let main_window = app
            .get_webview_window("main")
            .and_then(|window| window.ns_window().ok())
            .map(|pointer| pointer as usize);
        *lock(&KEY_HANDLER) = Some(keys);
        let for_main = Arc::clone(&renderer);
        let parts = on_main(app, move || {
            // SAFETY: on the main thread; the pointer is the live main window.
            unsafe { create_window(&for_main, main_window.map_or(NIL, |w| w as Id)) }
        })??;
        Ok(Self::start(renderer, parts))
    }

    fn start(renderer: Arc<Renderer>, parts: Parts) -> Self {
        spawn_render_thread(Arc::clone(&renderer), parts.layer as Id);
        Self {
            renderer,
            parts,
            windowed: Arc::default(),
        }
    }

    /// Toggles the fallback window between full screen and a smaller window.
    /// Returns true for full screen. Does nothing for an embedded surface.
    pub fn toggle_window_fullscreen(&self, app: &AppHandle) -> AppResult<bool> {
        let windowed = !self.windowed.load(Ordering::SeqCst);
        self.set_window_fullscreen(app, !windowed)?;
        Ok(!windowed)
    }

    pub fn is_window_fullscreen(&self) -> bool {
        !self.windowed.load(Ordering::SeqCst)
    }

    pub fn set_window_fullscreen(&self, app: &AppHandle, fullscreen: bool) -> AppResult<()> {
        let Kind::Window {
            window,
            presentation,
            screen_frame,
        } = self.parts.kind
        else {
            return Ok(());
        };
        self.windowed.store(!fullscreen, Ordering::SeqCst);
        let state = Arc::clone(&self.windowed);
        let layer = self.parts.layer;
        on_main(app, move || {
            // The surface may have been removed while this was queued.
            if !lock(layers()).contains_key(&layer) {
                return;
            }
            state.store(!fullscreen, Ordering::SeqCst);
            let frame = if fullscreen {
                screen_frame
            } else {
                Rect {
                    x: screen_frame.x + screen_frame.width * 0.2,
                    y: screen_frame.y + screen_frame.height * 0.2,
                    width: screen_frame.width * 0.6,
                    height: screen_frame.height * 0.6,
                }
            };
            let options = if fullscreen {
                HIDE_DOCK_AND_MENU
            } else {
                presentation
            };
            // SAFETY: on the main thread, and the window is live because its
            // layer is still registered (removal also runs on this thread).
            unsafe {
                let window = window as Id;
                msg!(window, c"setFrame:display:", (); frame => Rect, true => bool);
                msg!(window, c"setMovableByWindowBackground:", (); !fullscreen => bool);
                set_presentation(options);
            }
        })
    }

    /// Tears the surface down. The render context is freed first, so mpv can
    /// be shut down afterwards even if the main thread does not answer.
    pub fn close(self, app: &AppHandle) {
        self.renderer.shutdown();
        if matches!(self.parts.kind, Kind::Window { .. }) {
            *lock(&KEY_HANDLER) = None;
        }
        let parts = self.parts;
        // SAFETY: `on_main` runs this on the main thread. Only the session's
        // own handle is ever closed, so the objects are removed once.
        if let Err(error) = on_main(app, move || unsafe { remove(parts) }) {
            log::warn!("could not remove the video view: {error}");
        }
    }

    /// Sends a synthetic key press to the fallback window, for the smoke test.
    #[cfg(debug_assertions)]
    pub fn press_key(&self, app: &AppHandle, key_code: u16, characters: &'static CStr) {
        let Kind::Window { window, .. } = self.parts.kind else {
            return;
        };
        let layer = self.parts.layer;
        let sent = on_main(app, move || {
            if lock(layers()).contains_key(&layer) {
                // SAFETY: on the main thread, and the window is live because
                // its layer is still registered.
                unsafe { send_key(window as Id, key_code, characters) };
            }
        });
        if let Err(error) = sent {
            log::warn!("could not send a key: {error}");
        }
    }

    /// Facts about the live surface, for the debug smoke test.
    #[cfg(debug_assertions)]
    pub fn describe(&self, app: &AppHandle) -> String {
        let parts = self.parts;
        let stats = format!(
            "draws={} render_errors={}",
            self.renderer.draws(),
            self.renderer.errors()
        );
        let shape = on_main(app, move || {
            // SAFETY: on the main thread with a live surface.
            unsafe { describe(parts) }
        });
        format!("{stats} {}", shape.unwrap_or_default())
    }
}

/// # Safety
/// Must run on the main thread; `webview` must be the live `WKWebView`.
unsafe fn insert_below(renderer: &Arc<Renderer>, webview: Id) -> AppResult<Parts> {
    let parent = msg!(webview, c"superview", Id);
    let window = msg!(webview, c"window", Id);
    if parent.is_null() || window.is_null() {
        return Err(AppError::Player("the webview is not in a window".into()));
    }
    let frame = msg!(parent, c"bounds", Rect);
    let scale = msg!(window, c"backingScaleFactor", f64);
    let (view, layer) = video_view(renderer, frame, scale)?;
    msg!(parent, c"setWantsLayer:", (); true => bool);
    msg!(parent, c"addSubview:positioned:relativeTo:", ();
        view => Id, ORDER_BELOW => isize, webview => Id);

    // The webview already draws no background of its own (the window has a
    // background colour set), so transparent HTML shows the video. Its
    // under-page colour is cleared too while the video is there.
    let under_page = msg!(webview, c"underPageBackgroundColor", Id);
    msg!(under_page, c"retain", Id);
    let clear = msg!(objc::class(c"NSColor"), c"clearColor", Id);
    msg!(webview, c"setUnderPageBackgroundColor:", (); clear => Id);
    msg!(webview, c"retain", Id);

    Ok(Parts {
        view: view as usize,
        layer: layer as usize,
        kind: Kind::Embedded {
            webview: webview as usize,
            under_page: under_page as usize,
        },
    })
}

/// # Safety
/// Must run on the main thread.
unsafe fn set_presentation(options: usize) {
    let app = msg!(objc::class(c"NSApplication"), c"sharedApplication", Id);
    msg!(app, c"setPresentationOptions:", (); options => usize);
}

/// # Safety
/// Must run on the main thread; `main_window` must be nil or a live window.
unsafe fn create_window(renderer: &Arc<Renderer>, main_window: Id) -> AppResult<Parts> {
    let mut screen = msg!(main_window, c"screen", Id);
    if screen.is_null() {
        screen = msg!(objc::class(c"NSScreen"), c"mainScreen", Id);
    }
    if screen.is_null() {
        return Err(AppError::Player("there is no screen to play on".into()));
    }
    let frame = msg!(screen, c"frame", Rect);
    let scale = msg!(screen, c"backingScaleFactor", f64);

    let window = msg!(msg!(window_class(), c"alloc", Id),
        c"initWithContentRect:styleMask:backing:defer:", Id;
        frame => Rect, STYLE_BORDERLESS => usize, BACKING_BUFFERED => usize, false => bool);
    if window.is_null() {
        return Err(AppError::Player("could not create the video window".into()));
    }
    let (view, layer) = match video_view(renderer, frame, scale) {
        Ok(created) => created,
        Err(error) => {
            msg!(window, c"release", ());
            return Err(error);
        }
    };
    msg!(window, c"setReleasedWhenClosed:", (); false => bool);
    let black = msg!(objc::class(c"NSColor"), c"blackColor", Id);
    msg!(window, c"setBackgroundColor:", (); black => Id);
    msg!(window, c"setTitle:", (); objc::ns_string(c"Popcorn") => Id);
    msg!(window, c"setCollectionBehavior:", (); FULLSCREEN_AUXILIARY => usize);
    msg!(window, c"setContentView:", (); view => Id);

    let app = msg!(objc::class(c"NSApplication"), c"sharedApplication", Id);
    let presentation = msg!(app, c"presentationOptions", usize);
    set_presentation(HIDE_DOCK_AND_MENU);
    msg!(window, c"makeKeyAndOrderFront:", (); NIL => Id);

    Ok(Parts {
        view: view as usize,
        layer: layer as usize,
        kind: Kind::Window {
            window: window as usize,
            presentation,
            screen_frame: frame,
        },
    })
}

/// # Safety
/// Must run on the main thread, once per `Parts`.
unsafe fn remove(parts: Parts) {
    let (view, layer) = (parts.view as Id, parts.layer as Id);
    unregister(parts.layer);
    match parts.kind {
        Kind::Embedded {
            webview,
            under_page,
        } => {
            let (webview, under_page) = (webview as Id, under_page as Id);
            msg!(webview, c"setUnderPageBackgroundColor:", (); under_page => Id);
            msg!(under_page, c"release", ());
            msg!(webview, c"release", ());
            msg!(view, c"removeFromSuperview", ());
        }
        Kind::Window {
            window,
            presentation,
            ..
        } => {
            let window = window as Id;
            set_presentation(presentation);
            msg!(window, c"orderOut:", (); NIL => Id);
            msg!(window, c"close", ());
            msg!(window, c"release", ());
        }
    }
    msg!(view, c"release", ());
    msg!(layer, c"release", ());
}

/// # Safety
/// Must run on the main thread with a live window.
#[cfg(debug_assertions)]
unsafe fn send_key(window: Id, key_code: u16, characters: &CStr) {
    /// `NSEventTypeKeyDown`
    const KEY_DOWN: usize = 10;
    #[repr(C)]
    struct Point {
        x: f64,
        y: f64,
    }
    let text = objc::ns_string(characters);
    let number = msg!(window, c"windowNumber", isize);
    let event = msg!(objc::class(c"NSEvent"),
        c"keyEventWithType:location:modifierFlags:timestamp:windowNumber:context:characters:charactersIgnoringModifiers:isARepeat:keyCode:", Id;
        KEY_DOWN => usize, Point { x: 0.0, y: 0.0 } => Point, 0 => usize, 0.0 => f64,
        number => isize, NIL => Id, text => Id, text => Id, false => bool, key_code => u16);
    msg!(window, c"sendEvent:", (); event => Id);
}

/// # Safety
/// Must run on the main thread with a live surface.
#[cfg(debug_assertions)]
unsafe fn describe(parts: Parts) -> String {
    let view = parts.view as Id;
    let parent = msg!(view, c"superview", Id);
    let window = msg!(view, c"window", Id);
    let frame = msg!(view, c"frame", Rect);
    let visible = msg!(window, c"isVisible", bool);
    let layer_matches = msg!(view, c"layer", Id) == parts.layer as Id;
    let mut text = format!(
        "view_in_window={} window_visible={visible} hosts_our_layer={layer_matches} frame={frame:?}",
        !window.is_null()
    );
    if let Kind::Embedded { webview, .. } = parts.kind {
        let siblings = msg!(parent, c"subviews", Id);
        let ours = msg!(siblings, c"indexOfObject:", usize; view => Id);
        let web = msg!(siblings, c"indexOfObject:", usize; webview as Id => Id);
        let configuration = msg!(webview as Id, c"configuration", Id);
        let draws_background = msg!(configuration, c"valueForKey:", Id;
            objc::ns_string(c"drawsBackground") => Id);
        let draws_background = msg!(draws_background, c"boolValue", bool);
        text.push_str(&format!(
            " subview_index={ours} webview_index={web} below_webview={} \
             webview_draws_background={draws_background}",
            ours < web
        ));
    } else {
        let key = msg!(window, c"isKeyWindow", bool);
        text.push_str(&format!(" key_window={key}"));
    }
    text
}

// ---- Keeping the screen awake ---------------------------------------------

/// Stops the display from sleeping while alive. mpv's `stop-screensaver` does
/// nothing with the render API, so we ask macOS ourselves.
pub struct WakeLock(usize);

impl WakeLock {
    pub fn new() -> Self {
        let token = objc::autorelease_pool(|| {
            // SAFETY: NSProcessInfo is thread-safe. The returned token is
            // retained so it outlives the pool.
            unsafe {
                let info = msg!(objc::class(c"NSProcessInfo"), c"processInfo", Id);
                let token = msg!(info, c"beginActivityWithOptions:reason:", Id;
                    KEEP_DISPLAY_AWAKE => u64,
                    objc::ns_string(c"Popcorn is playing a video") => Id);
                msg!(token, c"retain", Id)
            }
        });
        Self(token as usize)
    }
}

impl Drop for WakeLock {
    fn drop(&mut self) {
        objc::autorelease_pool(|| {
            // SAFETY: the token came from beginActivity and is ended and
            // released exactly once.
            unsafe {
                let info = msg!(objc::class(c"NSProcessInfo"), c"processInfo", Id);
                msg!(info, c"endActivity:", (); self.0 as Id => Id);
                msg!(self.0 as Id, c"release", ());
            }
        });
    }
}
