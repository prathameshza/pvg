# PVG for Android (`io.github.prathameshza:pvg`)

High-performance Android runtime for **Procedural Vector Graphics (PVG)**.  
Renders deterministic procedural graphics and animations directly on the CPU at 60+ FPS via `ANativeWindow` with zero HWUI upload overhead and zero GPU power draw.

---

## 📖 Table of Contents

- [Why PVG on Android?](#why-pvg-on-android)
- [Installation](#installation)
- [Global Configuration & Logging](#global-configuration--logging)
- [Quickstart Guide](#quickstart-guide)
  - [1. Jetpack Compose (`PvgView`)](#1-jetpack-compose-pvgview)
  - [2. Controller & Animation State (`PvgController`)](#2-controller--animation-state-pvgcontroller)
  - [3. Traditional XML Layouts (`PvgSurfaceView`)](#3-traditional-xml-layouts-pvgsurfaceview)
- [Complete API Reference](#complete-api-reference)
  - [`Pvg` (Global Configuration)](#pvg-global-configuration)
  - [`PvgView` (Composable)](#pvgview-composable)
  - [`PvgController`](#pvgcontroller)
  - [`rememberPvgController()`](#rememberpvgcontroller)
  - [`PvgSurfaceView` (Android View)](#pvgsurfaceview-android-view)
  - [`PvgTelemetry`](#pvgtelemetry)
  - [`PvgEngine` (Low-Level JNI)](#pvgengine-low-level-jni)
- [Real-Time Telemetry & Diagnostics](#real-time-telemetry--diagnostics)
- [Native Kernel Profiling & Logcat Output](#native-kernel-profiling--logcat-output)
- [Architecture & Memory Model](#architecture--memory-model)
- [Supported Architectures & ProGuard](#supported-architectures--proguard)
- [License](#license)

---

## Why PVG on Android?

Modern mobile vector rendering (SVGs and Lottie) relies heavily on DOM trees, JSON parsing churn, and GPU hardware layers that drain battery and trigger frame drops.

PVG eliminates this overhead entirely:
1. **0% GPU Dependency:** Evaluates pure vector geometry directly on the CPU and streams rendered scanlines directly into hardware `ANativeWindow` surface buffers.
2. **Microsecond Latency:** Evaluates animated scenes in under **40 microseconds** per frame on a single thread.
3. **Bounded Memory:** Runs inside a contiguous sub-50 KB heap budget with zero DOM garbage collection.
4. **Native Procedural Math:** Express dynamic dials, HUDs, charts, and radars with native `for` loops, `sin`, `cos`, and `time` clocks without JavaScript runtimes or WebView overhead.

---

## Installation

Add the dependency to your application's `build.gradle.kts`:

```kotlin
dependencies {
    implementation("io.github.prathameshza:pvg:0.1.0")
}
```

Or in Groovy (`build.gradle`):

```groovy
dependencies {
    implementation 'io.github.prathameshza:pvg:0.1.0'
}
```

---

## Global Configuration & Logging

By default, native Android Logcat outputs from the background render thread are disabled (`false`) to ensure silent, zero-overhead production builds.

You can enable or disable real-time native performance logs and kernel `/proc` thread profiler diagnostics using `Pvg.isLoggingEnabled`:

```kotlin
import com.pvg.android.Pvg

class MainApplication : Application() {
    override fun onCreate() {
        super.onCreate()

        // Enable native ANativeWindow & 1-second profiler logs in debug builds
        Pvg.isLoggingEnabled = BuildConfig.DEBUG
    }
}
```

When enabled, the native engine prints 1-second performance summaries and kernel CPU usage breakdowns under the `PVG_NATIVE` Logcat tag:

```text
I/PVG_NATIVE: 📊 [NATIVE 1s LOG] FPS: 60.0 | Eval: 14.2µs | Raster: 0.42ms | Lock: 12.1µs | Post: 18.3µs | Buf: 480x480
I/PVG_NATIVE: ┌────────────────────────────────────────────────────────────────────────────────────────────────────────┐
I/PVG_NATIVE: │ 🔍 [KERNEL /proc THREAD-LEVEL CPU PROFILER (1s)]                                                       │
I/PVG_NATIVE: ├────────────────────────────────────────────────────────────────────────────────────────────────────────┤
I/PVG_NATIVE: │ TID 18423 Thread-2             │ User:  2.4% │ Sys:  0.2% │ Total:  2.6% │ VolCtx:   60/s │ InvolCtx:   2/s
I/PVG_NATIVE: └────────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Quickstart Guide

### 1. Jetpack Compose (`PvgView`)

Drop `PvgView` directly into any Compose layout:

```kotlin
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.pvg.android.PvgView

@Composable
fun PulsingCircleSample() {
    val pvgCode = """
        PVG 0.1
        canvas 400 400
          background #080a0f

        set pulse = 60 + 20 * sin(time * 4.0)

        circle
          center [200, 200]
          radius pulse
          fill #00ffcc
          stroke #ffffff
          width 2.0
    """.trimIndent()

    PvgView(
        source = pvgCode,
        modifier = Modifier.size(300.dp)
    )
}
```

---

### 2. Controller & Animation State (`PvgController`)

Use `rememberPvgController` for dynamic document swapping, scrubbing, speed changes, and interactive controls:

```kotlin
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.pvg.android.PvgView
import com.pvg.android.rememberPvgController

@Composable
fun InteractiveRadarScreen() {
    val radarSource = """
        PVG 0.1
        canvas 600 600
          background #080a0f

        set cx = 300
        set cy = 300
        set sweep = time * 2.0

        for r_idx from 1 to 4
          circle
            center [cx, cy]
            radius r_idx * 55
            fill none
            stroke #103b42
            width 1.5

        for trail from 0 to 20
          set a = sweep - trail * 0.035
          line
            from [cx, cy]
            to   [cx + 230 * cos(a), cy + 230 * sin(a)]
            stroke #00ffcc
            width 2
            opacity (1.0 - trail / 20) * 0.45

        line
          from [cx, cy]
          to   [cx + 230 * cos(sweep), cy + 230 * sin(sweep)]
          stroke #ffffff
          width 2.5
    """.trimIndent()

    val controller = rememberPvgController(
        source = radarSource,
        isPlaying = true,
        speed = 1.0
    )

    Column(modifier = Modifier.fillMaxSize().padding(16.dp)) {
        PvgView(
            source = controller.source,
            controller = controller,
            modifier = Modifier.weight(1f).fillMaxWidth()
        )

        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = { controller.toggle() }) {
                Text(if (controller.isPlaying) "Pause" else "Play")
            }
            Button(onClick = { controller.reset() }) {
                Text("Reset")
            }
            Button(onClick = { controller.setPlaybackSpeed(2.0) }) {
                Text("2x Speed")
            }
        }
    }
}
```

---

### 3. Traditional XML Layouts (`PvgSurfaceView`)

For View-based layouts and non-Compose applications:

In XML (`res/layout/activity_main.xml`):

```xml
<?xml version="1.0" encoding="utf-8"?>
<FrameLayout xmlns:android="http://schemas.android.com/apk/res/android"
    android:layout_width="match_parent"
    android:layout_height="match_parent"
    android:background="#08090D">

    <com.pvg.android.PvgSurfaceView
        android:id="@+id/pvgSurfaceView"
        android:layout_width="320dp"
        android:layout_height="320dp"
        android:layout_gravity="center" />

</FrameLayout>
```

In Activity or Fragment:

```kotlin
import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import com.pvg.android.PvgSurfaceView

class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)

        val pvgView = findViewById<PvgSurfaceView>(R.id.pvgSurfaceView)

        val pvgCode = """
            PVG 0.1
            canvas 400 400
              background #111116

            circle
              center [200, 200]
              radius 80
              fill #ff3355
              stroke #ffffff
              width 3
        """.trimIndent()

        pvgView.setSource(pvgCode, isPlaying = true)
    }
}
```

---

## Complete API Reference

### `Pvg` (Global Configuration)

Global configuration object for controlling runtime-wide behaviors:

```kotlin
object Pvg {
    var isLoggingEnabled: Boolean
}
```

- `isLoggingEnabled`: Controls whether native `PVG_NATIVE` Logcat diagnostics and `/proc` thread profiler statistics are output. (Default: `false`).

---

### `PvgView` (Composable)

Jetpack Compose component for hardware-backed vector rendering:

```kotlin
@Composable
fun PvgView(
    source: String,
    modifier: Modifier = Modifier,
    controller: PvgController? = null,
    isPlaying: Boolean = true,
    speed: Double = 1.0,
    time: Double = 0.0
)
```

Parameters:
- `source`: PVG document source text string (Mandatory).
- `modifier`: Modifier applied to the underlying viewport layout.
- `controller`: Optional external `PvgController` instance.
- `isPlaying`: Controls real-time animation playback (default: `true`).
- `speed`: Playback speed multiplier (default: `1.0`).
- `time`: Manual timeline scrub position in seconds (default: `0.0`).

---

### `PvgController`

A stateful, lifecycle-aware controller for programmatic scene manipulation:

```kotlin
class PvgController(
    initialSource: String = "",
    initialPlaying: Boolean = true,
    initialSpeed: Double = 1.0
) : Closeable
```

Methods and Properties:
- `source`: Active PVG source code (observable Compose State).
- `isPlaying`: Whether the animation timeline is running.
- `speed`: Current playback speed multiplier.
- `currentTime`: Current timeline position in seconds.
- `load(pvgCode)`: Dynamically updates and re-evaluates a new PVG document.
- `play()`: Starts real-time timeline playback.
- `pause()`: Pauses timeline playback.
- `toggle()`: Toggles between play and pause.
- `reset()`: Resets timeline clock back to `0.0s`.
- `seekTo(timeSeconds)`: Seeks directly to an arbitrary timestamp in seconds.
- `setPlaybackSpeed(speed)`: Updates timeline speed multiplier (e.g. `0.5`, `1.0`, `2.0`).
- `getTelemetry()`: Reads latest per-frame performance metrics.
- `close()`: Releases native render thread and C/Rust memory arena.

---

### `rememberPvgController()`

```kotlin
@Composable
fun rememberPvgController(
    source: String = "",
    isPlaying: Boolean = true,
    speed: Double = 1.0
): PvgController
```

Creates and manages the lifecycle of a `PvgController`, automatically calling `close()` when the Composable leaves composition.

---

### `PvgSurfaceView` (Android View)

```kotlin
class PvgSurfaceView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = 0
) : SurfaceView(context, attrs, defStyleAttr), SurfaceHolder.Callback
```

Methods:
- `setSource(code, isPlaying, speed)`: Loads a PVG document with playback settings.
- `play()` / `pause()`: Controls timeline playback.
- `seekTo(timeSeconds)`: Seeks timeline to a timestamp in seconds.
- `setPlaybackSpeed(speed)`: Adjusts playback speed.
- `getTelemetry()`: Retrieves latest telemetry metrics.

---

### `PvgTelemetry`

Real-time diagnostic metrics captured per frame:

```kotlin
data class PvgTelemetry(
    val parseUs: Double = 0.0,
    val evalUs: Double = 0.0,
    val rasterUs: Double = 0.0,
    val fps: Double = 60.0,
    val primitiveCount: Int = 0
)
```

Fields:
- `parseUs`: AST parse latency in microseconds (cached after initial load).
- `evalUs`: Procedural evaluation latency per frame tick in microseconds.
- `rasterUs`: Direct in-place `ANativeWindow` rasterization latency in microseconds.
- `fps`: Real-time measured render frame rate.
- `primitiveCount`: Total number of 2D vector shapes drawn in the scene.

---

### `PvgEngine` (Low-Level JNI)

```kotlin
class PvgEngine(source: String, isPlaying: Boolean, speed: Double) : Closeable
```

Low-level direct wrapper around `libpvg_android.so` for custom surface pipelines and non-standard view setups.

---

## Real-Time Telemetry & Diagnostics

Inspect live microsecond execution stats:

```kotlin
@Composable
fun TelemetryDisplay(controller: PvgController) {
    var telemetry by remember { mutableStateOf(PvgTelemetry()) }

    LaunchedEffect(Unit) {
        while (true) {
            telemetry = controller.getTelemetry()
            kotlinx.coroutines.delay(500)
        }
    }

    Text(
        text = "${telemetry.fps.toInt()} FPS | Eval: ${telemetry.evalUs.toInt()}µs | Raster: ${(telemetry.rasterUs / 1000.0).format(2)}ms | Shapes: ${telemetry.primitiveCount}",
        fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace
    )
}
```

---

## Native Kernel Profiling & Logcat Output

When `Pvg.isLoggingEnabled = true`, the engine monitors kernel execution metrics directly from Linux `/proc/self/task` to provide thread-isolated CPU profiling without requiring external profilers:

```text
I/PVG_NATIVE: 🖼️ [SURFACE CREATED] ANativeWindow handle = 0x762f029000
I/PVG_NATIVE: 📐 [SURFACE CHANGED] Dimensions: 480x480
I/PVG_NATIVE: 🔄 [SOURCE UPDATE] Re-parsed AST in 18.50 µs (Animated: true)
I/PVG_NATIVE: 📊 [NATIVE 1s LOG] FPS: 60.0 | Eval: 14.1µs | Raster: 0.38ms | Lock: 8.2µs | Post: 14.5µs | Buf: 480x480
```

---

## Architecture & Memory Model

```
[ PVG Source Code ]
        │
        ▼ (One-Time Parse to AST)
[ Cached Document AST ] (~8–16 KB)
        │
        ▼ (Evaluate per frame tick @ 60 FPS)
[ Flat 2D Draw List ] (~15–35 KB)
        │
        ▼ (tiny-skia zero-copy rasterization)
[ ANativeWindow Surface Buffer ] (Pure CPU direct lock & post)
```

1. **AST Caching:** When animations run at 60 FPS, the document is parsed once. Subsequent frames only re-evaluate the AST against the timeline clock `time` in under **40 microseconds**, avoiding string allocations.
2. **Zero GPU Lock-In:** Hardware rasterization occurs on a dedicated background native thread directly into Android's `ANativeWindow`, bypassing the Main UI thread and Android's HWUI RenderThread.

---

## Supported Architectures & ProGuard

Native ABIs:
- `arm64-v8a` (Physical 64-bit Android devices)
- `x86_64` (Android Studio PC Emulator)

Built-in ProGuard Rules:
The library automatically includes consumer rules for JNI preservation:

```pro
-keep class com.pvg.android.** { *; }
-keepclasseswithmembernames class * {
    native <methods>;
}
```

---

## License

Licensed under the Apache License, Version 2.0.