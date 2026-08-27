package com.pvg.pvg

import android.util.Log
import android.view.Surface
import java.io.Closeable

data class PvgTelemetry(
    val parseUs: Double = 0.0,
    val evalUs: Double = 0.0,
    val rasterUs: Double = 0.0,
    val fps: Double = 60.0,
    val primitiveCount: Int = 0
)

class PvgEngine(
    initialSource: String,
    isPlaying: Boolean = true,
    speed: Double = 1.0
) : Closeable {

    private var nativeHandle: Long = 0

    init {
        nativeHandle = nativeInit(initialSource, isPlaying, speed)
        Log.i(TAG, "Initialized PvgEngine instance (nativeHandle = $nativeHandle)")
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

    fun onSurfaceCreated(surface: Surface) {
        if (nativeHandle != 0L) {
            Log.i(TAG, "Dispatching onSurfaceCreated to Native JNI")
            nativeOnSurfaceCreated(nativeHandle, surface)
        }
    }

    fun onSurfaceChanged(width: Int, height: Int) {
        if (nativeHandle != 0L) {
            Log.i(TAG, "Dispatching onSurfaceChanged (${width}x${height}) to Native JNI")
            nativeOnSurfaceChanged(nativeHandle, width, height)
        }
    }

    fun onSurfaceDestroyed() {
        if (nativeHandle != 0L) {
            Log.i(TAG, "Dispatching onSurfaceDestroyed to Native JNI")
            nativeOnSurfaceDestroyed(nativeHandle)
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
                primitiveCount = data[4].toInt()
            )
        } else {
            PvgTelemetry()
        }
    }

    override fun close() {
        if (nativeHandle != 0L) {
            Log.i(TAG, "Destroying PvgEngine native instance ($nativeHandle)")
            nativeDestroy(nativeHandle)
            nativeHandle = 0L
        }
    }

    protected fun finalize() {
        close()
    }

    companion object {
        private const val TAG = "PVG_KOTLIN"

        init {
            System.loadLibrary("pvg_android")
            Log.i(TAG, "Loaded native library libpvg_android.so successfully")
        }

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
        private external fun nativeOnSurfaceCreated(handle: Long, surface: Surface)

        @JvmStatic
        private external fun nativeOnSurfaceChanged(handle: Long, width: Int, height: Int)

        @JvmStatic
        private external fun nativeOnSurfaceDestroyed(handle: Long)

        @JvmStatic
        private external fun nativeGetTelemetry(handle: Long): DoubleArray
    }
}