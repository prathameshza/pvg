package com.pvg.pvg

import android.content.Context
import android.graphics.PixelFormat
import android.util.Log
import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView

@Composable
fun PvgView(
    source: String,
    isPlaying: Boolean = true,
    speed: Double = 1.0,
    time: Double = 0.0,
    engine: PvgEngine,
    modifier: Modifier = Modifier
) {
    LaunchedEffect(source) {
        engine.setSource(source)
    }

    LaunchedEffect(isPlaying) {
        engine.setPlaying(isPlaying)
    }

    LaunchedEffect(speed) {
        engine.setSpeed(speed)
    }

    LaunchedEffect(time) {
        engine.setTime(time)
    }

    // Direct hardware SurfaceView - 0% Main Thread and 0% HWUI RenderThread overhead
    AndroidView(
        modifier = modifier.fillMaxSize(),
        factory = { ctx: Context ->
            SurfaceView(ctx).apply {
                holder.setFormat(PixelFormat.RGBA_8888)
                holder.setFixedSize(480, 480)
                setZOrderMediaOverlay(true)

                holder.addCallback(object : SurfaceHolder.Callback {
                    override fun surfaceCreated(holder: SurfaceHolder) {
                        Log.i("PVG_KOTLIN", "SurfaceHolder.Callback: surfaceCreated (valid = ${holder.surface.isValid})")
                        if (holder.surface.isValid) {
                            engine.onSurfaceCreated(holder.surface)
                        }
                    }

                    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
                        Log.i("PVG_KOTLIN", "SurfaceHolder.Callback: surfaceChanged (${width}x${height})")
                        engine.onSurfaceChanged(width, height)
                    }

                    override fun surfaceDestroyed(holder: SurfaceHolder) {
                        Log.i("PVG_KOTLIN", "SurfaceHolder.Callback: surfaceDestroyed")
                        engine.onSurfaceDestroyed()
                    }
                })
            }
        }
    )
}