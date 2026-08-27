package com.pvg.android

import org.junit.Assert.assertEquals
import org.junit.Test

class PvgControllerTest {

    @Test
    fun testPvgTelemetryDefaults() {
        val telemetry = PvgTelemetry()
        assertEquals(0.0, telemetry.parseUs, 0.001)
        assertEquals(0.0, telemetry.evalUs, 0.001)
        assertEquals(0.0, telemetry.rasterUs, 0.001)
        assertEquals(60.0, telemetry.fps, 0.001)
        assertEquals(0, telemetry.primitiveCount)
    }

    @Test
    fun testPvgTelemetryCustomValues() {
        val telemetry = PvgTelemetry(
            parseUs = 12.5,
            evalUs = 8.2,
            rasterUs = 450.0,
            fps = 59.8,
            primitiveCount = 35
        )
        assertEquals(12.5, telemetry.parseUs, 0.001)
        assertEquals(8.2, telemetry.evalUs, 0.001)
        assertEquals(450.0, telemetry.rasterUs, 0.001)
        assertEquals(59.8, telemetry.fps, 0.001)
        assertEquals(35, telemetry.primitiveCount)
    }
}