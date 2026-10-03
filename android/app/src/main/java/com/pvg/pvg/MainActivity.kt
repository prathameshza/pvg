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
import androidx.compose.ui.text.TextStyle
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

    // PVG 0.2 §18.1 host uniforms demo: numeric `param`s driven from Kotlin.
    // Defaults mirror the `param ...:` declarations in the 0.2 preset.
    // (String params like `pilot_tag` keep their document defaults; the JNI
    // bridge carries numeric uniforms only.)
    var hullHp by remember { mutableDoubleStateOf(0.65) }
    var shieldPower by remember { mutableDoubleStateOf(0.82) }
    var fluxTemp by remember { mutableDoubleStateOf(74.0) }

    // Bottom tabs: 0 = live preview, 1 = PVG source editor.
    var tab by remember { mutableIntStateOf(0) }
    // Editable copy of the active preset; resets when the preset changes.
    var draft by remember(currentPreset.code) { mutableStateOf(currentPreset.code) }

    fun applyPresetParams(code: String) {
        if (code.contains("param hull_hp")) controller.setParam("hull_hp", hullHp)
        if (code.contains("param shield_power")) controller.setParam("shield_power", shieldPower)
        if (code.contains("param flux_temp")) controller.setParam("flux_temp", fluxTemp)
    }

    LaunchedEffect(currentPreset.code) {
        applyPresetParams(currentPreset.code)
    }

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
        },
        bottomBar = {
            NavigationBar(
                containerColor = Color(0xFF11141D)
            ) {
                NavigationBarItem(
                    selected = tab == 0,
                    onClick = { tab = 0 },
                    icon = { Text("▶", fontSize = 16.sp, color = if (tab == 0) Color(0xFF00FFCC) else Color(0xFF8F96B0)) },
                    label = { Text("Preview", fontSize = 11.sp) },
                    colors = NavigationBarItemDefaults.colors(
                        selectedTextColor = Color(0xFF00FFCC),
                        unselectedTextColor = Color(0xFF8F96B0),
                        indicatorColor = Color(0xFF00D2FF).copy(alpha = 0.2f)
                    )
                )
                NavigationBarItem(
                    selected = tab == 1,
                    onClick = { tab = 1 },
                    icon = { Text("</>", fontFamily = FontFamily.Monospace, fontSize = 14.sp, color = if (tab == 1) Color(0xFF00FFCC) else Color(0xFF8F96B0)) },
                    label = { Text("Code", fontSize = 11.sp) },
                    colors = NavigationBarItemDefaults.colors(
                        selectedTextColor = Color(0xFF00FFCC),
                        unselectedTextColor = Color(0xFF8F96B0),
                        indicatorColor = Color(0xFF00D2FF).copy(alpha = 0.2f)
                    )
                )
            }
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
                            applyPresetParams(preset.code)
                            if (preset.isAnimated) controller.play() else controller.pause()
                            controller.reset()
                            tab = 0
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

            if (tab == 0) {
            // Native parse/eval failures (e.g. pasted code with tabs) show
            // here instead of failing silently with a black viewport.
            ErrorBanner(error = controller.lastError)

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

            // 3b. PVG 0.2 host uniforms (§18.1) — only for presets declaring params.
            if (currentPreset.code.contains("param hull_hp")
                || currentPreset.code.contains("param shield_power")
                || currentPreset.code.contains("param flux_temp")
            ) {
                Surface(
                    modifier = Modifier.fillMaxWidth(),
                    color = Color(0xFF0D0E15),
                    border = androidx.compose.foundation.BorderStroke(1.dp, Color(0xFF181B26))
                ) {
                    Column(
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
                        verticalArrangement = Arrangement.spacedBy(4.dp)
                    ) {
                        Text(
                            "HOST UNIFORMS • param",
                            fontFamily = FontFamily.Monospace,
                            fontSize = 10.sp,
                            color = Color(0xFF00D2FF)
                        )
                        if (currentPreset.code.contains("param hull_hp")) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(
                                    "hull_hp ${String.format("%.2f", hullHp)}",
                                    modifier = Modifier.width(110.dp),
                                    fontFamily = FontFamily.Monospace,
                                    fontSize = 11.sp,
                                    color = Color(0xFFC5C6C7)
                                )
                                Slider(
                                    value = hullHp.toFloat(),
                                    onValueChange = {
                                        hullHp = it.toDouble()
                                        controller.setParam("hull_hp", hullHp)
                                    },
                                    valueRange = 0f..1f,
                                    modifier = Modifier.weight(1f)
                                )
                            }
                        }
                        if (currentPreset.code.contains("param shield_power")) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(
                                    "shield ${String.format("%.2f", shieldPower)}",
                                    modifier = Modifier.width(110.dp),
                                    fontFamily = FontFamily.Monospace,
                                    fontSize = 11.sp,
                                    color = Color(0xFFC5C6C7)
                                )
                                Slider(
                                    value = shieldPower.toFloat(),
                                    onValueChange = {
                                        shieldPower = it.toDouble()
                                        controller.setParam("shield_power", shieldPower)
                                    },
                                    valueRange = 0f..1f,
                                    modifier = Modifier.weight(1f)
                                )
                            }
                        }
                        if (currentPreset.code.contains("param flux_temp")) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(
                                    "flux ${fluxTemp.toInt()} °C",
                                    modifier = Modifier.width(110.dp),
                                    fontFamily = FontFamily.Monospace,
                                    fontSize = 11.sp,
                                    color = Color(0xFFC5C6C7)
                                )
                                Slider(
                                    value = fluxTemp.toFloat(),
                                    onValueChange = {
                                        fluxTemp = it.toDouble()
                                        controller.setParam("flux_temp", fluxTemp)
                                    },
                                    valueRange = 20f..120f,
                                    modifier = Modifier.weight(1f)
                                )
                            }
                        }
                    }
                }
            }

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
            } else {
                // Code editor: full PVG source with Apply / Restore.
                CodePane(
                    draft = draft,
                    isDirty = draft != currentPreset.code,
                    error = controller.lastError,
                    controller = controller,
                    onDraftChange = { draft = it },
                    onApply = {
                        controller.load(draft)
                        controller.reset()
                        controller.refreshError()
                        tab = 0
                    },
                    onRestore = { draft = currentPreset.code }
                )
            }
        }
    }
}

