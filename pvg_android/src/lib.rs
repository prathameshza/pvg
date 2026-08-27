mod ffi;
mod rasterizer;
mod sys_monitor;

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
}

unsafe impl Send for PvgEngineState {}
unsafe impl Sync for PvgEngineState {}

pub struct PvgEngine {
    state: Arc<Mutex<PvgEngineState>>,
    running: Arc<AtomicBool>,
    needs_render: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
}

impl PvgEngine {
    pub fn new(source: String, is_playing: bool, speed: f64) -> Self {
        let is_animated = source.contains("time")
            || source.contains(" t ")
            || source.contains("(t)")
            || source.contains("* t");

        let mut parse_us = 0.0;
        let t0 = Instant::now();
        let cached_doc = match parse_pvg(&source) {
            Ok(doc) => {
                parse_us = t0.elapsed().as_secs_f64() * 1_000_000.0;
                Some(doc)
            }
            Err(e) => {
                log_warn!("PVG AST Parse Error: {}", e);
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
        }));

        let running = Arc::new(AtomicBool::new(true));
        let needs_render = Arc::new(AtomicBool::new(true));

        let thread_state = Arc::clone(&state);
        let thread_running = Arc::clone(&running);
        let thread_needs_render = Arc::clone(&needs_render);

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
                    let (eval_us, raster_us, lock_us, post_us) = Self::render_frame_direct(&thread_state, current_time);
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
        }
    }

    /// Renders directly into ANativeWindow buffer using PixmapMut zero-copy mapping
    fn render_frame_direct(state_arc: &Arc<Mutex<PvgEngineState>>, time: f64) -> (f64, f64, f64, f64) {
        let (window, doc_opt) = {
            let s = state_arc.lock().unwrap();
            (s.window, s.cached_doc.clone())
        };

        if window.is_null() {
            return (0.0, 0.0, 0.0, 0.0);
        }

        let doc = match doc_opt {
            Some(d) => d,
            None => return (0.0, 0.0, 0.0, 0.0),
        };

        // Phase 1: Procedural AST Evaluation
        let eval_t0 = Instant::now();
        let evaluator = Evaluator::new_with_time(time);
        let draw_list: DrawList = match evaluator.evaluate_document(&doc) {
            Ok(dl) => dl,
            Err(_) => return (0.0, 0.0, 0.0, 0.0),
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
                    let total_bytes = (buffer.stride * buffer.height * 4) as usize;
                    let raw_slice = std::slice::from_raw_parts_mut(buffer.bits as *mut u8, total_bytes);

                    let pixmap_width = buffer.stride as u32;
                    let pixmap_height = buffer.height as u32;

                    // Phase 3: Direct In-Place Rasterization
                    if let Some(mut pixmap_mut) = PixmapMut::from_bytes(raw_slice, pixmap_width, pixmap_height) {
                        let raster_t0 = Instant::now();
                        rasterize_draw_list_into_pixmap_mut(
                            &draw_list,
                            &mut pixmap_mut,
                            buffer.width as u32,
                            buffer.height as u32,
                        );
                        raster_elapsed = raster_t0.elapsed().as_secs_f64() * 1_000_000.0;
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
        }

        (eval_us, raster_us, lock_us, post_us)
    }

    pub fn set_source(&self, source: String) {
        let is_animated = source.contains("time")
            || source.contains(" t ")
            || source.contains("(t)")
            || source.contains("* t");

        let t0 = Instant::now();
        let cached_doc = parse_pvg(&source).ok();
        let parse_us = t0.elapsed().as_secs_f64() * 1_000_000.0;

        log_info!("🔄 [SOURCE UPDATE] Re-parsed AST in {:.2} µs (Animated: {})", parse_us, is_animated);

        if let Ok(mut s) = self.state.lock() {
            s.code = source;
            s.cached_doc = cached_doc;
            s.is_animated = is_animated;
            s.last_parse_us = parse_us;
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

    pub fn on_surface_created(&self, window: *mut ANativeWindow) {
        log_info!("🖼️ [SURFACE CREATED] ANativeWindow handle = {:?}", window);
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
        if let Ok(mut s) = self.state.lock() {
            s.surface_width = width;
            s.surface_height = height;
        }
        self.needs_render.store(true, Ordering::Relaxed);
    }

    pub fn on_surface_destroyed(&self) {
        log_info!("🗑️ [SURFACE DESTROYED] Releasing ANativeWindow handle");
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