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