package com.pvg.android

import android.content.Context
import android.graphics.PixelFormat
import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView

/**
 * High-performance Jetpack Compose viewport for rendering Procedural Vector Graphics (PVG).
 * Renders directly onto a dedicated hardware [SurfaceView] backed by `ANativeWindow`
 * with 0% Main UI thread and 0% GPU overhead.
 */
@Composable
fun PvgView(
    source: String,
    modifier: Modifier = Modifier,
    controller: PvgController? = null,
    isPlaying: Boolean = true,
    speed: Double = 1.0,
    time: Double = 0.0
) {
    val activeController = controller ?: rememberPvgController(
        source = source,
        isPlaying = isPlaying,
        speed = speed
    )

    LaunchedEffect(source) {
        activeController.load(source)
    }

    LaunchedEffect(isPlaying) {
        if (isPlaying) activeController.play() else activeController.pause()
    }

    LaunchedEffect(speed) {
        activeController.setPlaybackSpeed(speed)
    }

    LaunchedEffect(time) {
        activeController.seekTo(time)
    }

    AndroidView(
        modifier = modifier.fillMaxSize(),
        factory = { context: Context ->
            SurfaceView(context).apply {
                holder.setFormat(PixelFormat.RGBA_8888)
                setZOrderMediaOverlay(true)

                holder.addCallback(object : SurfaceHolder.Callback {
                    override fun surfaceCreated(holder: SurfaceHolder) {
                        if (holder.surface.isValid) {
                            activeController.engine.onSurfaceCreated(holder.surface)
                        }
                    }

                    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
                        activeController.engine.onSurfaceChanged(width, height)
                    }

                    override fun surfaceDestroyed(holder: SurfaceHolder) {
                        activeController.engine.onSurfaceDestroyed()
                    }
                })
            }
        }
    )
}