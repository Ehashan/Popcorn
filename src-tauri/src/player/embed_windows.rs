//! Where the video appears on Windows:
//! * embedded: a child `HWND` behind the transparent WebView2, resized via `SetWindowSubclass`;
//! * fallback: a borderless `WS_POPUP` window on its own message-loop thread.

use std::ffi::{c_int, c_uint, c_void};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tauri::{AppHandle, Manager};

use super::keys;
use super::mpv::Mpv;
use crate::error::{AppError, AppResult};

pub type KeyHandler = Arc<dyn Fn(u16, &str, usize) + Send + Sync>;

type Hwnd = isize;
type Hinstance = isize;
type Hbrush = isize;
type Hcursor = isize;
type Hmonitor = isize;
type Wparam = usize;
type Lparam = isize;
type Lresult = isize;

const MAIN_THREAD_TIMEOUT: Duration = Duration::from_secs(5);

const WS_CHILD: u32 = 0x4000_0000;
const WS_POPUP: u32 = 0x8000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_CLIPCHILDREN: u32 = 0x0200_0000;
const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
const WS_EX_NOPARENTNOTIFY: u32 = 0x0000_0004;

const CS_DBLCLKS: u32 = 0x0008;
const CS_HREDRAW: u32 = 0x0002;
const CS_VREDRAW: u32 = 0x0001;

const WM_DESTROY: u32 = 0x0002;
const WM_SIZE: u32 = 0x0005;
const WM_CLOSE: u32 = 0x0010;
const WM_QUIT: u32 = 0x0012;
const WM_NCDESTROY: u32 = 0x0082;
const WM_NCLBUTTONDOWN: u32 = 0x00A1;
const WM_KEYDOWN: u32 = 0x0100;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONDBLCLK: u32 = 0x0203;

const GWLP_USERDATA: c_int = -21;
const HWND_BOTTOM: Hwnd = 1;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_FRAMECHANGED: u32 = 0x0020;
const MONITOR_DEFAULTTONEAREST: u32 = 0x0002;
const HTCAPTION: isize = 2;

const IDC_ARROW: usize = 32512;
const BLACK_BRUSH: c_int = 4;

const VK_SHIFT: c_int = 0x10;
const VK_CONTROL: c_int = 0x11;
const VK_MENU: c_int = 0x12;
const VK_LWIN: c_int = 0x5B;
const VK_RWIN: c_int = 0x5C;

const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
const ES_DISPLAY_REQUIRED: u32 = 0x0000_0002;
const ES_CONTINUOUS: u32 = 0x8000_0000;

const SUBCLASS_ID: usize = 0x504F_5043; // "POPC"

#[repr(C)]
struct WndClassExW {
    cb_size: u32,
    style: u32,
    lpfn_wnd_proc: unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam) -> Lresult,
    cb_cls_extra: c_int,
    cb_wnd_extra: c_int,
    h_instance: Hinstance,
    h_icon: isize,
    h_cursor: Hcursor,
    h_br_background: Hbrush,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
    h_icon_sm: isize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct MonitorInfo {
    cb_size: u32,
    rc_monitor: Rect,
    rc_work: Rect,
    dw_flags: u32,
}

#[repr(C)]
struct Msg {
    hwnd: Hwnd,
    message: u32,
    w_param: Wparam,
    l_param: Lparam,
    time: u32,
    pt_x: i32,
    pt_y: i32,
}

type SubclassProc = unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam, usize, usize) -> Lresult;

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(lpModuleName: *const u16) -> Hinstance;
    fn SetThreadExecutionState(esFlags: u32) -> u32;
}

#[link(name = "gdi32")]
extern "system" {
    fn GetStockObject(i: c_int) -> isize;
}

