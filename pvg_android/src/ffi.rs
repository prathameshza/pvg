#![allow(dead_code)]
#![allow(non_snake_case)]
#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]

use std::ffi::c_void;

#[cfg(target_os = "android")]
use std::os::raw::c_char;

#[repr(C)]
pub struct ANativeWindow {
    _unused: [u8; 0],
}

/// Exact C ABI layout of ANativeWindow_Buffer from Android NDK native_window.h
#[repr(C)]
pub struct ANativeWindow_Buffer {
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub format: i32,
    pub bits: *mut c_void,
    pub reserved: [u32; 6],
}

pub const WINDOW_FORMAT_RGBA_8888: i32 = 1;

pub const ANDROID_LOG_INFO: i32 = 4;
pub const ANDROID_LOG_WARN: i32 = 5;
pub const ANDROID_LOG_ERROR: i32 = 6;

#[cfg(target_os = "android")]
#[link(name = "android")]
#[link(name = "log")]
extern "C" {
    pub fn ANativeWindow_fromSurface(
        env: *mut jni::sys::JNIEnv,
        surface: jni::sys::jobject,
    ) -> *mut ANativeWindow;

    pub fn ANativeWindow_release(window: *mut ANativeWindow);

    pub fn ANativeWindow_setBuffersGeometry(
        window: *mut ANativeWindow,
        width: i32,
        height: i32,
        format: i32,
    ) -> i32;

    pub fn ANativeWindow_lock(
        window: *mut ANativeWindow,
        outBuffer: *mut ANativeWindow_Buffer,
        inOutDirtyBounds: *mut c_void,
    ) -> i32;

    pub fn ANativeWindow_unlockAndPost(window: *mut ANativeWindow) -> i32;

    pub fn __android_log_print(
        prio: i32,
        tag: *const c_char,
        fmt: *const c_char,
        ...
    ) -> i32;
}

#[cfg(not(target_os = "android"))]
pub unsafe fn ANativeWindow_fromSurface(
    _env: *mut jni::sys::JNIEnv,
    _surface: jni::sys::jobject,
) -> *mut ANativeWindow {
    std::ptr::null_mut()
}

#[cfg(not(target_os = "android"))]
pub unsafe fn ANativeWindow_release(_window: *mut ANativeWindow) {}

#[cfg(not(target_os = "android"))]
pub unsafe fn ANativeWindow_setBuffersGeometry(
    _window: *mut ANativeWindow,
    _width: i32,
    _height: i32,
    _format: i32,
) -> i32 {
    0
}

#[cfg(not(target_os = "android"))]
pub unsafe fn ANativeWindow_lock(
    _window: *mut ANativeWindow,
    _outBuffer: *mut ANativeWindow_Buffer,
    _inOutDirtyBounds: *mut c_void,
) -> i32 {
    -1
}

#[cfg(not(target_os = "android"))]
pub unsafe fn ANativeWindow_unlockAndPost(_window: *mut ANativeWindow) -> i32 {
    0
}

/// Best-effort display frame-rate hint (`ANativeWindow_setFrameRate`, API 30+).
///
/// Tells SurfaceFlinger our cadence (60Hz) so it can schedule composition
/// optimally instead of inferring it from queue behavior — the standard
/// game-loop recommendation for stable pacing. Dynamically resolved via
/// `dlopen`/`dlsym` so the library still loads on older releases; silently
/// a no-op when the symbol is absent.
#[cfg(target_os = "android")]
pub mod frame_rate {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_float, c_void};

    type SetFrameRateFn = unsafe extern "C" fn(*mut c_void, c_float, i8) -> i32;

    extern "C" {
        fn dlopen(filename: *const c_char, flags: i32) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }

    const RTLD_NOW: i32 = 2;
    /// `ANATIVEWINDOW_FRAME_RATE_COMPATIBILITY_DEFAULT`.
    const COMPAT_DEFAULT: i8 = 0;

    /// Requests 60Hz composition scheduling for `window`. Never fails loudly.
    ///
    /// # Safety
    /// `window` must be a live `ANativeWindow*` (or null, which is ignored).
    pub unsafe fn set_frame_rate_60(window: *mut c_void) {
        if window.is_null() {
            return;
        }
        let Ok(lib) = CString::new("libandroid.so") else {
            return;
        };
        let Ok(sym) = CString::new("ANativeWindow_setFrameRate") else {
            return;
        };
        let handle = dlopen(lib.as_ptr(), RTLD_NOW);
        if handle.is_null() {
            return;
        }
        let fptr = dlsym(handle, sym.as_ptr());
        if fptr.is_null() {
            return;
        }
        let f: SetFrameRateFn = std::mem::transmute(fptr);
        // Deliberately no dlclose: one handle for the process lifetime.
        f(window, 60.0, COMPAT_DEFAULT);
    }
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        if $crate::is_logging_enabled() {
            let msg = format!($($arg)*);
            #[cfg(target_os = "android")]
            {
                if let (Ok(tag), Ok(c_msg)) = (std::ffi::CString::new("PVG_NATIVE"), std::ffi::CString::new(msg)) {
                    unsafe {
                        $crate::ffi::__android_log_print(
                            $crate::ffi::ANDROID_LOG_INFO,
                            tag.as_ptr(),
                            c_msg.as_ptr(),
                        );
                    }
                }
            }
            #[cfg(not(target_os = "android"))]
            {
                println!("[PVG_NATIVE INFO] {}", msg);
            }
        }
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        if $crate::is_logging_enabled() {
            let msg = format!($($arg)*);
            #[cfg(target_os = "android")]
            {
                if let (Ok(tag), Ok(c_msg)) = (std::ffi::CString::new("PVG_NATIVE"), std::ffi::CString::new(msg)) {
                    unsafe {
                        $crate::ffi::__android_log_print(
                            $crate::ffi::ANDROID_LOG_WARN,
                            tag.as_ptr(),
                            c_msg.as_ptr(),
                        );
                    }
                }
            }
            #[cfg(not(target_os = "android"))]
            {
                eprintln!("[PVG_NATIVE WARN] {}", msg);
            }
        }
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        let msg = format!($($arg)*);
        #[cfg(target_os = "android")]
        {
            if let (Ok(tag), Ok(c_msg)) = (std::ffi::CString::new("PVG_NATIVE"), std::ffi::CString::new(msg)) {
                unsafe {
                    $crate::ffi::__android_log_print(
                        $crate::ffi::ANDROID_LOG_ERROR,
                        tag.as_ptr(),
                        c_msg.as_ptr(),
                    );
                }
            }
        }
        #[cfg(not(target_os = "android"))]
        {
            eprintln!("[PVG_NATIVE ERROR] {}", msg);
        }
    };
}