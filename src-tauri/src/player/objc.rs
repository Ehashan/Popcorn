//! The little of the Objective-C runtime that we need, by hand.

use std::ffi::{c_char, c_void, CStr};

/// An Objective-C object or class pointer.
pub type Id = *mut c_void;
pub type Sel = *const c_void;

pub const NIL: Id = std::ptr::null_mut();

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const c_char) -> Id;
    fn sel_registerName(name: *const c_char) -> Sel;
    /// Never called through this signature: `msg!` casts it to the exact
    /// signature of each method, as arm64 requires.
    pub fn objc_msgSend();
    fn objc_allocateClassPair(superclass: Id, name: *const c_char, extra: usize) -> Id;
    fn objc_registerClassPair(class: Id);
    fn class_addMethod(class: Id, selector: Sel, imp: *const c_void, types: *const c_char) -> bool;
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

/// Sends a message: `msg!(object, c"name:", ReturnType; value => Type)`.
///
/// Must be used inside `unsafe`. The caller guarantees that the receiver is
/// nil or a live object that implements the selector with exactly these
/// argument and return types, called on a thread the method allows.
macro_rules! msg {
    ($receiver:expr, $selector:expr, $ret:ty $(; $($arg:expr => $ty:ty),+)?) => {{
        let function: unsafe extern "C" fn(
            $crate::player::objc::Id,
            $crate::player::objc::Sel
            $($(, $ty)+)?
        ) -> $ret = std::mem::transmute(
            $crate::player::objc::objc_msgSend as unsafe extern "C" fn(),
        );
        function($receiver, $crate::player::objc::sel($selector) $($(, $arg)+)?)
    }};
}
pub(crate) use msg;

pub fn sel(name: &CStr) -> Sel {
    // SAFETY: a NUL-terminated name; registering a selector is always valid.
    unsafe { sel_registerName(name.as_ptr()) }
}

/// Nil if the class does not exist, which makes later messages no-ops.
pub fn class(name: &CStr) -> Id {
    // SAFETY: a NUL-terminated name.
    unsafe { objc_getClass(name.as_ptr()) }
}

/// An autoreleased `NSString`. Use inside `autorelease_pool`.
pub fn ns_string(text: &CStr) -> Id {
    // SAFETY: `stringWithUTF8String:` takes a C string and returns an object.
    unsafe {
        msg!(class(c"NSString"), c"stringWithUTF8String:", Id; text.as_ptr() => *const c_char)
    }
}

pub fn autorelease_pool<T>(run: impl FnOnce() -> T) -> T {
    // SAFETY: push and pop are paired on the same thread.
    let pool = unsafe { objc_autoreleasePoolPush() };
    let result = run();
    // SAFETY: as above.
    unsafe { objc_autoreleasePoolPop(pool) };
    result
}

pub struct Method {
    pub name: &'static CStr,
    /// The Objective-C type encoding.
    pub types: &'static CStr,
    pub imp: *const c_void,
}

/// Creates and registers a subclass. Call once per name. Returns nil if the
/// superclass is missing or the name is taken.
///
/// # Safety
/// Every `imp` must be an `extern "C"` function taking `(Id, Sel, ...)` whose
/// signature matches its type encoding and the method it overrides.
pub unsafe fn subclass(superclass: &CStr, name: &CStr, methods: &[Method]) -> Id {
    let parent = class(superclass);
    if parent.is_null() {
        return NIL;
    }
    let new = objc_allocateClassPair(parent, name.as_ptr(), 0);
    if new.is_null() {
        return NIL;
    }
    for method in methods {
        class_addMethod(new, sel(method.name), method.imp, method.types.as_ptr());
    }
    objc_registerClassPair(new);
    new
}
