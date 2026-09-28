mod ffi;
mod rasterizer;
mod sys_monitor;
pub mod text;

pub use rasterizer::{FrameCache, FxCache};
pub use text::TextEngine;

use ffi::*;
use jni::objects::{JClass, JObject, JString};
use jni::sys::{jboolean, jdouble, jdoubleArray, jint, jlong, JNI_TRUE};
use jni::JNIEnv;
use pvg::ast::Document;
use pvg::draw_list::DrawList;
use pvg::eval::Evaluator;
use pvg::parse_pvg;
use rasterizer::rasterize_draw_list_into_pixmap_mut;
use sys_monitor::SystemMonitor;
use tiny_skia::PixmapMut;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

static LOGGING_ENABLED: AtomicBool = AtomicBool::new(false);

#[inline]
pub fn is_logging_enabled() -> bool {
    LOGGING_ENABLED.load(Ordering::Relaxed)
}

#[inline]
pub fn set_logging_enabled(enabled: bool) {
    LOGGING_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Profiling hook: rasterize a draw list into an existing pixmap.
/// Exposed for host-side benchmarks of the Android raster path.
/// Reuse one [`FrameCache`] across frames to profile steady-state
/// (warm-cache) animation performance, exactly like the engine does.
#[doc(hidden)]
pub fn rasterize_for_profile(
    draw_list: &DrawList,
    pixmap: &mut PixmapMut,
    target_width: u32,
    target_height: u32,
    cache: &mut FrameCache,
) {
    rasterizer::rasterize_draw_list_into_pixmap_mut(
        draw_list,
        pixmap,
        target_width,
        target_height,
        cache,
    )
}

struct PvgEngineState {
    code: String,
    cached_doc: Option<Document>,
    time: f64,
    speed: f64,
    is_playing: bool,
    is_animated: bool,
    window: *mut ANativeWindow,
    surface_width: i32,
    surface_height: i32,

    // Real-time telemetry
    last_parse_us: f64,
    last_eval_us: f64,
    last_raster_us: f64,
    last_fps: f64,
    primitive_count: usize,

    /// Latest parse/eval failure, surfaced to the Kotlin UI so broken
    /// sources (e.g. pasted code with tabs) explain themselves instead of
    /// rendering nothing. Empty when the current source is healthy.
    last_error: String,

    /// Host uniform overrides for declared `param` declarations (Section 18.1).
    /// Applied on every evaluate; values win over the document defaults.
    params: HashMap<String, f64>,

    /// Frame-persistent raster caches (static FX layers + glyph coverage).
    /// Survives across frames so animation ticks only pay for time-varying
    /// shapes and previously unseen characters.
    frame_cache: FrameCache,
}

unsafe impl Send for PvgEngineState {}
unsafe impl Sync for PvgEngineState {}

pub struct PvgEngine {
    state: Arc<Mutex<PvgEngineState>>,
    running: Arc<AtomicBool>,
    needs_render: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
    /// Serializes raw `ANativeWindow` use (lock/raster/post on the render
    /// thread) against lifecycle release (UI thread surface callbacks).
    /// A dedicated leaf mutex: acquiring it never blocks engine state ops
    /// (`set_source`, sliders, …), which don't touch the window. Lock order
    /// everywhere is surface → state, so no deadlock is possible.
    surface_lock: Arc<Mutex<()>>,
}

impl PvgEngine {
    pub fn new(source: String, is_playing: bool, speed: f64) -> Self {
        let is_animated = source.contains("time")
            || source.contains(" t ")
            || source.contains("(t)")
            || source.contains("* t");

        let mut parse_us = 0.0;
        let t0 = Instant::now();
        let mut init_error = String::new();
        let cached_doc = match parse_pvg(&source) {
            Ok(doc) => {
                parse_us = t0.elapsed().as_secs_f64() * 1_000_000.0;
                Some(doc)
            }
            Err(e) => {
                log_warn!("PVG AST Parse Error: {}", e);
                init_error = format!("Parse error: {e}");
                None
            }
        };

        log_info!("🚀 [ENGINE INIT] AST parsed in {:.2} µs (Animated: {})", parse_us, is_animated);

        let state = Arc::new(Mutex::new(PvgEngineState {
            code: source,
            cached_doc,
            time: 0.0,
            speed,
            is_playing,
            is_animated,
            window: std::ptr::null_mut(),
            surface_width: 480,
            surface_height: 480,
            last_parse_us: parse_us,
            last_eval_us: 0.0,
            last_raster_us: 0.0,
            last_fps: 60.0,
            primitive_count: 0,
            params: HashMap::new(),
            frame_cache: FrameCache::default(),
            last_error: init_error,
        }));

        let running = Arc::new(AtomicBool::new(true));
        let needs_render = Arc::new(AtomicBool::new(true));
        let surface_lock = Arc::new(Mutex::new(()));

        let thread_state = Arc::clone(&state);
        let thread_running = Arc::clone(&running);
        let thread_needs_render = Arc::clone(&needs_render);
        let thread_surface = Arc::clone(&surface_lock);

        // Dedicated native thread: 0% Main UI thread & 0% HWUI RenderThread overhead
        let thread_handle = thread::spawn(move || {
            let target_frame_budget = Duration::from_micros(16_666); // 60 FPS target
            let mut last_tick = Instant::now();
            let mut frame_count = 0;
            let mut log_timer = Instant::now();

            let mut acc_eval_us = 0.0;
            let mut acc_raster_us = 0.0;
            let mut acc_lock_us = 0.0;
            let mut acc_post_us = 0.0;

            let mut sys_monitor = SystemMonitor::new();

            while thread_running.load(Ordering::Relaxed) {
                let start_frame = Instant::now();
                let dt = start_frame.duration_since(last_tick).as_secs_f64().clamp(0.001, 0.033);
                last_tick = start_frame;

                frame_count += 1;

                let should_render;
                let current_time;
                let is_active;

                {
                    if let Ok(mut s) = thread_state.lock() {
                        let has_window = !s.window.is_null();
                        is_active = s.is_playing && s.is_animated && has_window;
                        if is_active {
                            s.time += dt * s.speed;
                        }
                        current_time = s.time;
                        should_render = (is_active || thread_needs_render.swap(false, Ordering::Relaxed)) && has_window;
                    } else {
                        break;
                    }
                }

                if should_render {
                    let (eval_us, raster_us, lock_us, post_us) =
                        Self::render_frame_direct(&thread_state, &thread_surface, current_time);
                    acc_eval_us += eval_us;
                    acc_raster_us += raster_us;
                    acc_lock_us += lock_us;
                    acc_post_us += post_us;
                }

                // 1-second interval diagnostics summary
                let elapsed_log = log_timer.elapsed().as_secs_f64();
                if elapsed_log >= 1.0 {
                    let fps = (frame_count as f64) / elapsed_log;
                    let n = (frame_count as f64).max(1.0);
                    let avg_eval = acc_eval_us / n;
                    let avg_raster = (acc_raster_us / n) / 1000.0;
                    let avg_lock = acc_lock_us / n;
                    let avg_post = acc_post_us / n;

                    if is_logging_enabled() {
                        if let Ok(mut s) = thread_state.lock() {
                            s.last_fps = fps;
                            log_info!(
                                "📊 [NATIVE 1s LOG] FPS: {:>4.1} | Eval: {:>5.1}µs | Raster: {:>4.2}ms | Lock: {:>5.1}µs | Post: {:>5.1}µs | Buf: {}x{}",
                                fps, avg_eval, avg_raster, avg_lock, avg_post, s.surface_width, s.surface_height
                            );
                        }
                        sys_monitor.log_1s_thread_profiler();
                    } else if let Ok(mut s) = thread_state.lock() {
                        s.last_fps = fps;
                    }

                    frame_count = 0;
                    acc_eval_us = 0.0;
                    acc_raster_us = 0.0;
                    acc_lock_us = 0.0;
                    acc_post_us = 0.0;
                    log_timer = Instant::now();
                }

                if !is_active {
                    thread::sleep(Duration::from_millis(30));
                    continue;
                }

                let frame_elapsed = start_frame.elapsed();
                if frame_elapsed < target_frame_budget {
                    thread::sleep(target_frame_budget - frame_elapsed);
                } else {
                    thread::sleep(Duration::from_millis(2));
                }
            }
        });

        Self {
            state,
            running,
            needs_render,
            thread_handle: Some(thread_handle),
            surface_lock,
        }
    }

    /// Renders directly into ANativeWindow buffer using PixmapMut zero-copy mapping.
    ///
    /// Holds the surface mutex across the whole lock/raster/post sequence so
    /// a concurrent `surfaceDestroyed` release cannot free the window
    /// mid-frame (which aborted the process with a RefBase "Double owned?"
    /// SIGABRT when the SurfaceView was torn down, e.g. tab switches).
    fn render_frame_direct(
        state_arc: &Arc<Mutex<PvgEngineState>>,
        surface_lock: &Arc<Mutex<()>>,
        time: f64,
    ) -> (f64, f64, f64, f64) {
        let _surface_guard = surface_lock.lock().unwrap();
        let (window, doc_opt, params) = {
            let s = state_arc.lock().unwrap();
            (
                s.window,
                s.cached_doc.clone(),
                s.params.clone(),
            )
        };
        // Frame cache moves out for the raster (no state lock held while
        // rasterizing) and moves back in afterwards.
        let mut frame_cache = {
            match state_arc.lock() {
                Ok(mut s) => std::mem::take(&mut s.frame_cache),
                Err(_) => FrameCache::default(),
            }
        };

        if window.is_null() {
            return (0.0, 0.0, 0.0, 0.0);
        }

        let doc = match doc_opt {
            Some(d) => d,
            None => return (0.0, 0.0, 0.0, 0.0),
        };

        // Phase 1: Procedural AST Evaluation (host uniforms override defaults).
        let eval_t0 = Instant::now();
        let mut evaluator = Evaluator::new_with_time(time);
        for (k, v) in &params {
            evaluator.set_param(k.clone(), pvg::eval::Value::Number(*v));
        }
        let draw_list: DrawList = match evaluator.evaluate_document(&doc) {
            Ok(dl) => dl,
            Err(e) => {
                // Never leave stale telemetry behind a broken frame: the UI
                // would otherwise keep showing the previous scene's counts.
                if let Ok(mut s) = state_arc.lock() {
                    s.last_eval_us = eval_t0.elapsed().as_secs_f64() * 1_000_000.0;
                    s.primitive_count = 0;
                    s.last_error = format!("Eval error: {e}");
                }
                return (0.0, 0.0, 0.0, 0.0);
            }
        };
        let eval_us = eval_t0.elapsed().as_secs_f64() * 1_000_000.0;
        let primitive_count = draw_list.items.len();

        // Phase 2: Lock Surface Buffer & In-Place Rasterization
        let (lock_us, raster_us, post_us) = unsafe {
            let mut buffer = ANativeWindow_Buffer {
                width: 0,
                height: 0,
                stride: 0,
                format: 0,
                bits: std::ptr::null_mut(),
                reserved: [0; 6],
            };

            let lock_t0 = Instant::now();
            let lock_res = ANativeWindow_lock(window, &mut buffer, std::ptr::null_mut());
            let lock_elapsed = lock_t0.elapsed().as_secs_f64() * 1_000_000.0;
            let mut raster_elapsed = 0.0;

            if lock_res == 0 {
                if !buffer.bits.is_null() && buffer.width > 0 && buffer.height > 0 && buffer.stride > 0 {
                    // The surface may have resized between frames (rotation,
                    // tab switches): posting a stale-size buffer is rejected
                    // by BLASTBufferQueue, so resync and skip this frame.
                    let dims_ok = {
                        if let Ok(s) = state_arc.lock() {
                            buffer.width == s.surface_width && buffer.height == s.surface_height
                        } else {
                            false
                        }
                    };
                    if !dims_ok {
                        if let Ok(mut s) = state_arc.lock() {
                            s.surface_width = buffer.width;
                            s.surface_height = buffer.height;
                            s.frame_cache.fx.clear();
                        }
                    } else {
                        let total_bytes =
                            (buffer.stride * buffer.height * 4) as usize;
                        let raw_slice = std::slice::from_raw_parts_mut(
                            buffer.bits as *mut u8,
                            total_bytes,
                        );

                        let pixmap_width = buffer.stride as u32;
                        let pixmap_height = buffer.height as u32;

                        // Phase 3: Direct In-Place Rasterization
                        if let Some(mut pixmap_mut) =
                            PixmapMut::from_bytes(raw_slice, pixmap_width, pixmap_height)
                        {
                            let raster_t0 = Instant::now();
                            rasterize_draw_list_into_pixmap_mut(
                                &draw_list,
                                &mut pixmap_mut,
                                buffer.width as u32,
                                buffer.height as u32,
                                &mut frame_cache,
                            );
                            raster_elapsed = raster_t0.elapsed().as_secs_f64() * 1_000_000.0;
                        }
                    }
                }

                // Phase 4: Post Framebuffer to Hardware Display
                let post_t0 = Instant::now();
                ANativeWindow_unlockAndPost(window);
                let post_elapsed = post_t0.elapsed().as_secs_f64() * 1_000_000.0;
                (lock_elapsed, raster_elapsed, post_elapsed)
            } else {
                (lock_elapsed, 0.0, 0.0)
            }
        };

        if let Ok(mut s) = state_arc.lock() {
            s.last_eval_us = eval_us;
            s.last_raster_us = raster_us;
            s.primitive_count = primitive_count;
            s.frame_cache = frame_cache;
            s.last_error.clear();
        }

        (eval_us, raster_us, lock_us, post_us)
    }

    pub fn set_source(&self, source: String) {
        let is_animated = source.contains("time")
            || source.contains(" t ")
            || source.contains("(t)")
            || source.contains("* t");

        let t0 = Instant::now();
        let parsed = parse_pvg(&source);
        let parse_us = t0.elapsed().as_secs_f64() * 1_000_000.0;

        log_info!("🔄 [SOURCE UPDATE] Re-parsed AST in {:.2} µs (Animated: {})", parse_us, is_animated);

        if let Ok(mut s) = self.state.lock() {
            match parsed {
                Ok(doc) => {
                    s.cached_doc = Some(doc);
                    s.last_error.clear();
                }
                Err(e) => {
                    s.cached_doc = None;
                    s.primitive_count = 0;
                    s.last_error = format!("Parse error: {e}");
                }
            }
            s.code = source;
            s.is_animated = is_animated;
            s.last_parse_us = parse_us;
            s.frame_cache.fx.clear();
        }
        self.needs_render.store(true, Ordering::Relaxed);
    }

    pub fn set_playing(&self, playing: bool) {
        log_info!("⏯️ [STATE] isPlaying = {}", playing);
        if let Ok(mut s) = self.state.lock() {
            s.is_playing = playing;
        }
        self.needs_render.store(true, Ordering::Relaxed);
    }

    pub fn set_time(&self, time: f64) {
        if let Ok(mut s) = self.state.lock() {
            s.time = time;
        }
        self.needs_render.store(true, Ordering::Relaxed);
    }

    pub fn set_speed(&self, speed: f64) {
        log_info!("⚡ [SPEED] Playback speed = {:.2}x", speed);
        if let Ok(mut s) = self.state.lock() {
            s.speed = speed;
        }
    }

    /// Sets a host uniform (`param`, Section 18.1) for the next evaluated frame.
    pub fn set_param(&self, name: String, value: f64) {
        if let Ok(mut s) = self.state.lock() {
            s.params.insert(name.clone(), value);
        }
        self.needs_render.store(true, Ordering::Relaxed);
    }

    /// Removes a host uniform override so the document default applies again.
    pub fn clear_param(&self, name: &str) {
        if let Ok(mut s) = self.state.lock() {
            s.params.remove(name);
        }
        self.needs_render.store(true, Ordering::Relaxed);
    }

    /// Declared host uniform names for the current source.
    pub fn param_names(&self) -> Vec<String> {
        self.state
            .lock()
            .ok()
            .and_then(|s| s.cached_doc.as_ref().map(|d| d.param_names().iter().map(|n| n.to_string()).collect()))
            .unwrap_or_default()
    }

    pub fn on_surface_created(&self, window: *mut ANativeWindow) {
        log_info!("🖼️ [SURFACE CREATED] ANativeWindow handle = {:?}", window);
        let _guard = self.surface_lock.lock().unwrap();
        if let Ok(mut s) = self.state.lock() {
            if !s.window.is_null() && s.window != window {
                unsafe { ANativeWindow_release(s.window); }
            }
            s.window = window;
        }
        self.needs_render.store(true, Ordering::Relaxed);
    }

    pub fn on_surface_changed(&self, width: i32, height: i32) {
        log_info!("📐 [SURFACE CHANGED] Dimensions: {}x{}", width, height);
        let _guard = self.surface_lock.lock().unwrap();
        if let Ok(mut s) = self.state.lock() {
            s.surface_width = width;
            s.surface_height = height;
        }
        self.needs_render.store(true, Ordering::Relaxed);
    }

    pub fn on_surface_destroyed(&self) {
        log_info!("🗑️ [SURFACE DESTROYED] Releasing ANativeWindow handle");
        let _guard = self.surface_lock.lock().unwrap();
        if let Ok(mut s) = self.state.lock() {
            if !s.window.is_null() {
                unsafe {
                    ANativeWindow_release(s.window);
                }
                s.window = std::ptr::null_mut();
            }
        }
    }

    pub fn get_telemetry(&self) -> (f64, f64, f64, f64, usize) {
        if let Ok(s) = self.state.lock() {
            (s.last_parse_us, s.last_eval_us, s.last_raster_us, s.last_fps, s.primitive_count)
        } else {
            (0.0, 0.0, 0.0, 0.0, 0)
        }
    }

    /// Latest parse/eval failure for the current source, or empty string.
    pub fn get_last_error(&self) -> String {
        if let Ok(s) = self.state.lock() {
            s.last_error.clone()
        } else {
            String::new()
        }
    }
}

impl Drop for PvgEngine {
    fn drop(&mut self) {
        log_info!("🛑 [ENGINE DROP] Terminating native render worker thread");
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
        self.on_surface_destroyed();
    }
}

// =========================================================================
// JNI EXPORTS (com.pvg.android.PvgEngine)
// =========================================================================

#[no_mangle]
pub extern "system" fn Java_com_pvg_android_PvgEngine_nativeSetLoggingEnabled(
    _env: JNIEnv,
    _class: JClass,
    enabled: jboolean,
) {
    set_logging_enabled(enabled == JNI_TRUE);
}

#[no_mangle]
pub extern "system" fn Java_com_pvg_android_PvgEngine_nativeInit(
    mut env: JNIEnv,
    _class: JClass,
    source: JString,
    is_playing: jboolean,
    speed: jdouble,
) -> jlong {
    let src_str: String = match env.get_string(&source) {
        Ok(s) => s.into(),
        Err(_) => String::new(),
    };

    let engine = Box::new(PvgEngine::new(src_str, is_playing == JNI_TRUE, speed));
    Box::into_raw(engine) as jlong
}

#[no_mangle]
pub extern "system" fn Java_com_pvg_android_PvgEngine_nativeDestroy(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if handle != 0 {
        unsafe {
            let _ = Box::from_raw(handle as *mut PvgEngine);
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_pvg_android_PvgEngine_nativeSetSource(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    source: JString,
) {
    if handle != 0 {
        let engine = unsafe { &*(handle as *const PvgEngine) };
        if let Ok(s) = env.get_string(&source) {
            engine.set_source(s.into());
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_pvg_android_PvgEngine_nativeSetPlaying(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    playing: jboolean,
) {
    if handle != 0 {
        let engine = unsafe { &*(handle as *const PvgEngine) };
        engine.set_playing(playing == JNI_TRUE);
    }
}

#[no_mangle]
pub extern "system" fn Java_com_pvg_android_PvgEngine_nativeSetTime(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    time: jdouble,
) {
    if handle != 0 {
        let engine = unsafe { &*(handle as *const PvgEngine) };
        engine.set_time(time);
    }
}

#[no_mangle]
pub extern "system" fn Java_com_pvg_android_PvgEngine_nativeSetSpeed(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    speed: jdouble,
) {
    if handle != 0 {
        let engine = unsafe { &*(handle as *const PvgEngine) };
        engine.set_speed(speed);
    }
}

/// Sets a declared `param` (host uniform, Section 18.1) for the running scene.
#[no_mangle]
pub extern "system" fn Java_com_pvg_android_PvgEngine_nativeSetParam(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    name: JString,
    value: jdouble,
) {
    if handle != 0 {
        let engine = unsafe { &*(handle as *const PvgEngine) };
        if let Ok(n) = env.get_string(&name) {
            let name_str: String = n.into();
            engine.set_param(name_str, value);
        }
    }
}

/// Clears a host uniform override so the document default applies again.
#[no_mangle]
pub extern "system" fn Java_com_pvg_android_PvgEngine_nativeClearParam(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    name: JString,
) {
    if handle != 0 {
        let engine = unsafe { &*(handle as *const PvgEngine) };
        if let Ok(n) = env.get_string(&name) {
            let name_str: String = n.into();
            engine.clear_param(&name_str);
        }
    }
}

/// Returns the declared `param` names for the current source (null on error).
#[no_mangle]
pub extern "system" fn Java_com_pvg_android_PvgEngine_nativeGetParamNames(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jni::sys::jobjectArray {
    let names: Vec<String> = if handle != 0 {
        let engine = unsafe { &*(handle as *const PvgEngine) };
        engine.param_names()
    } else {
        Vec::new()
    };

    let string_class = match env.find_class("java/lang/String") {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    let initial = JObject::null();
    match env.new_object_array(
        names.len() as jni::sys::jsize,
        &string_class,
        &initial,
    ) {
        Ok(arr) => {
            for (i, n) in names.iter().enumerate() {
                if let Ok(s) = env.new_string(n) {
                    let _ = env.set_object_array_element(&arr, i as jni::sys::jsize, &s);
                }
            }
            arr.into_raw()
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_pvg_android_PvgEngine_nativeOnSurfaceCreated(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
    surface: JObject,
) {
    if handle != 0 && !surface.is_null() {
        let engine = unsafe { &*(handle as *const PvgEngine) };
        let native_window = unsafe {
            ANativeWindow_fromSurface(env.get_raw() as *mut _, surface.as_raw())
        };
        if !native_window.is_null() {
            engine.on_surface_created(native_window);
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_pvg_android_PvgEngine_nativeOnSurfaceChanged(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    width: jint,
    height: jint,
) {
    if handle != 0 {
        let engine = unsafe { &*(handle as *const PvgEngine) };
        engine.on_surface_changed(width, height);
    }
}

#[no_mangle]
pub extern "system" fn Java_com_pvg_android_PvgEngine_nativeOnSurfaceDestroyed(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if handle != 0 {
        let engine = unsafe { &*(handle as *const PvgEngine) };
        engine.on_surface_destroyed();
    }
}

/// Returns the latest parse/eval error for the current source (empty = healthy).
#[no_mangle]
pub extern "system" fn Java_com_pvg_android_PvgEngine_nativeGetLastError(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jni::sys::jstring {
    let msg = if handle != 0 {
        let engine = unsafe { &*(handle as *const PvgEngine) };
        engine.get_last_error()
    } else {
        String::new()
    };
    match env.new_string(msg) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_pvg_android_PvgEngine_nativeGetTelemetry(
    env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jdoubleArray {
    let (parse_us, eval_us, raster_us, fps, shapes) = if handle != 0 {
        let engine = unsafe { &*(handle as *const PvgEngine) };
        engine.get_telemetry()
    } else {
        (0.0, 0.0, 0.0, 0.0, 0)
    };

    let arr = env.new_double_array(5).unwrap();
    let data = [parse_us, eval_us, raster_us, fps, shapes as f64];
    env.set_double_array_region(&arr, 0, &data).unwrap();
    arr.into_raw()
}