package com.pvg.android

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import java.io.Closeable

/**
 * State and playback controller for managing PVG animations and document sources.
 */
class PvgController(
    initialSource: String = "",
    initialPlaying: Boolean = true,
    initialSpeed: Double = 1.0
) : Closeable {

    var source by mutableStateOf(initialSource)
        private set

    var isPlaying by mutableStateOf(initialPlaying)
        private set

    var speed by mutableDoubleStateOf(initialSpeed)
        private set

    var currentTime by mutableDoubleStateOf(0.0)
        private set

    /** Latest native parse/eval failure ("" = healthy). Refresh via [refreshError]. */
    var lastError by mutableStateOf("")
        private set

    /** Host uniform overrides applied on top of the document's `param` defaults (PVG 0.2 §18.1). */
    var params by mutableStateOf<Map<String, Double>>(emptyMap())
        private set

    internal val engine = PvgEngine(initialSource, initialPlaying, initialSpeed)

    fun load(pvgCode: String) {
        source = pvgCode
        params = emptyMap()
        engine.setSource(pvgCode)
    }

    fun play() {
        isPlaying = true
        engine.setPlaying(true)
    }

    fun pause() {
        isPlaying = false
        engine.setPlaying(false)
    }

    fun toggle() {
        if (isPlaying) pause() else play()
    }

    fun setPlaybackSpeed(playbackSpeed: Double) {
        speed = playbackSpeed
        engine.setSpeed(playbackSpeed)
    }

    /**
     * Sets a host uniform (`param`, PVG 0.2 spec section 18.1) declared by the
     * document, e.g. `engine.setParam("shield", 0.42)`. Overrides the declared
     * default for every subsequent frame.
     */
    fun setParam(name: String, value: Double) {
        params = params + (name to value)
        engine.setParam(name, value)
    }

    /** Clears a host uniform override so the document default applies again. */
    fun clearParam(name: String) {
        params = params - name
        engine.clearParam(name)
    }

    /** Names of the `param` declarations in the loaded document. */
    fun paramNames(): List<String> = engine.paramNames()

    fun seekTo(timeSeconds: Double) {
        currentTime = timeSeconds
        engine.setTime(timeSeconds)
    }

    fun reset() {
        seekTo(0.0)
    }

    fun getTelemetry(): PvgTelemetry = engine.getTelemetry()

    /** Polls the native error string into [lastError] (call after load/apply and periodically). */
    fun refreshError() {
        lastError = engine.getLastError()
    }

    override fun close() {
        engine.close()
    }
}

/**
 * Creates and remembers a [PvgController] instance tied to the Composable lifecycle.
 */
@Composable
fun rememberPvgController(
    source: String = "",
    isPlaying: Boolean = true,
    speed: Double = 1.0
): PvgController {
    val controller = remember { PvgController(source, isPlaying, speed) }

    DisposableEffect(controller) {
        onDispose {
            controller.close()
        }
    }

    return controller
}