package com.pvg.pvg

import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.pvg.android.PvgController
import com.pvg.android.PvgTelemetry
import com.pvg.android.PvgView
import com.pvg.android.rememberPvgController
import com.pvg.pvg.ui.theme.PvgTheme
import kotlinx.coroutines.delay

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            PvgTheme {
                PvgAndroidStudio()
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PvgAndroidStudio() {
    var selectedPresetIndex by remember { mutableIntStateOf(0) }
    val currentPreset = Presets.list[selectedPresetIndex]

    val controller = rememberPvgController(
        source = currentPreset.code,
        isPlaying = currentPreset.isAnimated,
        speed = 1.0
    )

    Scaffold(
        modifier = Modifier.fillMaxSize(),
        containerColor = Color(0xFF08090D),
        topBar = {
            TopAppBar(
                title = {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        Text("⚡ PVG", fontWeight = FontWeight.Black, color = Color(0xFF00FFCC), fontSize = 18.sp)
                        Surface(
                            shape = RoundedCornerShape(4.dp),
                            color = Color(0xFF00D2FF).copy(alpha = 0.2f),
                            border = androidx.compose.foundation.BorderStroke(1.dp, Color(0xFF00D2FF).copy(alpha = 0.5f))
                        ) {
                            Text(
                                "PURE CPU 60 FPS",
                                modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
                                fontSize = 10.sp,
                                fontWeight = FontWeight.Bold,
                                color = Color(0xFF00D2FF)
                            )
                        }
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = Color(0xFF11141D)
                )
            )
        }
    ) { innerPadding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(innerPadding)
        ) {
            // 1. Preset Selector Tabs
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(Color(0xFF11141D))
                    .horizontalScroll(rememberScrollState())
                    .padding(horizontal = 12.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                Presets.list.forEachIndexed { index, preset ->
                    val isSelected = index == selectedPresetIndex
                    FilterChip(
                        selected = isSelected,
                        onClick = {
                            selectedPresetIndex = index
                            controller.load(preset.code)
                            if (preset.isAnimated) controller.play() else controller.pause()
                            controller.reset()
                        },
                        label = { Text(preset.name, fontSize = 12.sp) },
                        colors = FilterChipDefaults.filterChipColors(
                            selectedContainerColor = Color(0xFF00D2FF).copy(alpha = 0.25f),
                            selectedLabelColor = Color(0xFF00FFCC),
                            containerColor = Color(0xFF1B1F2C),
                            labelColor = Color(0xFF8F96B0)
                        )
                    )
                }
            }

            // 2. Interactive Native Viewport (Framed container)
            Box(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .padding(12.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .background(Color(0xFF08090D))
                    .border(1.dp, Color(0xFF1F2333), RoundedCornerShape(12.dp))
            ) {
                PvgView(
                    source = controller.source,
                    controller = controller
                )

                // Top Diagnostic Overlay Badge
                Surface(
                    shape = RoundedCornerShape(6.dp),
                    color = Color(0xFF11141D).copy(alpha = 0.85f),
                    border = androidx.compose.foundation.BorderStroke(1.dp, Color(0xFF282C3F)),
                    modifier = Modifier
                        .padding(8.dp)
                        .align(Alignment.TopEnd)
                ) {
                    Text(
                        text = "Native Direct ANativeWindow • 0 HWUI Upload • 0 GPU",
                        modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                        fontFamily = FontFamily.Monospace,
                        fontSize = 10.sp,
                        color = Color(0xFF00E676)
                    )
                }
            }

            // 3. Isolated Telemetry HUD
            TelemetryHud(controller = controller)

            // 4. Timeline Controls & Playback Speed
            Surface(
                modifier = Modifier.fillMaxWidth(),
                color = Color(0xFF11141D)
            ) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(14.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp)
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Button(
                            onClick = { controller.toggle() },
                            colors = ButtonDefaults.buttonColors(
                                containerColor = if (controller.isPlaying) Color(0xFF282C3F) else Color(0xFF00A854)
                            ),
                            shape = RoundedCornerShape(8.dp)
                        ) {
                            Text(if (controller.isPlaying) "⏸ Pause" else "▶ Play", fontSize = 12.sp)
                        }

                        Button(
                            onClick = { controller.reset() },
                            colors = ButtonDefaults.buttonColors(containerColor = Color(0xFF282C3F)),
                            shape = RoundedCornerShape(8.dp)
                        ) {
                            Text("⏮ Reset", fontSize = 12.sp)
                        }

                        Spacer(modifier = Modifier.weight(1f))

                        // Speed selection buttons
                        listOf(0.5, 1.0, 2.0).forEach { speedOption ->
                            val isCurrent = controller.speed == speedOption
                            OutlinedButton(
                                onClick = { controller.setPlaybackSpeed(speedOption) },
                                colors = ButtonDefaults.outlinedButtonColors(
                                    contentColor = if (isCurrent) Color(0xFF00FFCC) else Color(0xFF8F96B0)
                                ),
                                border = androidx.compose.foundation.BorderStroke(
                                    1.dp,
                                    if (isCurrent) Color(0xFF00FFCC) else Color(0xFF282C3F)
                                ),
                                contentPadding = PaddingValues(horizontal = 8.dp, vertical = 4.dp),
                                shape = RoundedCornerShape(6.dp)
                            ) {
                                Text("${speedOption}x", fontSize = 11.sp)
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
fun TelemetryHud(controller: PvgController) {
    var telemetry by remember { mutableStateOf(PvgTelemetry()) }

    LaunchedEffect(Unit) {
        while (true) {
            val t = controller.getTelemetry()
            telemetry = t
            val runtime = Runtime.getRuntime()
            val usedMemMb = (runtime.totalMemory() - runtime.freeMemory()) / (1024 * 1024)
            Log.i(
                "PVG_KOTLIN",
                "📱 [KOTLIN UI 1s LOG] Engine: ${t.fps.toInt()} FPS | Shapes: ${t.primitiveCount} | Eval: ${String.format("%.1f", t.evalUs)}µs | Raster: ${String.format("%.2f", t.rasterUs / 1000.0)}ms | JVM Heap: ${usedMemMb}MB"
            )
            delay(1000L)
        }
    }

    Surface(
        modifier = Modifier.fillMaxWidth(),
        color = Color(0xFF0D0E15),
        border = androidx.compose.foundation.BorderStroke(1.dp, Color(0xFF181B26))
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 10.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Column {
                Text(
                    "AST Parse: ${String.format("%.1f", telemetry.parseUs)} µs",
                    fontFamily = FontFamily.Monospace,
                    fontSize = 11.sp,
                    color = Color(0xFF00FFCC)
                )
                Text(
                    "Eval Latency: ${String.format("%.1f", telemetry.evalUs)} µs",
                    fontFamily = FontFamily.Monospace,
                    fontSize = 11.sp,
                    color = Color(0xFF00E676)
                )
            }

            Column {
                Text(
                    "In-Place Raster: ${String.format("%.2f", telemetry.rasterUs / 1000.0)} ms",
                    fontFamily = FontFamily.Monospace,
                    fontSize = 11.sp,
                    color = Color(0xFF00D2FF)
                )
                Text(
                    "Shapes: ${telemetry.primitiveCount}",
                    fontFamily = FontFamily.Monospace,
                    fontSize = 11.sp,
                    color = Color(0xFFC5C6C7)
                )
            }

            Surface(
                shape = RoundedCornerShape(4.dp),
                color = Color(0xFF00E676).copy(alpha = 0.15f),
                border = androidx.compose.foundation.BorderStroke(1.dp, Color(0xFF00E676).copy(alpha = 0.4f))
            ) {
                Text(
                    text = "${telemetry.fps.toInt()} FPS",
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp),
                    fontFamily = FontFamily.Monospace,
                    fontWeight = FontWeight.Bold,
                    fontSize = 13.sp,
                    color = Color(0xFF00E676)
                )
            }
        }
    }
}