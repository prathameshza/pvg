/**
 * PVG Landing Page Interactive Script
 * Initializes live <pvg-view> elements, telemetry sync, dino comparison controls, and preset explorer.
 */

document.addEventListener('DOMContentLoaded', () => {
  const heroView = document.getElementById('hero-pvg-view');
  const demoView = document.getElementById('demo-pvg-view');
  const heroStatusPill = document.getElementById('hero-status-pill');
  const demoCodeDisplay = document.getElementById('demo-code-display');
  const demoTelemetry = document.getElementById('demo-telemetry');
  const demoTimeDisplay = document.getElementById('demo-time-display');
  const presetTabContainer = document.getElementById('preset-tab-container');
  const btnTogglePlay = document.getElementById('btn-toggle-play');
  const btnResetDemoTime = document.getElementById('btn-reset-demo-time');
  const btnCopyCode = document.getElementById('btn-copy-code');

  // Dino Comparison elements
  const dinoPvgView = document.getElementById('dino-showcase-pvg');
  const dinoSvg = document.getElementById('dino-showcase-svg');
  const btnSyncDino = document.getElementById('btn-sync-dino');
  const btnCopyDinoPvg = document.getElementById('btn-copy-dino-pvg');
  const btnCopyDinoSvg = document.getElementById('btn-copy-dino-svg');
  const codeDinoPvg = document.getElementById('code-dino-pvg');
  const codeDinoSvg = document.getElementById('code-dino-svg');

  // 1. Reference Presets from docs/pvg_web_gui/presets.js or fallback
  const presets = window.PVG_PRESETS || window.PVG?.presets || [
    {
      name: '🦖 Chrome Dino (Anim)',
      code: `PVG 0.1
canvas 80 72
  background #000000

set fg = #f97316
set t2 = time % 2.0
set in_jump = (t2 >= 0.6) and (t2 <= 1.2)
set jump_y = in_jump ? (-30 * sin(((t2 - 0.6) / 0.6) * PI)) : 0
set leg = (time % 0.2) < 0.1

rect
  pos [0.5, 0.5]
  size [79, 71]
  stroke fg
  opacity 0.2
  fill none

for i from 0 to 21
  set gx = i * 4 - ((time * 50) % 4)
  line
    from [gx, 54]
    to   [gx + 1, 54]
    stroke fg
    opacity 0.3

group
  pos [80 - (t2 / 2.0) * 140, 18]
  rect
    pos [0, 26]
    size [3, 10]
    fill fg

group
  pos [0, 30.2222 + jump_y]
  rect
    pos [17, 21.11]
    size [1.78, leg ? 2.67 : 0.89]
    fill fg
  rect
    pos [21.44, 21.11]
    size [1.78, leg ? 0.89 : 2.67]
    fill fg`
    },
    {
      name: '🌀 Radar Scanner',
      code: `PVG 0.1
canvas 600 600
  background #080a0f

set cx = 300
set cy = 300
set sweep = time * 2.0

for r_idx from 1 to 4
  circle
    center [cx, cy]
    radius r_idx * 55
    fill none
    stroke #103b42
    width 1.5

for trail from 0 to 20
  set a = sweep - trail * 0.035
  line
    from [cx, cy]
    to   [cx + 230 * cos(a), cy + 230 * sin(a)]
    stroke #00ffcc
    width 2
    opacity (1.0 - trail / 20) * 0.45

line
  from [cx, cy]
  to   [cx + 230 * cos(sweep), cy + 230 * sin(sweep)]
  stroke #ffffff
  width 2.5

circle
  center [cx, cy]
  radius 8
  fill #00ffcc`
    }
  ];

  let currentPresetIndex = 0;

  // 2. Initialize Hero Viewport with first animated preset
  if (heroView && presets.length > 0) {
    heroView.code = presets[0].code;
    heroView.play();

    heroView.addEventListener('render', (e) => {
      if (heroStatusPill && e.detail) {
        const ms = e.detail.renderTimeMs || 0.02;
        heroStatusPill.textContent = `60 FPS · ${ms.toFixed(3)} ms`;
      }
    });
  }

  // 3. Dino Comparison Sync Control
  if (btnSyncDino && dinoPvgView && dinoSvg) {
    btnSyncDino.addEventListener('click', () => {
      // Reset PVG timeline
      dinoPvgView.reset();
      dinoPvgView.play();

      // Reset CSS animations in SVG by re-inserting node
      const parent = dinoSvg.parentElement;
      const clone = dinoSvg.cloneNode(true);
      parent.replaceChild(clone, dinoSvg);
    });
  }

  if (btnCopyDinoPvg && codeDinoPvg) {
    btnCopyDinoPvg.addEventListener('click', () => {
      navigator.clipboard.writeText(codeDinoPvg.textContent).then(() => {
        const orig = btnCopyDinoPvg.textContent;
        btnCopyDinoPvg.textContent = '✓ Copied!';
        setTimeout(() => { btnCopyDinoPvg.textContent = orig; }, 2000);
      });
    });
  }

  if (btnCopyDinoSvg && codeDinoSvg) {
    btnCopyDinoSvg.addEventListener('click', () => {
      navigator.clipboard.writeText(codeDinoSvg.textContent).then(() => {
        const orig = btnCopyDinoSvg.textContent;
        btnCopyDinoSvg.textContent = '✓ Copied!';
        setTimeout(() => { btnCopyDinoSvg.textContent = orig; }, 2000);
      });
    });
  }

  // 4. Build Preset Explorer Tabs
  function initPresetTabs() {
    if (!presetTabContainer) return;
    presetTabContainer.innerHTML = '';

    presets.forEach((preset, idx) => {
      const btn = document.createElement('button');
      btn.className = `preset-tab ${idx === currentPresetIndex ? 'active' : ''}`;
      btn.textContent = preset.name;
      btn.addEventListener('click', () => loadPreset(idx));
      presetTabContainer.appendChild(btn);
    });
  }

  // 5. Load Preset Function
  function loadPreset(idx) {
    currentPresetIndex = idx;
    const selected = presets[idx];
    if (!selected) return;

    // Update active tab styles
    const tabs = presetTabContainer.querySelectorAll('.preset-tab');
    tabs.forEach((t, i) => t.classList.toggle('active', i === idx));

    // Update code viewer and live demo viewport
    demoCodeDisplay.textContent = selected.code;
    if (demoView) {
      demoView.code = selected.code;
      demoView.reset();
      demoView.play();
    }
    if (btnTogglePlay) {
      btnTogglePlay.textContent = '⏸ Pause';
    }
  }

  // 6. Telemetry & Time Event Listeners on Demo View
  if (demoView) {
    demoView.addEventListener('render', (e) => {
      const { drawList, renderTimeMs } = e.detail;
      if (demoTelemetry && drawList) {
        demoTelemetry.textContent = `Primitives: ${drawList.items.length} | Latency: ${renderTimeMs.toFixed(3)} ms`;
      }
    });

    demoView.addEventListener('timeupdate', (e) => {
      if (demoTimeDisplay && e.detail) {
        demoTimeDisplay.textContent = `${e.detail.time.toFixed(2)}s`;
      }
    });
  }

  // 7. Interactive Button Controls
  if (btnTogglePlay && demoView) {
    btnTogglePlay.addEventListener('click', () => {
      demoView.togglePlay();
      btnTogglePlay.textContent = demoView.isPlaying ? '⏸ Pause' : '▶ Play';
    });
  }

  if (btnResetDemoTime && demoView) {
    btnResetDemoTime.addEventListener('click', () => {
      demoView.reset();
      if (demoTimeDisplay) demoTimeDisplay.textContent = '0.00s';
    });
  }

  if (btnCopyCode && demoCodeDisplay) {
    btnCopyCode.addEventListener('click', () => {
      navigator.clipboard.writeText(demoCodeDisplay.textContent).then(() => {
        const originalText = btnCopyCode.textContent;
        btnCopyCode.textContent = '✓ Copied!';
        setTimeout(() => {
          btnCopyCode.textContent = originalText;
        }, 2000);
      });
    });
  }

  // 8. Initialize default preset
  initPresetTabs();
  loadPreset(0);
});