#[link(name = "comctl32")]
extern "system" {
    fn SetWindowSubclass(
        hWnd: Hwnd,
        pfnSubclass: SubclassProc,
        uIdSubclass: usize,
        dwRefData: usize,
    ) -> c_int;
    fn RemoveWindowSubclass(hWnd: Hwnd, pfnSubclass: SubclassProc, uIdSubclass: usize) -> c_int;
    fn DefSubclassProc(hWnd: Hwnd, uMsg: u32, wParam: Wparam, lParam: Lparam) -> Lresult;
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassExW(lpwcx: *const WndClassExW) -> u16;
    fn CreateWindowExW(
        dwExStyle: u32,
        lpClassName: *const u16,
        lpWindowName: *const u16,
        dwStyle: u32,
        X: c_int,
        Y: c_int,
        nWidth: c_int,
        nHeight: c_int,
        hWndParent: Hwnd,
        hMenu: isize,
        hInstance: Hinstance,
        lpParam: *mut c_void,
    ) -> Hwnd;
    fn DestroyWindow(hWnd: Hwnd) -> c_int;
    fn DefWindowProcW(hWnd: Hwnd, Msg: u32, wParam: Wparam, lParam: Lparam) -> Lresult;
    fn GetMessageW(lpMsg: *mut Msg, hWnd: Hwnd, wMsgFilterMin: u32, wMsgFilterMax: u32) -> c_int;
    fn TranslateMessage(lpMsg: *const Msg) -> c_int;
    fn DispatchMessageW(lpMsg: *const Msg) -> Lresult;
    fn PostMessageW(hWnd: Hwnd, Msg: u32, wParam: Wparam, lParam: Lparam) -> c_int;
    fn PostQuitMessage(nExitCode: c_int);
    fn LoadCursorW(hInstance: Hinstance, lpCursorName: *const u16) -> Hcursor;
    fn GetSystemMetrics(nIndex: c_int) -> c_int;
    fn SetWindowLongPtrW(hWnd: Hwnd, nIndex: c_int, dwNewLong: isize) -> isize;
    fn GetWindowLongPtrW(hWnd: Hwnd, nIndex: c_int) -> isize;
    fn SetWindowPos(
        hWnd: Hwnd,
        hWndInsertAfter: Hwnd,
        X: c_int,
        Y: c_int,
        cx: c_int,
        cy: c_int,
        uFlags: u32,
    ) -> c_int;
    fn GetWindowRect(hWnd: Hwnd, lpRect: *mut Rect) -> c_int;
    fn GetClientRect(hWnd: Hwnd, lpRect: *mut Rect) -> c_int;
    fn MonitorFromWindow(hwnd: Hwnd, dwFlags: u32) -> Hmonitor;
    fn GetMonitorInfoW(hMonitor: Hmonitor, lpmi: *mut MonitorInfo) -> c_int;
    fn GetKeyState(nVirtKey: c_int) -> i16;
    fn ToUnicode(
        wVirtKey: c_uint,
        wScanCode: c_uint,
        lpKeyState: *const u8,
        pwszBuff: *mut u16,
        cchBuff: c_int,
        wFlags: c_uint,
    ) -> c_int;
    fn GetKeyboardState(lpKeyState: *mut u8) -> c_int;
    fn ReleaseCapture() -> c_int;
    fn SendMessageW(hWnd: Hwnd, Msg: u32, wParam: Wparam, lParam: Lparam) -> Lresult;
}

struct WindowData {
    keys: KeyHandler,
    fullscreen: AtomicBool,
    saved_rect: Mutex<Rect>,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn on_main<T: Send + 'static>(
    app: &AppHandle,
    run: impl FnOnce() -> T + Send + 'static,
) -> AppResult<T> {
    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(run());
    })?;
    rx.recv_timeout(MAIN_THREAD_TIMEOUT)
        .map_err(|_| AppError::Player("the window did not respond".into()))
}

fn modifier_flags() -> usize {
    let mut flags = 0usize;
    // SAFETY: GetKeyState has no preconditions.
    unsafe {
        if GetKeyState(VK_SHIFT) < 0 {
            flags |= keys::SHIFT;
        }
        if GetKeyState(VK_CONTROL) < 0 {
            flags |= keys::CONTROL;
        }
        if GetKeyState(VK_MENU) < 0 {
            flags |= keys::OPTION;
        }
        if GetKeyState(VK_LWIN) < 0 || GetKeyState(VK_RWIN) < 0 {
            flags |= keys::COMMAND;
        }
    }
    flags
}

fn translate_char(vk: u16, l_param: Lparam) -> String {
    let mut state = [0u8; 256];
    // SAFETY: `state` is 256 bytes as required by GetKeyboardState.
    if unsafe { GetKeyboardState(state.as_mut_ptr()) } == 0 {
        return String::new();
    }
    state[VK_CONTROL as usize] = 0;
    state[VK_MENU as usize] = 0;
    let scan_code = ((l_param >> 16) & 0xFF) as c_uint;
    let mut buf = [0u16; 8];
    // SAFETY: buffers are valid and sized.
    let len = unsafe {
        ToUnicode(
            c_uint::from(vk),
            scan_code,
            state.as_ptr(),
            buf.as_mut_ptr(),
            buf.len() as c_int,
            0,
        )
    };
    if len > 0 {
        String::from_utf16_lossy(&buf[..len as usize])
    } else {
        String::new()
    }
}

