package com.pvg.android

import android.content.Context
import android.graphics.PixelFormat
import android.util.AttributeSet
import android.view.SurfaceHolder
import android.view.SurfaceView

/**
 * Traditional Android View component for non-Compose XML layouts and Java/Kotlin activities.
 *
 * Example XML usage:
 * ```xml
 * <com.pvg.android.PvgSurfaceView
 *     android:id="@+id/pvgSurfaceView"
 *     android:layout_width="match_parent"
 *     android:layout_height="match_parent" />
 * ```
 */
class PvgSurfaceView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = 0
) : SurfaceView(context, attrs, defStyleAttr), SurfaceHolder.Callback {

    private var engine: PvgEngine? = null

    init {
        holder.setFormat(PixelFormat.RGBA_8888)
        holder.addCallback(this)
        setZOrderMediaOverlay(true)
    }

    fun setSource(pvgCode: String, isPlaying: Boolean = true, speed: Double = 1.0) {
        if (engine == null) {
            engine = PvgEngine(pvgCode, isPlaying, speed)
            if (holder.surface.isValid) {
                engine?.onSurfaceCreated(holder.surface)
            }
        } else {
            engine?.setSource(pvgCode)
            engine?.setPlaying(isPlaying)
            engine?.setSpeed(speed)
        }
    }

    fun play() {
        engine?.setPlaying(true)
    }

    fun pause() {
        engine?.setPlaying(false)
    }

    fun seekTo(time: Double) {
        engine?.setTime(time)
    }

    fun setPlaybackSpeed(speed: Double) {
        engine?.setSpeed(speed)
    }

    fun getTelemetry(): PvgTelemetry {
        return engine?.getTelemetry() ?: PvgTelemetry()
    }

    override fun surfaceCreated(holder: SurfaceHolder) {
        if (holder.surface.isValid) {
            engine?.onSurfaceCreated(holder.surface)
        }
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        engine?.onSurfaceChanged(width, height)
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        engine?.onSurfaceDestroyed()
    }

    override fun onDetachedFromWindow() {
        super.onDetachedFromWindow()
        engine?.close()
        engine = null
    }
}