@Composable
fun ErrorBanner(error: String) {
    if (error.isEmpty()) return
    Surface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 6.dp),
        shape = RoundedCornerShape(8.dp),
        color = Color(0xFF3A0D0D),
        border = androidx.compose.foundation.BorderStroke(1.dp, Color(0xFFFF5555).copy(alpha = 0.6f))
    ) {
        Text(
            text = "⚠ $error",
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
            fontFamily = FontFamily.Monospace,
            fontSize = 11.sp,
            color = Color(0xFFFF8888)
        )
    }
}

@Composable
fun CodePane(
    draft: String,
    isDirty: Boolean,
    error: String,
    controller: PvgController,
    onDraftChange: (String) -> Unit,
    onApply: () -> Unit,
    onRestore: () -> Unit
) {
    // Keep the native error fresh while the editor is open.
    LaunchedEffect(controller) {
        while (true) {
            controller.refreshError()
            delay(1000L)
        }
    }
    val lines = draft.lines().size
    val kb = draft.toByteArray().size / 1024
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            Text(
                text = "PVG SOURCE",
                fontFamily = FontFamily.Monospace,
                fontSize = 11.sp,
                fontWeight = FontWeight.Bold,
                color = Color(0xFF00D2FF)
            )
            Text(
                text = "$lines lines • $kb KB",
                fontFamily = FontFamily.Monospace,
                fontSize = 11.sp,
                color = Color(0xFF8F96B0)
            )
            Spacer(modifier = Modifier.weight(1f))
            if (isDirty) {
                Surface(
                    shape = RoundedCornerShape(4.dp),
                    color = Color(0xFFFFAA00).copy(alpha = 0.15f),
                    border = androidx.compose.foundation.BorderStroke(1.dp, Color(0xFFFFAA00).copy(alpha = 0.5f))
                ) {
                    Text(
                        text = "• edited",
                        modifier = Modifier.padding(horizontal = 8.dp, vertical = 2.dp),
                        fontFamily = FontFamily.Monospace,
                        fontSize = 10.sp,
                        color = Color(0xFFFFAA00)
                    )
                }
            }
        }

        if (error.isNotEmpty()) {
            Surface(
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(8.dp),
                color = Color(0xFF3A0D0D),
                border = androidx.compose.foundation.BorderStroke(1.dp, Color(0xFFFF5555).copy(alpha = 0.6f))
            ) {
                Text(
                    text = "⚠ $error",
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
                    fontFamily = FontFamily.Monospace,
                    fontSize = 11.sp,
                    color = Color(0xFFFF8888)
                )
            }
        }

        OutlinedTextField(
            value = draft,
            onValueChange = onDraftChange,
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth(),
            textStyle = TextStyle(
                fontFamily = FontFamily.Monospace,
                fontSize = 11.sp,
                lineHeight = 15.sp,
                color = Color(0xFFE8EAF2)
            ),
            shape = RoundedCornerShape(8.dp),
            maxLines = Int.MAX_VALUE,
            colors = OutlinedTextFieldDefaults.colors(
                focusedBorderColor = Color(0xFF00D2FF),
                unfocusedBorderColor = Color(0xFF282C3F),
                focusedContainerColor = Color(0xFF0D0E15),
                unfocusedContainerColor = Color(0xFF0D0E15),
                cursorColor = Color(0xFF00FFCC)
            )
        )

        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(10.dp)
        ) {
            Button(
                onClick = onApply,
                colors = ButtonDefaults.buttonColors(containerColor = Color(0xFF00A854)),
                shape = RoundedCornerShape(8.dp),
                modifier = Modifier.weight(1f)
            ) {
                Text("▶ Apply & Preview", fontSize = 12.sp)
            }
            OutlinedButton(
                onClick = onRestore,
                colors = ButtonDefaults.outlinedButtonColors(contentColor = Color(0xFF8F96B0)),
                border = androidx.compose.foundation.BorderStroke(1.dp, Color(0xFF282C3F)),
                shape = RoundedCornerShape(8.dp)
            ) {
                Text("Restore", fontSize = 12.sp)
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
            controller.refreshError()
            val runtime = Runtime.getRuntime()
            val usedMemMb = (runtime.totalMemory() - runtime.freeMemory()) / (1024 * 1024)
            Log.i(
                "PVG_KOTLIN",
                "📱 [KOTLIN UI 1s LOG] Engine: ${t.fps.toInt()} FPS | Shapes: ${t.primitiveCount} | Eval: ${String.format("%.1f", t.evalUs)}µs | Raster: ${String.format("%.2f", t.rasterUs / 1000.0)}ms | Lock: ${String.format("%.2f", t.lockUs / 1000.0)}ms | JVM Heap: ${usedMemMb}MB"
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
                    "Lock ${String.format("%.2f", telemetry.lockUs / 1000.0)} • Post ${String.format("%.2f", telemetry.postUs / 1000.0)} ms",
                    fontFamily = FontFamily.Monospace,
                    fontSize = 11.sp,
                    color = Color(0xFF8F96B0)
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