fn toggle_fullscreen_hwnd(hwnd: Hwnd, data: &WindowData) -> bool {
    let entering = !data.fullscreen.load(Ordering::Acquire);
    set_fullscreen_hwnd(hwnd, data, entering);
    entering
}

fn set_fullscreen_hwnd(hwnd: Hwnd, data: &WindowData, fullscreen: bool) {
    let was = data.fullscreen.swap(fullscreen, Ordering::AcqRel);
    if was == fullscreen {
        return;
    }
    // SAFETY: `hwnd` is a valid window.
    unsafe {
        if fullscreen {
            let mut rect = Rect::default();
            if GetWindowRect(hwnd, &mut rect) != 0 {
                if let Ok(mut saved) = data.saved_rect.lock() {
                    *saved = rect;
                }
            }
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let mut info = MonitorInfo {
                cb_size: std::mem::size_of::<MonitorInfo>() as u32,
                rc_monitor: Rect::default(),
                rc_work: Rect::default(),
                dw_flags: 0,
            };
            if GetMonitorInfoW(monitor, &mut info) != 0 {
                let r = info.rc_monitor;
                SetWindowPos(
                    hwnd,
                    0,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
        } else if let Ok(saved) = data.saved_rect.lock() {
            SetWindowPos(
                hwnd,
                0,
                saved.left,
                saved.top,
                saved.right - saved.left,
                saved.bottom - saved.top,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }
}

unsafe extern "system" fn parent_subclass_proc(
    hwnd: Hwnd,
    msg: u32,
    w_param: Wparam,
    l_param: Lparam,
    _uid: usize,
    child_hwnd: usize,
) -> Lresult {
    if msg == WM_SIZE {
        let mut rect = Rect::default();
        if GetClientRect(hwnd, &mut rect) != 0 {
            SetWindowPos(
                child_hwnd as Hwnd,
                HWND_BOTTOM,
                0,
                0,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOACTIVATE,
            );
        }
    }
    DefSubclassProc(hwnd, msg, w_param, l_param)
}

unsafe extern "system" fn wnd_proc(
    hwnd: Hwnd,
    msg: u32,
    w_param: Wparam,
    l_param: Lparam,
) -> Lresult {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowData;
    match msg {
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if !ptr.is_null() {
                let data = &*ptr;
                let vk = w_param as u16;
                let chars = translate_char(vk, l_param);
                let mods = modifier_flags();
                (data.keys)(vk, &chars, mods);
            }
            0
        }
        WM_LBUTTONDBLCLK => {
            if !ptr.is_null() {
                toggle_fullscreen_hwnd(hwnd, &*ptr);
            }
            0
        }
        WM_LBUTTONDOWN => {
            if !ptr.is_null() && !(*ptr).fullscreen.load(Ordering::Acquire) {
                ReleaseCapture();
                SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION as Wparam, 0);
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        WM_NCDESTROY => {
            if !ptr.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(ptr as *mut WindowData));
            }
            DefWindowProcW(hwnd, msg, w_param, l_param)
        }
        _ => DefWindowProcW(hwnd, msg, w_param, l_param),
    }
}

#[derive(Clone)]
enum SurfaceKind {
    Embedded {
        parent_hwnd: Hwnd,
        child_hwnd: Hwnd,
    },
    Window {
        hwnd: Hwnd,
        thread: Arc<Mutex<Option<JoinHandle<()>>>>,
        fullscreen: Arc<AtomicBool>,
    },
}

#[derive(Clone)]
pub struct VideoSurface {
    kind: SurfaceKind,
}

impl VideoSurface {
    pub fn embed(app: &AppHandle, mpv: &Arc<Mpv>) -> AppResult<Self> {
        let app_clone = app.clone();
        let (parent_hwnd, child_hwnd) = on_main(app, move || -> AppResult<(Hwnd, Hwnd)> {
            let window = app_clone
                .get_webview_window("main")
                .ok_or_else(|| AppError::Player("main window not found".into()))?;
            let parent_hwnd = window
                .hwnd()
                .map_err(|e| AppError::Player(format!("could not get window handle: {e}")))?
                .0 as Hwnd;

            // Make WebView2's background transparent so the child HWND shows through.
            let _ = window.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));

            let class_name = wide("STATIC");
            let title = wide("PopcornVideo");
            // SAFETY: creating a child HWND inside `parent_hwnd` on the main thread.
            let child_hwnd = unsafe {
                let h_instance = GetModuleHandleW(null());
                let mut rect = Rect::default();
                GetClientRect(parent_hwnd, &mut rect);
                let child = CreateWindowExW(
                    WS_EX_NOPARENTNOTIFY,
                    class_name.as_ptr(),
                    title.as_ptr(),
                    WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
                    0,
                    0,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    parent_hwnd,
                    0,
                    h_instance,
                    null_mut(),
                );
                if child == 0 {
                    let _ = window
                        .set_background_color(Some(tauri::window::Color(0x10, 0x10, 0x11, 0xff)));
                    return Err(AppError::Player(
                        "could not create embedded video surface".into(),
                    ));
                }
                SetWindowPos(
                    child,
                    HWND_BOTTOM,
                    0,
                    0,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOACTIVATE,
                );
                SetWindowSubclass(
                    parent_hwnd,
                    parent_subclass_proc,
                    SUBCLASS_ID,
                    child as usize,
                );
                child
            };

            Ok((parent_hwnd, child_hwnd))
        })??;

        mpv.set("wid", &child_hwnd.to_string())?;

        Ok(Self {
            kind: SurfaceKind::Embedded {
                parent_hwnd,
                child_hwnd,
            },
        })
    }

    pub fn open_window(_app: &AppHandle, mpv: &Arc<Mpv>, keys: KeyHandler) -> AppResult<Self> {
        let (tx, rx) = mpsc::channel::<AppResult<Hwnd>>();
        let fullscreen = Arc::new(AtomicBool::new(false));

        let handle = thread::Builder::new()
            .name("popcorn-win-player".into())
            .spawn(move || {
                let class_name = wide("PopcornPlayerWindow");
                let title = wide("Popcorn");
                // SAFETY: Win32 window registration and creation on this dedicated UI thread.
                let hwnd = unsafe {
                    let h_instance = GetModuleHandleW(null());
                    let wc = WndClassExW {
                        cb_size: std::mem::size_of::<WndClassExW>() as u32,
                        style: CS_DBLCLKS | CS_HREDRAW | CS_VREDRAW,
                        lpfn_wnd_proc: wnd_proc,
                        cb_cls_extra: 0,
                        cb_wnd_extra: 0,
                        h_instance,
                        h_icon: 0,
                        h_cursor: LoadCursorW(0, IDC_ARROW as *const u16),
                        h_br_background: GetStockObject(BLACK_BRUSH),
                        lpsz_menu_name: null(),
                        lpsz_class_name: class_name.as_ptr(),
                        h_icon_sm: 0,
                    };
                    RegisterClassExW(&wc);

                    let screen_w = GetSystemMetrics(0).max(1280);
                    let screen_h = GetSystemMetrics(1).max(720);
                    let width = (screen_w * 3 / 4).clamp(960, 1280);
                    let height = (width * 9 / 16).min(screen_h - 80);
                    let x = (screen_w - width) / 2;
                    let y = (screen_h - height) / 2;

                    let hwnd = CreateWindowExW(
                        0,
                        class_name.as_ptr(),
                        title.as_ptr(),
                        WS_POPUP | WS_VISIBLE | WS_CLIPCHILDREN,
                        x,
                        y,
                        width,
                        height,
                        0,
                        0,
                        h_instance,
                        null_mut(),
                    );
                    if hwnd == 0 {
                        let _ = tx.send(Err(AppError::Player(
                            "could not create the player window".into(),
                        )));
                        return;
                    }
                    let data = Box::new(WindowData {
                        keys,
                        fullscreen: AtomicBool::new(false),
                        saved_rect: Mutex::new(Rect {
                            left: x,
                            top: y,
                            right: x + width,
                            bottom: y + height,
                        }),
                    });
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(data) as isize);
                    hwnd
                };

                let _ = tx.send(Ok(hwnd));

                let mut msg = Msg {
                    hwnd: 0,
                    message: 0,
                    w_param: 0,
                    l_param: 0,
                    time: 0,
                    pt_x: 0,
                    pt_y: 0,
                };
                // SAFETY: standard Win32 message loop for `hwnd` on this thread.
                unsafe {
                    while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                        if msg.message == WM_QUIT {
                            break;
                        }
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            })
            .map_err(|error| AppError::Player(format!("could not start player window: {error}")))?;

        let hwnd = rx
            .recv()
            .map_err(|_| AppError::Player("player window thread exited early".into()))??;

        mpv.set("wid", &hwnd.to_string())?;

        Ok(Self {
            kind: SurfaceKind::Window {
                hwnd,
                thread: Arc::new(Mutex::new(Some(handle))),
                fullscreen,
            },
        })
    }

    pub fn is_embedded(&self) -> bool {
        matches!(self.kind, SurfaceKind::Embedded { .. })
    }

    pub fn set_scale(&self, _: &AppHandle, _: f64) {}

    pub fn is_window_fullscreen(&self) -> bool {
        match &self.kind {
            SurfaceKind::Window { fullscreen, .. } => fullscreen.load(Ordering::Acquire),
            SurfaceKind::Embedded { .. } => false,
        }
    }

    pub fn toggle_window_fullscreen(&self, app: &AppHandle) -> AppResult<bool> {
        let next = !self.is_window_fullscreen();
        self.set_window_fullscreen(app, next)?;
        Ok(next)
    }

    pub fn set_window_fullscreen(&self, _: &AppHandle, fullscreen: bool) -> AppResult<()> {
        let SurfaceKind::Window {
            hwnd,
            fullscreen: flag,
            ..
        } = &self.kind
        else {
            return Ok(());
        };
        let was = flag.swap(fullscreen, Ordering::AcqRel);
        if was == fullscreen {
            return Ok(());
        }
        // SAFETY: `hwnd` is valid while `VideoSurface` is alive.
        let ptr = unsafe { GetWindowLongPtrW(*hwnd, GWLP_USERDATA) } as *const WindowData;
        if !ptr.is_null() {
            // SAFETY: `ptr` points to live `WindowData` owned by the window until `WM_NCDESTROY`.
            unsafe { set_fullscreen_hwnd(*hwnd, &*ptr, fullscreen) };
        }
        Ok(())
    }

    pub fn close(self, app: &AppHandle) {
        match self.kind {
            SurfaceKind::Embedded {
                parent_hwnd,
                child_hwnd,
            } => {
                let app_clone = app.clone();
                let _ = on_main(app, move || {
                    // SAFETY: removing subclass and destroying child window on the main thread.
                    unsafe {
                        RemoveWindowSubclass(parent_hwnd, parent_subclass_proc, SUBCLASS_ID);
                        DestroyWindow(child_hwnd);
                    }
                    if let Some(window) = app_clone.get_webview_window("main") {
                        let _ = window.set_background_color(Some(tauri::window::Color(
                            0x10, 0x10, 0x11, 0xff,
                        )));
                    }
                });
            }
            SurfaceKind::Window { hwnd, thread, .. } => {
                // SAFETY: posting WM_CLOSE to a live HWND shuts down its message loop cleanly.
                unsafe {
                    PostMessageW(hwnd, WM_CLOSE, 0, 0);
                }
                if let Ok(mut guard) = thread.lock() {
                    if let Some(handle) = guard.take() {
                        let _ = handle.join();
                    }
                }
            }
        }
    }

    #[cfg(debug_assertions)]
    pub fn press_key(&self, _: &AppHandle, _: u16, _: &'static std::ffi::CStr) {}

    #[cfg(debug_assertions)]
    pub fn describe(&self, _: &AppHandle) -> String {
        match &self.kind {
            SurfaceKind::Embedded { child_hwnd, .. } => format!("windows-embedded({child_hwnd})"),
            SurfaceKind::Window { hwnd, .. } => format!("windows-hwnd({hwnd})"),
        }
    }
}

pub struct WakeLock;

impl WakeLock {
    pub fn new() -> Self {
        // SAFETY: no preconditions.
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED);
        }
        Self
    }
}

impl Drop for WakeLock {
    fn drop(&mut self) {
        // SAFETY: clears our display/system wake requirement.
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS);
        }
    }
}
