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
        // Opaque buffer (no alpha) so SurfaceFlinger can skip blending.
        holder.setFormat(PixelFormat.RGBX_8888)
        holder.addCallback(this)
        setZOrderMediaOverlay(true)
    }

    fun setSource(pvgCode: String, isPlaying: Boolean = true, speed: Double = 1.0) {
        if (engine == null) {
            engine = PvgEngine(pvgCode, isPlaying, speed)
            if (holder.surface.isValid) {
                engine?.onSurfaceCreated(holder.surface)
                if (isPlaying) engine?.startVsync()
            }
        } else {
            engine?.setSource(pvgCode)
            engine?.setPlaying(isPlaying)
            engine?.setSpeed(speed)
            if (isPlaying) engine?.startVsync() else engine?.stopVsync()
        }
    }

    fun play() {
        engine?.setPlaying(true)
        engine?.startVsync()
    }

    fun pause() {
        engine?.setPlaying(false)
        engine?.stopVsync()
    }

    fun seekTo(time: Double) {
        engine?.setTime(time)
    }

    fun setPlaybackSpeed(speed: Double) {
        engine?.setSpeed(speed)
    }

    /**
     * Sets a host uniform (`param`, PVG 0.2 §18.1) declared by the document.
     * Numeric params only; string params must be baked into the source.
     */
    fun setParam(name: String, value: Double) {
        engine?.setParam(name, value)
    }

    /** Clears a host uniform override so the document default applies again. */
    fun clearParam(name: String) {
        engine?.clearParam(name)
    }

    /** Names of the `param` declarations in the loaded document. */
    fun paramNames(): List<String> {
        return engine?.paramNames() ?: emptyList()
    }

    fun getTelemetry(): PvgTelemetry {
        return engine?.getTelemetry() ?: PvgTelemetry()
    }

    /** Latest native parse/eval failure for the current source, or "" when healthy. */
    fun getLastError(): String {
        return engine?.getLastError() ?: ""
    }

    override fun surfaceCreated(holder: SurfaceHolder) {
        if (holder.surface.isValid) {
            engine?.onSurfaceCreated(holder.surface)
            engine?.startVsync()
        }
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        engine?.onSurfaceChanged(width, height)
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        engine?.stopVsync()
        engine?.onSurfaceDestroyed()
    }

    override fun onDetachedFromWindow() {
        super.onDetachedFromWindow()
        engine?.close()
        engine = null
    }
}