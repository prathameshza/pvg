package com.pvg.android

import android.util.Log
import android.view.Choreographer
import android.view.Surface
import java.io.Closeable

/**
 * Low-level JNI interface bridging to the native Rust engine (`libpvg_android.so`).
 *
 * Frame pacing is display-locked: [startVsync] posts a [Choreographer]
 * callback that forwards every panel tick to the native render thread via
 * [nativeOnVsync]. This phase-locks output to 60/90/120Hz — a free-running
 * timer can never hold 60 FPS (timer slack + drift against SurfaceFlinger).
 */
class PvgEngine(
    initialSource: String,
    isPlaying: Boolean = true,
    speed: Double = 1.0
) : Closeable {

    private var nativeHandle: Long = 0

    private val choreographer: Choreographer = Choreographer.getInstance()

    @Volatile
    private var vsyncRunning = false

    private val frameCallback = object : Choreographer.FrameCallback {
        override fun doFrame(frameTimeNanos: Long) {
            if (!vsyncRunning || nativeHandle == 0L) return
            nativeOnVsync(nativeHandle, frameTimeNanos)
            if (vsyncRunning) choreographer.postFrameCallback(this)
        }
    }

    /** Starts display-locked ticks; idempotent. Call on the UI thread. */
    fun startVsync() {
        if (vsyncRunning) return
        vsyncRunning = true
        choreographer.postFrameCallback(frameCallback)
    }

    /** Stops display ticks; idempotent. Call on the UI thread. */
    fun stopVsync() {
        vsyncRunning = false
        choreographer.removeFrameCallback(frameCallback)
    }

    init {
        nativeHandle = nativeInit(initialSource, isPlaying, speed)
        Log.i(TAG, "Initialized native PvgEngine (handle = $nativeHandle)")
    }

    fun setSource(source: String) {
        if (nativeHandle != 0L) {
            nativeSetSource(nativeHandle, source)
        }
    }

    fun setPlaying(playing: Boolean) {
        if (nativeHandle != 0L) {
            nativeSetPlaying(nativeHandle, playing)
        }
    }

    fun setTime(time: Double) {
        if (nativeHandle != 0L) {
            nativeSetTime(nativeHandle, time)
        }
    }

    fun setSpeed(speed: Double) {
        if (nativeHandle != 0L) {
            nativeSetSpeed(nativeHandle, speed)
        }
    }

    /**
     * Sets a host uniform (`param`, PVG 0.2 spec section 18.1) declared by the
     * current source. Overrides the document's default value and re-renders the
     * next frame.
     */
    fun setParam(name: String, value: Double) {
        if (nativeHandle != 0L) {
            nativeSetParam(nativeHandle, name, value)
        }
    }

    /** Clears a host uniform override so the document default applies again. */
    fun clearParam(name: String) {
        if (nativeHandle != 0L) {
            nativeClearParam(nativeHandle, name)
        }
    }

    /** Names of the `param` declarations in the current source. */
    fun paramNames(): List<String> {
        if (nativeHandle == 0L) return emptyList()
        val arr = nativeGetParamNames(nativeHandle) ?: return emptyList()
        return (0 until arr.size).map { arr[it] }
    }

    fun onSurfaceCreated(surface: Surface) {
        if (nativeHandle != 0L) {
            nativeOnSurfaceCreated(nativeHandle, surface)
        }
    }

    fun onSurfaceChanged(width: Int, height: Int) {
        if (nativeHandle != 0L) {
            nativeOnSurfaceChanged(nativeHandle, width, height)
        }
    }

    fun onSurfaceDestroyed() {
        if (nativeHandle != 0L) {
            nativeOnSurfaceDestroyed(nativeHandle)
        }
    }

    /** Latest parse/eval failure for the current source, or "" when healthy. */
    fun getLastError(): String {
        if (nativeHandle == 0L) return ""
        return try {
            nativeGetLastError(nativeHandle) ?: ""
        } catch (_: Exception) {
            ""
        }
    }

    fun getTelemetry(): PvgTelemetry {
        if (nativeHandle == 0L) return PvgTelemetry()
        val data = nativeGetTelemetry(nativeHandle)
        return if (data.size >= 5) {
            PvgTelemetry(
                parseUs = data[0],
                evalUs = data[1],
                rasterUs = data[2],
                fps = data[3],
                primitiveCount = data[4].toInt(),
                lockUs = if (data.size >= 7) data[5] else 0.0,
                postUs = if (data.size >= 7) data[6] else 0.0
            )
        } else {
            PvgTelemetry()
        }
    }

    override fun close() {
        stopVsync()
        if (nativeHandle != 0L) {
            Log.i(TAG, "Releasing native PvgEngine ($nativeHandle)")
            nativeDestroy(nativeHandle)
            nativeHandle = 0L
        }
    }

    protected fun finalize() {
        close()
    }

    companion object {
        private const val TAG = "PVG_NATIVE"

        init {
            System.loadLibrary("pvg_android")
        }

        fun setLoggingEnabled(enabled: Boolean) {
            nativeSetLoggingEnabled(enabled)
        }

        @JvmStatic
        private external fun nativeSetLoggingEnabled(enabled: Boolean)

        @JvmStatic
        private external fun nativeInit(source: String, isPlaying: Boolean, speed: Double): Long

        @JvmStatic
        private external fun nativeDestroy(handle: Long)

        @JvmStatic
        private external fun nativeSetSource(handle: Long, source: String)

        @JvmStatic
        private external fun nativeSetPlaying(handle: Long, playing: Boolean)

        @JvmStatic
        private external fun nativeSetTime(handle: Long, time: Double)

        @JvmStatic
        private external fun nativeSetSpeed(handle: Long, speed: Double)

        @JvmStatic
        private external fun nativeSetParam(handle: Long, name: String, value: Double)

        @JvmStatic
        private external fun nativeClearParam(handle: Long, name: String)

        @JvmStatic
        private external fun nativeGetParamNames(handle: Long): Array<String>

        @JvmStatic
        private external fun nativeOnVsync(handle: Long, frameNanos: Long)

        @JvmStatic
        private external fun nativeOnSurfaceCreated(handle: Long, surface: Surface)

        @JvmStatic
        private external fun nativeOnSurfaceChanged(handle: Long, width: Int, height: Int)

        @JvmStatic
        private external fun nativeOnSurfaceDestroyed(handle: Long)

        @JvmStatic
        private external fun nativeGetTelemetry(handle: Long): DoubleArray

        @JvmStatic
        private external fun nativeGetLastError(handle: Long): String?
    }
}