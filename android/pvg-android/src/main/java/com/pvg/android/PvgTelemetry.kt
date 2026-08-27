package com.pvg.android

/**
 * Diagnostics and real-time execution telemetry measured per frame.
 */
data class PvgTelemetry(
    /** Time taken to parse the PVG document into an AST (in microseconds). */
    val parseUs: Double = 0.0,
    /** Time taken to evaluate the AST at the current timeline clock (in microseconds). */
    val evalUs: Double = 0.0,
    /** Time taken by tiny-skia to rasterize the 2D draw commands into the ANativeWindow framebuffer (in microseconds). */
    val rasterUs: Double = 0.0,
    /** Current real-time frame rate. */
    val fps: Double = 60.0,
    /** Number of rendered 2D vector primitives in the active scene. */
    val primitiveCount: Int = 0
)