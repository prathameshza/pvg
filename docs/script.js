/**
 * PVG Landing Page Interactive Script
 * Initializes live <pvg-view> elements, telemetry sync, and preset explorer.
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

  // 1. Reference Presets from docs/pvg_web_gui/presets.js or fallback
  const presets = window.PVG_PRESETS || window.PVG?.presets || [
    {
      name: '🌀 Radar Scanner',
      code: `PVG 0.1\ncanvas 600 600\n  background #080a0f\n\nset cx = 300\nset cy = 300\nset sweep = time * 2.0\n\nfor r_idx from 1 to 4\n  circle\n    center [cx, cy]\n    radius r_idx * 55\n    fill none\n    stroke #103b42\n    width 1.5\n\nfor trail from 0 to 20\n  set a = sweep - trail * 0.035\n  line\n    from [cx, cy]\n    to   [cx + 230 * cos(a), cy + 230 * sin(a)]\n    stroke #00ffcc\n    width 2\n    opacity (1.0 - trail / 20) * 0.45\n\nline\n  from [cx, cy]\n  to   [cx + 230 * cos(sweep), cy + 230 * sin(sweep)]\n  stroke #ffffff\n  width 2.5\n\ncircle\n  center [cx, cy]\n  radius 8\n  fill #00ffcc`
    }
  ];

  let currentPresetIndex = 0;

  // 2. Initialize Hero Viewport with first preset
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

  // 3. Build Preset Explorer Tabs
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

  // 4. Load Preset Function
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

  // 5. Telemetry & Time Event Listeners on Demo View
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

  // 6. Interactive Button Controls
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

  // 7. Initialize default preset
  initPresetTabs();
  loadPreset(0);
});