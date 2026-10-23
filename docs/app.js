(() => {
  "use strict";

  // =========================================================================
  // 1. WebMIDI Device Discovery & Auto-Selection (Mirrors Bandolibre midi.html)
  // =========================================================================
  const BANDOLIBRE_RE = /bandolibre/i;
  // Matches code/main-g474/app/keyboard.h in bandolibre/bandolibre:
  // L_MIDI_CH = 0 (1-based ch 1 = Left Wing / Bajos)
  // R_MIDI_CH = 1 (1-based ch 2 = Right Wing / Cantabile)
  const KBD_CHANNEL = { left: 1, right: 2 };

  const statusDot = document.getElementById("statusDot");
  const statusText = document.getElementById("statusText");
  const select = document.getElementById("deviceSelect");
  const bwIndicatorEl = document.getElementById("bwIndicator");
  const midiLogEl = document.getElementById("midi-log");

  let midiAccess = null;
  let activeKey = null;
  let activeName = null;
  let activeInput = null;
  let activeOutput = null;
  let bwBytesSinceTick = 0;
  let liveCcMsb = 90;
  let liveCcLsb = 0;

  setInterval(() => {
    const kbps = (bwBytesSinceTick * 8) / 1000;
    bwIndicatorEl.textContent = kbps.toFixed(1) + " kb/s";
    bwBytesSinceTick = 0;
  }, 1000);

  function isBandolibre(name) {
    return !!name && BANDOLIBRE_RE.test(name);
  }

  function setStatus(kind, text) {
    statusDot.className = "dot" + (kind ? " " + kind : "");
    statusText.textContent = text;
  }

  function logMidi(msg) {
    const line = document.createElement("div");
    line.textContent = `[${new Date().toLocaleTimeString()}] ${msg}`;
    midiLogEl.prepend(line);
    while (midiLogEl.childNodes.length > 100) {
      midiLogEl.removeChild(midiLogEl.lastChild);
    }
  }

  /* Groups MIDIAccess's live input/output ports into "devices" exactly as
   * Bandolibre's official docs/tools/midi.html configurator does. */
  function collectDevices() {
    if (!midiAccess) return [];
    const inputsByName = new Map();
    const outputsByName = new Map();

    for (const input of midiAccess.inputs.values()) {
      if (input.state !== "connected") continue;
      const name = input.name || "(unnamed)";
      if (!inputsByName.has(name)) inputsByName.set(name, []);
      inputsByName.get(name).push(input);
    }
    for (const output of midiAccess.outputs.values()) {
      if (output.state !== "connected") continue;
      const name = output.name || "(unnamed)";
      if (!outputsByName.has(name)) outputsByName.set(name, []);
      outputsByName.get(name).push(output);
    }

    const names = new Set([...inputsByName.keys(), ...outputsByName.keys()]);
    const devices = [];
    for (const name of names) {
      const ins = inputsByName.get(name) || [];
      const outs = outputsByName.get(name) || [];
      const count = Math.max(ins.length, outs.length);
      for (let i = 0; i < count; i++) {
        const input = ins[i] || null;
        const output = outs[i] || null;
        const key = (input && input.id) || (output && output.id);
        devices.push({ key, name, input, output, index: i, total: count });
      }
    }
    return devices;
  }

  function deviceLabel(device) {
    return device.total > 1 ? `${device.name} (${device.index + 1})` : device.name;
  }

  function renderSelector() {
    const devices = collectDevices();
    const previousValue = activeKey || "";

    select.innerHTML = "";
    const emptyOpt = document.createElement("option");
    emptyOpt.value = "";
    emptyOpt.textContent = devices.length ? "— select a MIDI device —" : "(no MIDI devices found)";
    select.appendChild(emptyOpt);

    for (const device of devices) {
      const opt = document.createElement("option");
      opt.value = device.key;
      opt.textContent = deviceLabel(device) + (isBandolibre(device.name) ? " — Bandolibre" : "");
      select.appendChild(opt);
    }
    select.value = previousValue;
    if (select.value !== previousValue) select.value = "";
  }

  function updateStatus() {
    if (!midiAccess) return;
    if (activeName) {
      setStatus("ok", `Connected: ${activeName}`);
    } else {
      const hasAnyDevice = collectDevices().length > 0;
      setStatus(hasAnyDevice ? "warn" : "", hasAnyDevice ? "Waiting for Bandolibre…" : "No MIDI devices found");
    }
  }

  function disconnect() {
    if (activeInput) activeInput.onmidimessage = null;
    activeKey = null;
    activeName = null;
    activeInput = null;
    activeOutput = null;
    sendAllNotesOff();
    renderSelector();
    updateStatus();
  }

  function connectTo(key) {
    const device = collectDevices().find((d) => d.key === key);
    if (!device) return;

    disconnect();
    activeKey = device.key;
    activeName = device.name;
    activeInput = device.input;
    activeOutput = device.output;
    if (activeInput) activeInput.onmidimessage = handleHardwareMidiMessage;

    logMidi(`Connected to "${device.name}"`);
    renderSelector();
    updateStatus();
  }

  /* Auto-connect rule from Bandolibre midi.html:
   * Only act when nothing is currently connected. Prefer a Bandolibre with
   * both input and output ready, or fall back to any Bandolibre input. */
  function maybeAutoConnect() {
    if (activeKey) return;
    const devices = collectDevices();
    const fullBando = devices.find((d) => isBandolibre(d.name) && d.input && d.output);
    const anyBando = fullBando || devices.find((d) => isBandolibre(d.name) && d.input);
    if (anyBando) connectTo(anyBando.key);
  }

  function reconcileDevices() {
    if (activeKey && !collectDevices().some((d) => d.key === activeKey)) {
      logMidi(`"${activeName}" disconnected`);
      disconnect();
    }
    renderSelector();
    maybeAutoConnect();
    updateStatus();
  }

  select.addEventListener("change", () => {
    if (!select.value) {
      disconnect();
    } else {
      connectTo(select.value);
    }
  });

  async function initWebMIDI() {
    if (!navigator.requestMIDIAccess) {
      setStatus("bad", "WebMIDI not supported in this browser");
      return;
    }
    try {
      // Request sysex: true just like Bandolibre configurator, with fallback to sysex: false
      try {
        midiAccess = await navigator.requestMIDIAccess({ sysex: true });
      } catch (_) {
        midiAccess = await navigator.requestMIDIAccess({ sysex: false });
      }
      midiAccess.onstatechange = (ev) => {
        const p = ev.port;
        logMidi(`${p.name || "(unnamed)"} [${p.type}] ${p.state}`);
        reconcileDevices();
      };
      reconcileDevices();
      // Periodic 1000ms reconciliation poll (matches Bandolibre midi.html workaround for Chrome/Edge USB-MIDI)
      setInterval(reconcileDevices, 1000);
    } catch (err) {
      setStatus("bad", "WebMIDI access denied");
      logMidi(`WebMIDI error: ${err.message}`);
    }
  }

  function handleHardwareMidiMessage(event) {
    const bytes = Array.from(event.data);
    bwBytesSinceTick += bytes.length;
    if (bytes.length === 0 || bytes[0] === 0xfe || bytes[0] === 0xf0) return;

    ensureAudioStarted();
    const status = bytes[0] & 0xf0;
    const chZero = bytes[0] & 0x0f;
    const channel = chZero + 1;
    const d1 = bytes[1] || 0;
    const d2 = bytes[2] || 0;

    sendMidiToWorklet(bytes[0], d1, d2);

    if (status === 0x90 && d2 > 0) {
      updateVelocityUI(d2);
      const effCh = effectiveChannelForHardwareMidi(channel, d1);
      highlightKeyUI(d1, true, d2, liveCcMsb, null, effCh);
      const handDesc = effCh === KBD_CHANNEL.left ? "Left Bajos • Bass Box" : "Right Cantabile • Treble Grill";
      const ovTag = isOverlapMidiNote(d1) ? " [Overlap C4–B4]" : "";
      logMidi(`Note On  | Ch ${channel} -> ${handDesc} | ${midiNoteLabel(d1)} (${d1})${ovTag} Vel ${d2}`);
    } else if (status === 0x80 || (status === 0x90 && d2 === 0)) {
      const effCh = effectiveChannelForHardwareMidi(channel, d1);
      highlightKeyUI(d1, false, 0, liveCcMsb, null, effCh);
    } else if (status === 0xb0) {
      if (d1 === 11) {
        liveCcMsb = d2;
        updateCC11UI(liveCcMsb, liveCcLsb, false);
      } else if (d1 === 43) {
        liveCcLsb = d2;
        updateCC11UI(liveCcMsb, liveCcLsb, false);
      }
    }
  }

  // =========================================================================
  // 2. WebAudio + WASM AudioWorklet Engine & Multi-Version Synthesis Selector
  // =========================================================================
  const SYNTH_VERSIONS = [
    {
      id: 0,
      slug: "v1",
      shortTitle: "v1: Lumped Clipper",
      label: "v1 Baseline — Lumped Odd Clipper + Shared Body EQ",
      train: 61.36,
      val: 59.75,
      perf: 87.01,
      harm: 55.66,
      tran: 60.41,
      dual: 53.72,
      pitch: 82.66,
      body: 66.23,
      cpu16: "139.0 us",
      physics:
        "Initial baseline: memoryless odd algebraic clipper u/sqrt(1+u²), single-pole low-pass filter, and a single shared 6-mode body resonator across both manuals (no ADAA anti-aliasing, no finite-baffle diffraction shelf).",
    },
    {
      id: 1,
      slug: "v2",
      shortTitle: "v2: ADAA + Baffle",
      label: "v2 Baseline — 1st-Order ADAA + 1-Pole Baffle + Coupled Celda",
      train: 65.06,
      val: 63.52,
      perf: 79.31,
      harm: 63.26,
      tran: 61.99,
      dual: 56.11,
      pitch: 82.66,
      body: 68.18,
      cpu16: "232.0 us",
      physics:
        "Second-generation baseline: 1st-order Antiderivative Anti-Aliasing (ADAA) orifice solver, 1-pole finite wooden box baffle high-pass shelf, coupled 8'+4' celda impedance drop, and dual-manual Left/Right wooden box modal banks.",
    },
    {
      id: 2,
      slug: "v3_aero",
      shortTitle: "v3-Aero: dU/dt Jet",
      label: "v3-Aero (Dir 1) — Quadrature Monopole dU/dt Radiation & 2-Pole Dipole Baffle",
      train: 65.11,
      val: 63.70,
      perf: 77.78,
      harm: 63.39,
      tran: 62.66,
      dual: 56.08,
      pitch: 82.66,
      body: 68.25,
      cpu16: "252.6 us",
      physics:
        "v3 Direction 1 (Ricot et al. 2005; Puranik & Scavone 2022): Adds quadrature band-limited monopole volume-acceleration radiation (dU/dt = lp_state1 - lp_state2, 90° orthogonal to volume flow) and 2nd-order sub-baffle dipole diffraction.",
    },
    {
      id: 3,
      slug: "v3_beam",
      shortTitle: "v3-Beam: Clamped-Bar",
      label: "v3-Beam (Dir 2) — Poiseuille-Bernoulli Duty Softening & Clamped-Bar Modes",
      train: 65.08,
      val: 63.57,
      perf: 76.29,
      harm: 63.31,
      tran: 62.17,
      dual: 56.15,
      pitch: 82.66,
      body: 68.20,
      cpu16: "272.7 us",
      physics:
        "v3 Direction 2 (Millot & Baumann 2007; Puranik & Scavone 2023): Models zero-sum barycentric Poiseuille-Bernoulli boundary-layer softening, pressure-driven H3 duty-cycle sharpening, and Euler-Bernoulli 2nd flexural clamped-bar modes (~6f0).",
    },
    {
      id: 4,
      slug: "v3_transient",
      shortTitle: "v3-Transient: Hopf Lag",
      label: "v3-Transient (Dir 3) — Unclamped Lastre Inertia, Hopf Lag & Staccato Chiff",
      train: 65.10,
      val: 63.58,
      perf: 79.16,
      harm: 63.34,
      tran: 61.99,
      dual: 56.25,
      pitch: 82.66,
      body: 68.24,
      cpu16: "234.2 us",
      physics:
        "v3 Direction 3 (Ramos et al. 2023; Cottingham 2001-2013): Models low-bass tip-weight (lastre) inertia on C0..A#0, complementary 8'/4' Hopf onset lag, velocity-coupled 4' octave bite, and post-click staccato vortex detachment chiff.",
    },
    {
      id: 5,
      slug: "v3",
      shortTitle: "v3: Unified",
      label: "v3 Unified — Monopole dU/dt + Clamped-Bar + Hopf Entrainment",
      train: 65.24,
      val: 63.84,
      perf: 73.26,
      harm: 63.55,
      tran: 62.71,
      dual: 56.30,
      pitch: 82.66,
      body: 68.33,
      cpu16: "312.3 us",
      physics:
        "Third-generation unified physical model (v3 Directions 1 + 2 + 3 co-calibrated): Combines quadrature monopole dU/dt jet radiation, 2nd-order sub-baffle dipole diffraction, zero-sum barycentric clamped-bar modes, and Hopf onset entrainment.",
    },
    {
      id: 6,
      slug: "v4_cabinet",
      shortTitle: "v4-Cabinet: 10-Mode IR",
      label: "v4-Cabinet (Dir 1) — Deconvolved 10-Mode Wooden Box & Early-Reflection FIR",
      train: 65.25,
      val: 63.85,
      perf: 73.28,
      harm: 63.57,
      tran: 62.71,
      dual: 56.30,
      pitch: 82.66,
      body: 68.35,
      cpu16: "312.0 us",
      physics:
        "v4 Direction 1 (Deconvolved Cabinet Modal Bank + Early-Reflection FIR): Expands each manual's wooden cabezal enclosure from 6 to 10 biquad modes (185/980/2050/6400 Hz Left Bajos; 370/1120/2020/3450 Hz Right Cantabile) plus a 32-sample internal boundary reflection comb.",
    },
    {
      id: 7,
      slug: "v4_slot",
      shortTitle: "v4-Slot: 3-Regime ADAA",
      label: "v4-Slot (Dir 2) — 3-Regime Plate-Channel Slot Transit & Vowel Swell ADAA",
      train: 65.25,
      val: 63.85,
      perf: 73.25,
      harm: 63.57,
      tran: 62.71,
      dual: 56.30,
      pitch: 82.66,
      body: 68.34,
      cpu16: "312.5 us",
      physics:
        "v4 Direction 2 (3-Regime Plate-Channel Slot Transit): Models finite zinc/aluminum reed-plate channel thickness h_plate via channel-notch H3 shaping, orifice-pinch H5 ADAA, dynamic vowel-swell clamped-bar modulation, and low-bass 4' sub-baffle H2 boom taming.",
    },
    {
      id: 8,
      slug: "v4_coupled",
      shortTitle: "v4-Coupled: 2nd-Order ODE",
      label: "v4-Coupled (Dir 3) — 2nd-Order Coupled Reed ODE, Startup Chirp & Celda Feedback",
      train: 65.24,
      val: 63.84,
      perf: 73.51,
      harm: 63.57,
      tran: 62.71,
      dual: 56.30,
      pitch: 82.66,
      body: 68.35,
      cpu16: "309.0 us",
      physics:
        "v4 Direction 3 (2nd-Order Coupled Reed ODE & Startup Chirp): Adds 2nd-order mechanical amplitude envelope momentum during onset, hydrodynamic added-mass startup pitch chirp, amplitude-governed Hopf onset lead, and audio-rate 8'->4' shared celda cross-modulation.",
    },
    {
      id: 9,
      slug: "v4_celda",
      shortTitle: "v4-Celda: Cavity SVF",
      label: "v4-Celda (Dir 4) — Pitch-Scaled Individual Celda Cavity Resonator (Okada 2025)",
      train: 65.24,
      val: 63.84,
      perf: 73.50,
      harm: 63.56,
      tran: 62.71,
      dual: 56.30,
      pitch: 82.66,
      body: 68.34,
      cpu16: "309.1 us",
      physics:
        "v4 Direction 4 (Okada 2025 Individual Celda Cavity Resonator): Embeds a per-voice Cytomic trapezoidal 2-pole SVF bandpass resonator whose Helmholtz/quarter-wave frequency scales smoothly with manual pitch u in [0, 1].",
    },
    {
      id: 10,
      slug: "v4_zapatilla",
      shortTitle: "v4-Zapatilla: Valve Muffle",
      label: "v4-Zapatilla (Dir 5) — Leather Pallet Thump & Closed-Valve Muffled Ring-Down",
      train: 65.24,
      val: 63.84,
      perf: 73.47,
      harm: 63.55,
      tran: 62.71,
      dual: 56.30,
      pitch: 82.66,
      body: 68.35,
      cpu16: "309.6 us",
      physics:
        "v4 Direction 5 (Leather Zapatilla Valve Physics): Models pitch-scaled pallet click transmission across wooden soundboard ribs, leather pallet seating thump and resting-gap Bernoulli chiff, and closed-valve acoustic occlusion low-pass muffling during onset/release.",
    },
    {
      id: 11,
      slug: "v4",
      shortTitle: "v4: Unified SOTA",
      label: "v4 Unified SOTA — Cabinet IR + 3-Regime Slot + Coupled ODE + Celda + Zapatilla",
      train: 65.27,
      val: 63.87,
      perf: 72.60,
      harm: 63.61,
      tran: 62.72,
      dual: 56.30,
      pitch: 82.66,
      body: 68.39,
      cpu16: "320.9 us",
      physics:
        "Unified SOTA v4 Physical Model: Co-calibrates all 5 v4 physical modeling advances (10-mode deconvolved cabinet + 3-regime slot transit + coupled ODE startup chirp + pitch-scaled celda SVF + zapatilla valve occlusion) with branchless minimax fast_sin_cos and single-division ADAA.",
    },
    {
      id: 12,
      slug: "v5",
      shortTitle: "v5: Vivo Expressive ★",
      label: "v5 Vivo (Default) — Staged 8'->4' Attack + Sustain Pulsation + Low-Mid Body & Treble Clarity",
      train: 65.36,
      val: 64.14,
      perf: 71.67,
      harm: 63.61,
      tran: 62.91,
      dual: 56.79,
      pitch: 82.66,
      body: 68.56,
      cpu16: "333.0 us",
      physics:
        "v5 Vivo Expressive Model: Staged 8' grave-first attack followed by distinct 4' octave secondary attack and symbiotic blend, organic held-note sustain pulsation (celda beating + physiological fueye respiration), warm 160-450 Hz wooden body fullness, and crystalline 2-6 kHz treble clarity.",
    },
  ];

  let audioCtx = null;
  let workletNode = null;
  let analyserNode = null;
  let analyserL = null;
  let analyserR = null;
  let audioInitPromise = null;
  let soundReady = false;
  let activeVersionId = 12; // Default to v5 Vivo Expressive
  let currentStereoWidth = 0.84;
  let currentSplitNote = 60; // C4

  const startBtn = document.getElementById("btn-start");
  const demoBtn = document.getElementById("btn-demo");
  const overlapDemoBtn = document.getElementById("btn-overlap-demo");
  const abCycleBtn = document.getElementById("btn-ab-cycle");
  const toneBtn = document.getElementById("btn-tone");
  const panicBtn = document.getElementById("btn-panic");
  const audioStatusBadge = document.getElementById("audioStatusBadge");
  const cc11Slider = document.getElementById("cc11Slider");
  const cc11Readout = document.getElementById("cc11Readout");
  const cc11Bar = document.getElementById("cc11Bar");
  const velReadout = document.getElementById("velReadout");
  const velBar = document.getElementById("velBar");
  const stereoWidthSlider = document.getElementById("stereoWidthSlider");
  const stereoWidthReadout = document.getElementById("stereoWidthReadout");
  const leftMeterBar = document.getElementById("leftMeterBar");
  const leftMeterText = document.getElementById("leftMeterText");
  const rightMeterBar = document.getElementById("rightMeterBar");
  const rightMeterText = document.getElementById("rightMeterText");
  const invertYToggle = document.getElementById("invertYToggle");
  const sustainHoldToggle = document.getElementById("sustainHoldToggle");
  const manualSelect = document.getElementById("manualSelect");
  const splitPointGroup = document.getElementById("splitPointGroup");
  const splitNoteSelect = document.getElementById("splitNoteSelect");
  const pianoStack = document.getElementById("pianoStack");
  const canvas = document.getElementById("scope-canvas");
  const canvasCtx = canvas.getContext("2d");
  const versionSelect = document.getElementById("versionSelect");
  const versionPills = document.getElementById("versionPills");
  const versionDescBox = document.getElementById("versionDescBox");
  const versionTableBody = document.getElementById("versionTableBody");

  // Studio Post-Processing Controls (Spatializer + Optical Bus Compressor + Salon/Hall Reverb)
  const postEnableToggle = document.getElementById("postEnableToggle");
  const postStatusBadge = document.getElementById("postStatusBadge");
  const postControlsGrid = document.getElementById("postControlsGrid");
  const postSpatialSlider = document.getElementById("postSpatialSlider");
  const postSpatialReadout = document.getElementById("postSpatialReadout");
  const postCompSlider = document.getElementById("postCompSlider");
  const postCompReadout = document.getElementById("postCompReadout");
  const postGrBadge = document.getElementById("postGrBadge");
  const postRevMixSlider = document.getElementById("postRevMixSlider");
  const postRevMixReadout = document.getElementById("postRevMixReadout");
  const postRevSizeSlider = document.getElementById("postRevSizeSlider");
  const postRevSizeReadout = document.getElementById("postRevSizeReadout");

  const POST_PRESETS = {
    milonga: { label: "SALON MILONGA", spatial: 55, comp: 45, revMix: 26, revSize: 50 },
    concert: { label: "TEATRO COLÓN (HALL)", spatial: 72, comp: 38, revMix: 40, revSize: 78 },
    studio: { label: "STUDIO CLOSE-MIC", spatial: 35, comp: 65, revMix: 14, revSize: 32 },
    binaural: { label: "BINAURAL HEADPHONES", spatial: 85, comp: 42, revMix: 22, revSize: 44 },
  };

  let postEnabled = true;
  let activePostPresetLabel = "SALON MILONGA";
  let postParams = {
    compAmount: 0.45,
    reverbMix: 0.26,
    reverbSize: 0.50,
    spatialAmount: 0.55,
  };

  function syncPostProcessingToWorklet() {
    if (postSpatialReadout) postSpatialReadout.textContent = `${Math.round(postParams.spatialAmount * 100)}%`;
    if (postCompReadout) postCompReadout.textContent = `${Math.round(postParams.compAmount * 100)}%`;
    if (postRevMixReadout) postRevMixReadout.textContent = `${Math.round(postParams.reverbMix * 100)}%`;
    if (postRevSizeReadout) postRevSizeReadout.textContent = `${Math.round(postParams.reverbSize * 100)}%`;
    if (postControlsGrid) {
      postControlsGrid.style.opacity = postEnabled ? "1.0" : "0.45";
    }
    if (postStatusBadge) {
      if (postEnabled) {
        postStatusBadge.textContent = `ON • ${activePostPresetLabel}`;
        postStatusBadge.className = "ch-badge ch1";
      } else {
        postStatusBadge.textContent = "BYPASSED (DRY)";
        postStatusBadge.className = "ch-badge";
      }
    }
    if (!postEnabled && postGrBadge) {
      postGrBadge.textContent = "GR: 0.0 dB";
    }
    if (workletNode) {
      workletNode.port.postMessage({ type: "set-post-enabled", enabled: postEnabled });
      workletNode.port.postMessage({
        type: "set-post-params",
        compAmount: postParams.compAmount,
        reverbMix: postParams.reverbMix,
        reverbSize: postParams.reverbSize,
        spatialAmount: postParams.spatialAmount,
      });
    }
  }

  if (postEnableToggle) {
    postEnableToggle.addEventListener("change", (e) => {
      ensureAudioStarted();
      postEnabled = !!e.target.checked;
      syncPostProcessingToWorklet();
      logMidi(`Studio Post-Processing -> ${postEnabled ? "ENABLED (" + activePostPresetLabel + ")" : "BYPASSED (Dry)"}`);
    });
  }

  function handleCustomPostSlider() {
    ensureAudioStarted();
    activePostPresetLabel = "CUSTOM";
    if (postSpatialSlider) postParams.spatialAmount = Number(postSpatialSlider.value) / 100;
    if (postCompSlider) postParams.compAmount = Number(postCompSlider.value) / 100;
    if (postRevMixSlider) postParams.reverbMix = Number(postRevMixSlider.value) / 100;
    if (postRevSizeSlider) postParams.reverbSize = Number(postRevSizeSlider.value) / 100;
    syncPostProcessingToWorklet();
  }

  [postSpatialSlider, postCompSlider, postRevMixSlider, postRevSizeSlider].forEach((sl) => {
    if (sl) sl.addEventListener("input", handleCustomPostSlider);
  });

  document.querySelectorAll(".post-preset-btn").forEach((btn) => {
    btn.addEventListener("click", () => {
      ensureAudioStarted();
      const key = btn.dataset.preset;
      const p = POST_PRESETS[key];
      if (!p) return;
      postEnabled = true;
      if (postEnableToggle) postEnableToggle.checked = true;
      activePostPresetLabel = p.label;
      postParams.spatialAmount = p.spatial / 100;
      postParams.compAmount = p.comp / 100;
      postParams.reverbMix = p.revMix / 100;
      postParams.reverbSize = p.revSize / 100;
      if (postSpatialSlider) postSpatialSlider.value = String(p.spatial);
      if (postCompSlider) postCompSlider.value = String(p.comp);
      if (postRevMixSlider) postRevMixSlider.value = String(p.revMix);
      if (postRevSizeSlider) postRevSizeSlider.value = String(p.revSize);
      document.querySelectorAll(".post-preset-btn").forEach((b) => {
        b.style.borderColor = b === btn ? "#2f7d78" : "var(--border)";
      });
      syncPostProcessingToWorklet();
      logMidi(
        `Post-FX Preset -> ${p.label} (Spatial ${p.spatial}%, Comp ${p.comp}%, Reverb ${p.revMix}%, Room ${p.revSize}%)`
      );
    });
  });

  function renderVersionUI() {
    if (!versionSelect || !versionPills || !versionDescBox || !versionTableBody) return;

    // 1. Dropdown
    versionSelect.innerHTML = "";
    for (const v of SYNTH_VERSIONS) {
      const opt = document.createElement("option");
      opt.value = String(v.id);
      opt.textContent = `${v.label} (Val: ${v.val.toFixed(2)} | Perf: ${v.perf.toFixed(1)})`;
      versionSelect.appendChild(opt);
    }
    versionSelect.value = String(activeVersionId);

    // 2. Quick-select pills (ordered v5 Vivo first, then v4 SOTA & ablations, then v3/v2/v1)
    const displayOrder = [12, 11, 6, 7, 8, 9, 10, 5, 2, 3, 4, 1, 0];
    versionPills.innerHTML = "";
    for (const vid of displayOrder) {
      const v = SYNTH_VERSIONS.find((x) => x.id === vid);
      if (!v) continue;
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "ver-btn" + (v.id === activeVersionId ? " active" : "");
      btn.innerHTML = `
        <div class="ver-btn-top">
          <span>${v.shortTitle}</span>
          <span class="ver-score-pill">Val ${v.val.toFixed(2)}</span>
        </div>
        <div class="ver-btn-sub">Train ${v.train.toFixed(2)} &bull; Perf ${v.perf.toFixed(1)} &bull; ${v.cpu16}</div>
      `;
      btn.addEventListener("click", () => setSynthVersion(v.id));
      versionPills.appendChild(btn);
    }

    // 3. Active description box
    const cur = SYNTH_VERSIONS.find((x) => x.id === activeVersionId) || SYNTH_VERSIONS[12];
    const deltaVal = cur.val - 63.52;
    const deltaSign = deltaVal >= 0 ? `+${deltaVal.toFixed(2)}` : deltaVal.toFixed(2);
    versionDescBox.innerHTML = `
      <div><strong>Active Engine — ${cur.label}</strong> &nbsp;|&nbsp; Train: <strong>${cur.train.toFixed(2)}</strong> &bull; Held-Out Val: <strong>${cur.val.toFixed(2)}</strong> (${deltaSign} vs v2) &bull; Perf Score: <strong>${cur.perf.toFixed(2)}/100</strong> &bull; 16-Voice CPU: <strong>${cur.cpu16}</strong></div>
      <div style="margin-top:4px; color:var(--muted);">${cur.physics}</div>
    `;

    // 4. Full 13-version comparison table
    versionTableBody.innerHTML = "";
    for (const v of SYNTH_VERSIONS) {
      const tr = document.createElement("tr");
      if (v.id === activeVersionId) tr.className = "active-row";
      const dVal = v.val - 63.52;
      let dClass = "delta-base";
      let dText = "baseline";
      if (v.id !== 1) {
        dClass = dVal >= 0 ? "delta-pos" : "delta-neg";
        dText = (dVal >= 0 ? "+" : "") + dVal.toFixed(2);
      }
      tr.innerHTML = `
        <td>${v.id === activeVersionId ? "▶ " : ""}${v.slug}</td>
        <td style="font-family:system-ui, sans-serif;">${v.label}</td>
        <td>${v.train.toFixed(2)}</td>
        <td>${v.val.toFixed(2)}</td>
        <td class="${dClass}">${dText}</td>
        <td>${v.perf.toFixed(2)}</td>
        <td>${v.harm.toFixed(2)}</td>
        <td>${v.tran.toFixed(2)}</td>
        <td>${v.dual.toFixed(2)}</td>
        <td>${v.pitch.toFixed(2)}</td>
        <td>${v.body.toFixed(2)}</td>
        <td>${v.cpu16}</td>
      `;
      tr.addEventListener("click", () => setSynthVersion(v.id));
      versionTableBody.appendChild(tr);
    }
  }

  function setSynthVersion(versionId) {
    const v = SYNTH_VERSIONS.find((x) => x.id === Number(versionId));
    if (!v) return;
    activeVersionId = v.id;
    if (workletNode) {
      workletNode.port.postMessage({ type: "set-version", versionId: v.id });
    }
    renderVersionUI();
    const verTag = v.slug.toUpperCase();
    if (soundReady && audioCtx) {
      audioStatusBadge.textContent = `Audio: ${audioCtx.sampleRate} Hz WASM [${verTag}] (${audioCtx.state})`;
    }
    logMidi(`Switched Synthesis Engine -> ${v.label} [Train ${v.train.toFixed(2)} / Val ${v.val.toFixed(2)} / Perf ${v.perf.toFixed(2)}]`);
  }

  if (versionSelect) {
    versionSelect.addEventListener("change", (e) => {
      ensureAudioStarted();
      setSynthVersion(Number(e.target.value));
    });
  }

  renderVersionUI();

  async function ensureAudioStarted() {
    unlockAudioSynchronously();
    if (workletNode) return;
    if (audioInitPromise) return audioInitPromise;

    audioInitPromise = (async () => {
      try {
        audioStatusBadge.textContent = "Loading WASM…";

        const cacheBust = `?v=${Date.now()}`;
        await audioCtx.audioWorklet.addModule(`worklet-processor.js${cacheBust}`);
        const wasmResp = await fetch(`synth_wasm.wasm${cacheBust}`, { cache: "no-store" });
        const wasmBytes = await wasmResp.arrayBuffer();

        const node = new AudioWorkletNode(audioCtx, "bandolibre-processor", {
          numberOfInputs: 0,
          numberOfOutputs: 1,
          outputChannelCount: [2],
        });

        analyserNode = audioCtx.createAnalyser();
        analyserNode.fftSize = 2048;
        analyserL = audioCtx.createAnalyser();
        analyserL.fftSize = 1024;
        analyserR = audioCtx.createAnalyser();
        analyserR.fftSize = 1024;
        const splitter = audioCtx.createChannelSplitter(2);
        // Connect worklet directly to destination AND in parallel to analyserNode + L/R splitter
        node.connect(audioCtx.destination);
        node.connect(analyserNode);
        node.connect(splitter);
        splitter.connect(analyserL, 0);
        splitter.connect(analyserR, 1);

        node.port.onmessage = (e) => {
          if (e.data.type === "ready") {
            soundReady = true;
            const cur = SYNTH_VERSIONS.find((x) => x.id === activeVersionId) || SYNTH_VERSIONS[12];
            audioStatusBadge.textContent = `Audio: ${e.data.sampleRate} Hz WASM [${cur.slug.toUpperCase()}] (${audioCtx.state})`;
            startBtn.textContent = "Sound Active";
            startBtn.disabled = true;
            sendMidiToWorklet(0xb0, 11, liveCcMsb);
          } else if (e.data.type === "version-changed") {
            const cur = SYNTH_VERSIONS.find((x) => x.id === e.data.activeVersionId) || SYNTH_VERSIONS[12];
            audioStatusBadge.textContent = `Audio: ${audioCtx.sampleRate} Hz WASM [${cur.slug.toUpperCase()}] (${audioCtx.state})`;
          } else if (e.data.type === "post-telemetry") {
            if (postGrBadge) {
              const gr = postEnabled ? Math.max(0, Number(e.data.grDb) || 0) : 0;
              postGrBadge.textContent = gr > 0.05 ? `GR: -${gr.toFixed(1)} dB` : "GR: 0.0 dB";
            }
          } else if (e.data.type === "error") {
            audioStatusBadge.textContent = `WASM Error`;
            logMidi(`AudioWorklet Error: ${e.data.message}`);
          }
        };

        const isPianistSplit = manualSelect && manualSelect.value === "pianist_split";
        node.port.postMessage({
          type: "init-wasm",
          wasmBytes,
          tableMode: false,
          versionId: activeVersionId,
          keyboardMode: isPianistSplit ? 1 : 0,
          splitNote: currentSplitNote,
          stereoWidth: currentStereoWidth,
          postEnabled,
          postParams,
        });

        workletNode = node;
        requestAnimationFrame(drawScopeAndSpectrum);
      } catch (err) {
        audioInitPromise = null;
        audioStatusBadge.textContent = "Audio Init Failed";
        logMidi(`Audio init error: ${err.message || err}`);
      }
    })();

    return audioInitPromise;
  }

  // Unlock and resume AudioContext synchronously on any pointer gesture
  document.addEventListener("pointerdown", () => {
    unlockAudioSynchronously();
  });

  function sendMidiToWorklet(status, data1, data2) {
    if (workletNode) {
      workletNode.port.postMessage({ type: "midi", status, data1, data2 });
    }
  }

  function updateCC11UI(msb, lsb = 0, sendToSynth = true) {
    liveCcMsb = Math.max(0, Math.min(127, Math.round(msb)));
    liveCcLsb = Math.max(0, Math.min(127, Math.round(lsb)));
    const pct = ((liveCcMsb / 127) * 100).toFixed(0);
    cc11Slider.value = liveCcMsb;
    cc11Readout.textContent = `CC#11: ${liveCcMsb} (${pct}%)`;
    cc11Bar.style.width = `${pct}%`;

    if (sendToSynth) {
      sendMidiToWorklet(0xb0, 11, liveCcMsb);
      if (lsb > 0) sendMidiToWorklet(0xb0, 43, liveCcLsb);
    }

    // Update live fill height on all currently active keys
    document.querySelectorAll(".piano-key.active").forEach((keyEl) => {
      const fillEl = keyEl.querySelector(".key-cc-fill");
      const badgeEl = keyEl.querySelector(".key-top-badge");
      if (fillEl) fillEl.style.height = `${pct}%`;
      if (badgeEl) {
        const vel = keyEl.dataset.lastVel || "—";
        badgeEl.textContent = `V:${vel} CC:${liveCcMsb}`;
      }
    });
  }

  function updateVelocityUI(vel) {
    const v = Math.max(1, Math.min(127, Math.round(vel)));
    const pct = ((v / 127) * 100).toFixed(0);
    velReadout.textContent = `Vel: ${v} (${pct}%)`;
    velBar.style.width = `${pct}%`;
  }

  cc11Slider.addEventListener("input", (e) => {
    ensureAudioStarted();
    updateCC11UI(Number(e.target.value), 0, true);
  });

  startBtn.addEventListener("click", () => {
    unlockAudioSynchronously();
    ensureAudioStarted();
  });

  /**
   * Synchronous WebAudio unlock inside the direct user click/pointerdown turn.
   * Safari and strict autoplay policies suspend AudioContext if `new AudioContext()`
   * or `audioCtx.resume()` happens after an `await` boundary.
   */
  function unlockAudioSynchronously() {
    if (!audioCtx) {
      audioCtx = new (window.AudioContext || window.webkitAudioContext)({
        latencyHint: "interactive",
      });
      audioCtx.onstatechange = () => {
        if (soundReady) {
          const cur = SYNTH_VERSIONS.find((x) => x.id === activeVersionId) || SYNTH_VERSIONS[12];
          audioStatusBadge.textContent = `Audio: ${audioCtx.sampleRate} Hz WASM [${cur.slug.toUpperCase()}] (${audioCtx.state})`;
        }
      };
    }
    if (audioCtx.state !== "running") {
      audioCtx.resume().catch(() => {});
    }
  }

  toneBtn.addEventListener("click", () => {
    unlockAudioSynchronously();
    const osc = audioCtx.createOscillator();
    const gain = audioCtx.createGain();
    osc.type = "sine";
    osc.frequency.value = 440;
    gain.gain.setValueAtTime(0.3, audioCtx.currentTime);
    gain.gain.exponentialRampToValueAtTime(0.001, audioCtx.currentTime + 0.35);
    osc.connect(gain);
    gain.connect(audioCtx.destination);
    if (analyserNode) gain.connect(analyserNode);
    osc.start();
    osc.stop(audioCtx.currentTime + 0.35);
    ensureAudioStarted();
    setTimeout(() => {
      logMidi(
        `Speaker Beep (440 Hz) | state=${audioCtx.state}, currentTime=${audioCtx.currentTime.toFixed(2)}s, sampleRate=${audioCtx.sampleRate} Hz, sinkId="${audioCtx.sinkId || "default"}"`
      );
    }, 60);
  });

  function sendAllNotesOff() {
    sendMidiToWorklet(0xb0, 123, 0);
    sendMidiToWorklet(0xb1, 123, 0);
    latchedNotes.clear();
    document.querySelectorAll(".piano-key.active").forEach((el) => {
      el.classList.remove("active");
      const fill = el.querySelector(".key-cc-fill");
      if (fill) fill.style.height = "0%";
      const badge = el.querySelector(".key-top-badge");
      if (badge) badge.textContent = el.dataset.noteLabel;
    });
  }

  panicBtn.addEventListener("click", () => {
    if (demoInterval) {
      clearInterval(demoInterval);
      demoInterval = null;
    }
    sendAllNotesOff();
    logMidi("All Notes Off");
  });

  // =========================================================================
  // 3. Interactive Dual-Channel Bandolibre & Pianist Split Keyboards:
  //    - Default ("dual"): Two keyboards, one per MIDI channel
  //        * Ch 1 Left Keyboard (Bajos, C2..B4, MIDI 36..71, Left Pan, Deep Bass Box)
  //        * Ch 2 Right Keyboard (Cantabile, C4..C7, MIDI 60..96, Right Pan, Treble Grill)
  //        * Shared Overlap Octave C4..B4 (MIDI 60..71) is highlighted on BOTH keyboards!
  //    - "pianist_split": Single unified keyboard (C2..C7) split at `currentSplitNote`
  //    - Vertical Position of Click -> Note-On Velocity (1..127)
  //    - Moving Mouse Up & Down While Pressed -> Continuous CC#11 Variation
  // =========================================================================
  const NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
  const BLACK_SEMITONES = new Set([1, 3, 6, 8, 10]);
  const OVERLAP_MIN_MIDI = 60; // C4
  const OVERLAP_MAX_MIDI = 71; // B4

  function midiNoteLabel(midi) {
    const name = NOTE_NAMES[midi % 12];
    const oct = Math.floor(midi / 12) - 1;
    return `${name}${oct}`;
  }

  function isOverlapMidiNote(midi) {
    return midi >= OVERLAP_MIN_MIDI && midi <= OVERLAP_MAX_MIDI;
  }

  function effectiveChannelForHardwareMidi(rawChannel, midi) {
    if (manualSelect && manualSelect.value === "pianist_split") {
      return midi < currentSplitNote ? KBD_CHANNEL.left : KBD_CHANNEL.right;
    }
    if (rawChannel === KBD_CHANNEL.left || rawChannel === KBD_CHANNEL.right) {
      return rawChannel;
    }
    return midi < currentSplitNote ? KBD_CHANNEL.left : KBD_CHANNEL.right;
  }

  function updateStereoWidthUI(widthNorm, sendToSynth = true) {
    currentStereoWidth = Math.max(0.0, Math.min(1.0, Number(widthNorm)));
    const pct = Math.round(currentStereoWidth * 100);
    const mainPanPct = Math.round((0.5 + 0.5 * currentStereoWidth) * 100);
    const crossPanPct = 100 - mainPanPct;
    if (stereoWidthSlider) stereoWidthSlider.value = String(pct);
    if (stereoWidthReadout) {
      stereoWidthReadout.textContent = `${pct}% (${mainPanPct}% Main / ${crossPanPct}% Cross)`;
    }
    if (sendToSynth && workletNode) {
      workletNode.port.postMessage({
        type: "set-stereo-width",
        stereoWidth: currentStereoWidth,
      });
    }
  }

  if (stereoWidthSlider) {
    stereoWidthSlider.addEventListener("input", (e) => {
      ensureAudioStarted();
      updateStereoWidthUI(Number(e.target.value) / 100.0, true);
    });
  }

  function syncKeyboardModeToWorklet() {
    const mode = manualSelect ? manualSelect.value : "dual";
    const isPianistSplit = mode === "pianist_split";
    if (splitPointGroup) {
      splitPointGroup.style.display = isPianistSplit ? "inline-flex" : "none";
    }
    if (workletNode) {
      workletNode.port.postMessage({
        type: "set-keyboard-mode",
        modeId: isPianistSplit ? 1 : 0,
      });
      workletNode.port.postMessage({
        type: "set-split-note",
        splitNote: currentSplitNote,
      });
    }
  }

  /**
   * Maps a pointer event's vertical Y coordinate within `keyEl` to `[0..1]`.
   * By default:
   * - Clicking/dragging toward the BOTTOM of the key = stronger velocity / higher CC#11 (up to 127),
   *   and dragging UP / DOWN smoothly sweeps CC#11 between 0 and 127!
   * - If `invertYToggle` is checked, TOP of the key = 127 and BOTTOM = 1.
   */
  function yRatioFromKeyEvent(keyEl, clientY) {
    const rect = keyEl.getBoundingClientRect();
    const rawRatio = (clientY - rect.top) / Math.max(1, rect.height);
    const clamped = Math.max(0, Math.min(1, rawRatio));
    return invertYToggle.checked ? 1.0 - clamped : clamped;
  }

  function highlightKeyUI(midi, isActive, velocity, cc11, clickRatioY = null, channel = null) {
    if (!pianoStack) return;
    const selector =
      channel !== null && channel !== undefined
        ? `.piano-key[data-midi="${midi}"][data-channel="${channel}"]`
        : `.piano-key[data-midi="${midi}"]`;
    const keyEls = pianoStack.querySelectorAll(selector);
    keyEls.forEach((keyEl) => {
      const fillEl = keyEl.querySelector(".key-cc-fill");
      const markerEl = keyEl.querySelector(".key-vel-marker");
      const badgeEl = keyEl.querySelector(".key-top-badge");

      if (isActive) {
        keyEl.classList.add("active");
        keyEl.dataset.lastVel = velocity;
        const pct = ((cc11 / 127) * 100).toFixed(0);
        if (fillEl) fillEl.style.height = `${pct}%`;
        if (markerEl && clickRatioY !== null) {
          markerEl.style.top = `${(clickRatioY * 100).toFixed(1)}%`;
        }
        if (badgeEl) badgeEl.textContent = `V:${velocity} CC:${cc11}`;
      } else {
        keyEl.classList.remove("active");
        if (fillEl) fillEl.style.height = "0%";
        if (badgeEl) badgeEl.textContent = keyEl.dataset.noteLabel;
      }
    });
  }

  // Active pointer tracking: pointerId -> { midi, channel, keyEl, statusOff, cancelled }
  const activePointers = new Map();
  // Latched notes when "Latch / Hold Notes" is enabled: `${channel}:${midi}` -> { midi, channel }
  const latchedNotes = new Map();

  function startNoteFromPointer(keyEl, clientY) {
    const midi = Number(keyEl.dataset.midi);
    const channel = Number(keyEl.dataset.channel) || KBD_CHANNEL.left;
    const statusOn = 0x90 | (channel - 1);
    const statusOff = 0x80 | (channel - 1);
    const latchKey = `${channel}:${midi}`;

    // Toggle off if Latch mode is enabled and this (channel, midi) key is already latched
    if (sustainHoldToggle.checked && latchedNotes.has(latchKey)) {
      const old = latchedNotes.get(latchKey);
      sendMidiToWorklet(0x80 | (old.channel - 1), old.midi, 64);
      latchedNotes.delete(latchKey);
      highlightKeyUI(midi, false, 0, liveCcMsb, null, channel);
      logMidi(`Latch Off | Ch ${channel} Note ${midiNoteLabel(midi)} (${midi})`);
      return null;
    }

    const ratio = yRatioFromKeyEvent(keyEl, clientY);
    const velocity = Math.max(1, Math.min(127, Math.round(1 + ratio * 126)));
    const initialCc11 = Math.max(8, Math.min(127, Math.round(ratio * 127)));

    updateVelocityUI(velocity);
    updateCC11UI(initialCc11, 0, true);
    sendMidiToWorklet(statusOn, midi, velocity);

    const visualY = invertYToggle.checked ? 1.0 - ratio : ratio;
    highlightKeyUI(midi, true, velocity, initialCc11, visualY, channel);

    if (sustainHoldToggle.checked) {
      latchedNotes.set(latchKey, { midi, channel });
    }

    const handDesc =
      channel === KBD_CHANNEL.left
        ? "Ch 1 • Left Bajos (Bass Box, Left Pan)"
        : "Ch 2 • Right Cantabile (Treble Grill, Right Pan)";
    const overlapTag = isOverlapMidiNote(midi) ? " [Shared Overlap C4–B4]" : "";
    logMidi(
      `Key Press | ${midiNoteLabel(midi)}${overlapTag} -> ${handDesc} | Vel=${velocity}, CC#11=${initialCc11}`
    );

    return { midi, channel, keyEl, statusOff, cancelled: false };
  }

  function createPianoLaneDOM({
    laneClass,
    badgeClass,
    badgeText,
    titleText,
    metaText,
    overlapNoteText,
    startMidi,
    endMidi,
    channelResolver,
    compact = false,
    splitMarkerMidi = null,
  }) {
    const laneEl = document.createElement("div");
    laneEl.className = `manual-lane ${laneClass}`;

    const headerEl = document.createElement("div");
    headerEl.className = "manual-lane-header";
    headerEl.innerHTML = `
      <div class="manual-lane-title">
        <span class="ch-badge ${badgeClass}">${badgeText}</span>
        <span>${titleText}</span>
        ${overlapNoteText ? `<span class="ch-badge overlap">${overlapNoteText}</span>` : ""}
      </div>
      <div class="manual-lane-meta">${metaText}</div>
    `;
    laneEl.appendChild(headerEl);

    const scrollEl = document.createElement("div");
    scrollEl.className = "piano-scroll";
    const containerEl = document.createElement("div");
    containerEl.className = `piano-container${compact ? " compact-lane" : ""}`;

    // Count white keys so black keys and optional split marker can be positioned accurately
    const whiteMidis = [];
    for (let m = startMidi; m <= endMidi; m++) {
      if (!BLACK_SEMITONES.has(m % 12)) whiteMidis.push(m);
    }
    const totalWhites = whiteMidis.length;
    const whiteWidthPct = 100 / Math.max(1, totalWhites);

    let whiteIndex = 0;
    const whiteIndexByMidi = new Map();
    for (let m = startMidi; m <= endMidi; m++) {
      if (!BLACK_SEMITONES.has(m % 12)) {
        whiteIndexByMidi.set(m, whiteIndex);
        const ch = channelResolver(m);
        const keyEl = createKeyElement(m, false, ch, splitMarkerMidi !== null);
        containerEl.appendChild(keyEl);
        whiteIndex++;
      }
    }

    for (let m = startMidi; m <= endMidi; m++) {
      if (BLACK_SEMITONES.has(m % 12)) {
        const prevWhiteIdx = whiteIndexByMidi.get(m - 1);
        if (prevWhiteIdx === undefined) continue;
        const ch = channelResolver(m);
        const keyEl = createKeyElement(m, true, ch, splitMarkerMidi !== null);
        const leftPct = (prevWhiteIdx + 0.68) * whiteWidthPct;
        keyEl.style.left = `${leftPct.toFixed(3)}%`;
        keyEl.style.width = `${(whiteWidthPct * 0.64).toFixed(3)}%`;
        containerEl.appendChild(keyEl);
      }
    }

    // In Pianist Split Mode, render a visual Split Point Divider on the unified keyboard
    if (splitMarkerMidi !== null && splitMarkerMidi > startMidi && splitMarkerMidi <= endMidi) {
      let splitWhiteIdx = whiteIndexByMidi.get(splitMarkerMidi);
      if (splitWhiteIdx === undefined) {
        const prevW = whiteIndexByMidi.get(splitMarkerMidi - 1);
        splitWhiteIdx = prevW !== undefined ? prevW + 0.5 : 0;
      }
      const splitLeftPct = splitWhiteIdx * whiteWidthPct;
      const dividerEl = document.createElement("div");
      dividerEl.className = "split-divider";
      dividerEl.style.left = `${splitLeftPct.toFixed(3)}%`;
      dividerEl.innerHTML = `<div class="split-divider-badge">◄ Ch 1 Bajos (Left) &bull; Split @ ${midiNoteLabel(splitMarkerMidi)} &bull; Ch 2 Cantabile (Right) ►</div>`;
      containerEl.appendChild(dividerEl);
    }

    scrollEl.appendChild(containerEl);
    laneEl.appendChild(scrollEl);
    return laneEl;
  }

  function buildInteractivePiano() {
    if (!pianoStack) return;
    pianoStack.innerHTML = "";
    const mode = manualSelect ? manualSelect.value : "dual";
    syncKeyboardModeToWorklet();

    if (mode === "dual") {
      // DEFAULT: Two separate Bandolibre keyboards, one for each MIDI channel!
      // 1. Left Keyboard (Channel 1 — Bajos, C2..B4, MIDI 36..71)
      const leftLane = createPianoLaneDOM({
        laneClass: "left-lane",
        badgeClass: "ch1",
        badgeText: "MIDI Ch 1 • Left Keyboard",
        titleText: "Bajos (Bass Manual — C2 to B4, MIDI 36–71)",
        overlapNoteText: "Overlap C4–B4: Also on Right Ch 2",
        metaText: "Tone: Deep Wooden Bass Box (255–495 Hz Formant, Lastre Bass) • Pan: Left",
        startMidi: 36,
        endMidi: 71,
        channelResolver: () => KBD_CHANNEL.left,
        compact: true,
        splitMarkerMidi: null,
      });

      // 2. Right Keyboard (Channel 2 — Cantabile, C4..C7, MIDI 60..96)
      const rightLane = createPianoLaneDOM({
        laneClass: "right-lane",
        badgeClass: "ch2",
        badgeText: "MIDI Ch 2 • Right Keyboard",
        titleText: "Cantabile (Treble Manual — C4 to C7, MIDI 60–96)",
        overlapNoteText: "Overlap C4–B4: Also on Left Ch 1",
        metaText: "Tone: Singing Metal Grill (1–4 kHz Formants, Fast 4' Octave) • Pan: Right",
        startMidi: 60,
        endMidi: 96,
        channelResolver: () => KBD_CHANNEL.right,
        compact: true,
        splitMarkerMidi: null,
      });

      pianoStack.appendChild(leftLane);
      pianoStack.appendChild(rightLane);
    } else if (mode === "pianist_split") {
      // PIANIST SPLIT MODE: Single unified piano keyboard (C2..C7) split at `currentSplitNote`
      const splitLabel = midiNoteLabel(currentSplitNote);
      const lowMaxLabel = midiNoteLabel(currentSplitNote - 1);
      const splitLane = createPianoLaneDOM({
        laneClass: "split-lane",
        badgeClass: "ch1",
        badgeText: `Pianist Split @ ${splitLabel}`,
        titleText: `Single Unified Keyboard: Ch 1 Bajos (C2–${lowMaxLabel}) | Ch 2 Cantabile (${splitLabel}–C7)`,
        overlapNoteText: "Bandoneon Overlap Zone: C4–B4",
        metaText: "Routes single-keyboard or 1-channel MIDI piano input into Ch 1 (Left Tone & Pan) / Ch 2 (Right Tone & Pan)",
        startMidi: 36,
        endMidi: 96,
        channelResolver: (m) => (m < currentSplitNote ? KBD_CHANNEL.left : KBD_CHANNEL.right),
        compact: false,
        splitMarkerMidi: currentSplitNote,
      });
      pianoStack.appendChild(splitLane);
    } else if (mode === "left") {
      const leftOnly = createPianoLaneDOM({
        laneClass: "left-lane",
        badgeClass: "ch1",
        badgeText: "MIDI Ch 1 • Left Keyboard Only",
        titleText: "Bajos (Bass Manual — C2 to B4, MIDI 36–71)",
        overlapNoteText: "Overlap C4–B4",
        metaText: "Tone: Deep Wooden Bass Box • Pan: Left",
        startMidi: 36,
        endMidi: 71,
        channelResolver: () => KBD_CHANNEL.left,
        compact: false,
        splitMarkerMidi: null,
      });
      pianoStack.appendChild(leftOnly);
    } else {
      const rightOnly = createPianoLaneDOM({
        laneClass: "right-lane",
        badgeClass: "ch2",
        badgeText: "MIDI Ch 2 • Right Keyboard Only",
        titleText: "Cantabile (Treble Manual — C4 to C7, MIDI 60–96)",
        overlapNoteText: "Overlap C4–B4",
        metaText: "Tone: Singing Metal Grill • Pan: Right",
        startMidi: 60,
        endMidi: 96,
        channelResolver: () => KBD_CHANNEL.right,
        compact: false,
        splitMarkerMidi: null,
      });
      pianoStack.appendChild(rightOnly);
    }
  }

  function createKeyElement(midi, isBlack, ch, showSplitTint = false) {
    const label = midiNoteLabel(midi);
    const handTag = ch === KBD_CHANNEL.left ? "Ch1•Bajos" : "Ch2•Cant";
    const isOverlap = isOverlapMidiNote(midi);
    const tintClass =
      !isBlack && showSplitTint
        ? ch === KBD_CHANNEL.left
          ? " left-tint"
          : " right-tint"
        : "";

    const keyEl = document.createElement("div");
    keyEl.className = `piano-key ${isBlack ? "black-key" : "white-key"}${isOverlap ? " overlap-key" : ""}${tintClass}`;
    keyEl.dataset.midi = midi;
    keyEl.dataset.channel = ch;
    keyEl.dataset.noteLabel = label;

    keyEl.innerHTML = `
      <div class="key-cc-fill"></div>
      <div class="key-vel-marker"></div>
      <div class="key-top-badge">${label}</div>
      <div class="key-bottom-label">
        ${isOverlap ? `<span class="key-overlap-pill" title="Note ${label} exists on BOTH Bandolibre keyboards (Ch 1 Bajos & Ch 2 Cantabile)">BOTH</span>` : ""}
        <span class="key-note-name">${label}</span>
        <span class="key-hand-tag">${handTag}</span>
      </div>
    `;

    keyEl.addEventListener("pointerdown", async (e) => {
      e.preventDefault();
      keyEl.setPointerCapture(e.pointerId);
      // Record state immediately so pointerup during async WASM load never leaves a stuck note
      const state = { midi, channel: ch, keyEl, statusOff: 0x80 | (ch - 1), cancelled: false };
      activePointers.set(e.pointerId, state);
      await ensureAudioStarted();
      if (state.cancelled) return;
      const started = startNoteFromPointer(keyEl, e.clientY);
      if (started) {
        activePointers.set(e.pointerId, started);
      } else {
        activePointers.delete(e.pointerId);
      }
    });

    keyEl.addEventListener("pointermove", (e) => {
      const state = activePointers.get(e.pointerId);
      if (!state) return;
      e.preventDefault();
      // Moving the mouse up and down on a pressed key continuously modulates CC#11!
      const ratio = yRatioFromKeyEvent(state.keyEl, e.clientY);
      const cc14 = Math.round(ratio * 16383);
      const msb = (cc14 >> 7) & 0x7f;
      const lsb = cc14 & 0x7f;
      updateCC11UI(msb, lsb, true);
    });

    const releasePointer = (e) => {
      const state = activePointers.get(e.pointerId);
      if (!state) return;
      e.preventDefault();
      state.cancelled = true;
      activePointers.delete(e.pointerId);
      if (!sustainHoldToggle.checked) {
        sendMidiToWorklet(state.statusOff, state.midi, 64);
        highlightKeyUI(state.midi, false, 0, liveCcMsb, null, state.channel);
      }
    };

    keyEl.addEventListener("pointerup", releasePointer);
    keyEl.addEventListener("pointercancel", releasePointer);
    return keyEl;
  }

  manualSelect.addEventListener("change", () => {
    sendAllNotesOff();
    buildInteractivePiano();
    const mode = manualSelect.value;
    if (mode === "dual") {
      logMidi("Keyboard Mode -> Dual Keyboards (Ch 1 Left Bajos C2–B4 + Ch 2 Right Cantabile C4–C7)");
    } else if (mode === "pianist_split") {
      logMidi(`Keyboard Mode -> Pianist Split Mode (Single Unified Keyboard split at ${midiNoteLabel(currentSplitNote)})`);
    }
  });

  if (splitNoteSelect) {
    splitNoteSelect.addEventListener("change", (e) => {
      currentSplitNote = Number(e.target.value) || 60;
      sendAllNotesOff();
      buildInteractivePiano();
      logMidi(`Updated Pianist Split Point -> ${midiNoteLabel(currentSplitNote)} (MIDI ${currentSplitNote})`);
    });
  }

  buildInteractivePiano();

  // --- Tango Chord & Bellows Swell Demo ---
  let demoInterval = null;
  demoBtn.addEventListener("click", async () => {
    await ensureAudioStarted();
    if (demoInterval) {
      clearInterval(demoInterval);
      demoInterval = null;
    }
    sendAllNotesOff();

    const chord = [
      { midi: 45, ch: KBD_CHANNEL.left, vel: 96 },  // A2 (Bajos)
      { midi: 52, ch: KBD_CHANNEL.left, vel: 90 },  // E3 (Bajos)
      { midi: 69, ch: KBD_CHANNEL.right, vel: 102 }, // A4 (Cantabile)
      { midi: 72, ch: KBD_CHANNEL.right, vel: 95 },  // C5 (Cantabile)
      { midi: 76, ch: KBD_CHANNEL.right, vel: 98 },  // E5 (Cantabile)
    ];

    updateCC11UI(28, 0, true);
    for (const c of chord) {
      sendMidiToWorklet(0x90 | (c.ch - 1), c.midi, c.vel);
      highlightKeyUI(c.midi, true, c.vel, 28, c.vel / 127, c.ch);
    }
    updateVelocityUI(102);
    logMidi("Playing 5-voice A-minor Tango Chord with CC#11 Bellows Swell…");

    let step = 0;
    demoInterval = setInterval(() => {
      step++;
      const t = step * 0.025;
      if (t < 1.1) {
        updateCC11UI(28 + (t / 1.1) * 95, 0, true);
      } else if (t < 2.4) {
        const base = 123 - ((t - 1.1) / 1.3) * 85;
        const vib = Math.sin(t * 20.0) * 5;
        updateCC11UI(base + vib, 0, true);
      } else {
        for (const c of chord) {
          sendMidiToWorklet(0x80 | (c.ch - 1), c.midi, 64);
          highlightKeyUI(c.midi, false, 0, 85, null, c.ch);
        }
        updateCC11UI(85, 0, true);
        clearInterval(demoInterval);
        demoInterval = null;
      }
    }, 25);
  });

  // --- Overlap Octave Comparison Demo (Ch 1 Left Bajos vs Ch 2 Right Cantabile vs Both) ---
  if (overlapDemoBtn) {
    overlapDemoBtn.addEventListener("click", async () => {
      await ensureAudioStarted();
      if (demoInterval) {
        clearInterval(demoInterval);
        demoInterval = null;
      }
      sendAllNotesOff();
      if (manualSelect && manualSelect.value !== "dual") {
        manualSelect.value = "dual";
        buildInteractivePiano();
      }

      // E4 (MIDI 64) + A4 (MIDI 69) lie inside the C4..B4 overlap octave present on both keyboards
      const overlapNotes = [64, 69];
      let phase = 0;

      const startPhase = (newPhase) => {
        sendAllNotesOff();
        phase = newPhase;
        updateCC11UI(96, 0, true);
        updateVelocityUI(100);
        if (phase === 1) {
          for (const m of overlapNotes) {
            sendMidiToWorklet(0x90, m, 100); // Ch 1 Left Bajos
            highlightKeyUI(m, true, 100, 96, 0.78, KBD_CHANNEL.left);
          }
          logMidi("Overlap Demo [1/3]: E4 + A4 on MIDI Ch 1 (Left Keyboard • Bajos — Deep Woody Box, Left Pan)");
        } else if (phase === 2) {
          for (const m of overlapNotes) {
            sendMidiToWorklet(0x91, m, 100); // Ch 2 Right Cantabile
            highlightKeyUI(m, true, 100, 96, 0.78, KBD_CHANNEL.right);
          }
          logMidi("Overlap Demo [2/3]: Same E4 + A4 on MIDI Ch 2 (Right Keyboard • Cantabile — Bright Metal Grill, Right Pan)");
        } else if (phase === 3) {
          for (const m of overlapNotes) {
            sendMidiToWorklet(0x90, m, 100); // Ch 1 Left
            sendMidiToWorklet(0x91, m, 100); // Ch 2 Right
            highlightKeyUI(m, true, 100, 96, 0.78, KBD_CHANNEL.left);
            highlightKeyUI(m, true, 100, 96, 0.78, KBD_CHANNEL.right);
          }
          logMidi("Overlap Demo [3/3]: E4 + A4 on BOTH Keyboards Simultaneously (Ch 1 Left + Ch 2 Right Stereo Unison)");
        }
      };

      startPhase(1);
      let step = 0;
      demoInterval = setInterval(() => {
        step++;
        const t = step * 0.04;
        const swell = 86 + Math.sin(t * 5.0) * 22;
        updateCC11UI(swell, 0, true);
        if (step === 32) {
          startPhase(2);
        } else if (step === 64) {
          startPhase(3);
        } else if (step >= 98) {
          sendAllNotesOff();
          updateCC11UI(90, 0, true);
          clearInterval(demoInterval);
          demoInterval = null;
          logMidi("Completed Overlap Comparison Demo.");
        }
      }, 40);
    });
  }

  if (abCycleBtn) {
    abCycleBtn.addEventListener("click", async () => {
      await ensureAudioStarted();
      if (demoInterval) {
        clearInterval(demoInterval);
        demoInterval = null;
      }
      sendAllNotesOff();

      const chord = [
        { midi: 45, ch: KBD_CHANNEL.left, vel: 100 },  // A2 (Bajos)
        { midi: 52, ch: KBD_CHANNEL.left, vel: 92 },   // E3 (Bajos)
        { midi: 69, ch: KBD_CHANNEL.right, vel: 104 }, // A4 (Cantabile)
        { midi: 72, ch: KBD_CHANNEL.right, vel: 96 },  // C5 (Cantabile)
        { midi: 76, ch: KBD_CHANNEL.right, vel: 100 }, // E5 (Cantabile)
      ];

      const order = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
      let idx = 0;
      setSynthVersion(order[idx]);
      updateCC11UI(96, 0, true);
      for (const c of chord) {
        sendMidiToWorklet(0x90 | (c.ch - 1), c.midi, c.vel);
        highlightKeyUI(c.midi, true, c.vel, 96, c.vel / 127, c.ch);
      }
      updateVelocityUI(104);
      logMidi("Starting 13-Version A/B Cycle (v1 -> v2 -> v3_* -> v3 -> v4_* -> v4 -> v5)…");

      let step = 0;
      demoInterval = setInterval(() => {
        step++;
        const t = step * 0.05;
        const nextIdx = Math.floor(t / 0.85);
        if (nextIdx < order.length) {
          if (nextIdx !== idx) {
            idx = nextIdx;
            setSynthVersion(order[idx]);
          }
          const swell = 88 + Math.sin(t * 5.5) * 24;
          updateCC11UI(swell, 0, true);
        } else {
          for (const c of chord) {
            sendMidiToWorklet(0x80 | (c.ch - 1), c.midi, 64);
            highlightKeyUI(c.midi, false, 0, 90, null, c.ch);
          }
          updateCC11UI(90, 0, true);
          clearInterval(demoInterval);
          demoInterval = null;
          logMidi("Completed 13-Version A/B Cycle (active: v5 Vivo Expressive).");
        }
      }, 50);
    });
  }

  // =========================================================================
  // 4. Real-Time Stereo (L / R) Waveform & Harmonic Spectrum Visualizer
  // =========================================================================
  function computeRmsPercent(timeBuf) {
    let sum = 0;
    for (let i = 0; i < timeBuf.length; i++) {
      sum += timeBuf[i] * timeBuf[i];
    }
    const rms = Math.sqrt(sum / Math.max(1, timeBuf.length));
    return Math.min(100, Math.round(rms * 280));
  }

  function drawScopeAndSpectrum() {
    requestAnimationFrame(drawScopeAndSpectrum);
    if (!analyserNode) return;

    const w = canvas.width;
    const h = canvas.height;
    canvasCtx.fillStyle = "#0e131b";
    canvasCtx.fillRect(0, 0, w, h);

    const freqBins = new Uint8Array(analyserNode.frequencyBinCount);
    analyserNode.getByteFrequencyData(freqBins);
    const barCount = 96;
    const barW = w / barCount;
    for (let i = 0; i < barCount; i++) {
      const mag = freqBins[i] / 255.0;
      const barH = mag * (h * 0.78);
      canvasCtx.fillStyle = `hsl(${175 - mag * 140}, 80%, ${38 + mag * 25}%)`;
      canvasCtx.fillRect(i * barW, h - barH, barW - 1, barH);
    }

    if (analyserL && analyserR) {
      const bufL = new Float32Array(analyserL.fftSize);
      const bufR = new Float32Array(analyserR.fftSize);
      analyserL.getFloatTimeDomainData(bufL);
      analyserR.getFloatTimeDomainData(bufR);

      const pctL = computeRmsPercent(bufL);
      const pctR = computeRmsPercent(bufR);
      if (leftMeterBar) leftMeterBar.style.width = `${pctL}%`;
      if (leftMeterText) leftMeterText.textContent = `${pctL}%`;
      if (rightMeterBar) rightMeterBar.style.width = `${pctR}%`;
      if (rightMeterText) rightMeterText.textContent = `${pctR}%`;

      const sliceW = w / 512;
      // Draw Left channel (Ch 1 Bajos) in Teal
      canvasCtx.strokeStyle = "#2dd4bf";
      canvasCtx.lineWidth = 1.8;
      canvasCtx.beginPath();
      for (let i = 0; i < 512; i++) {
        const x = i * sliceW;
        const y = h * 0.34 - bufL[i] * (h * 0.30);
        if (i === 0) canvasCtx.moveTo(x, y);
        else canvasCtx.lineTo(x, y);
      }
      canvasCtx.stroke();

      // Draw Right channel (Ch 2 Cantabile) in Amber
      canvasCtx.strokeStyle = "#fbbf24";
      canvasCtx.lineWidth = 1.8;
      canvasCtx.beginPath();
      for (let i = 0; i < 512; i++) {
        const x = i * sliceW;
        const y = h * 0.68 - bufR[i] * (h * 0.30);
        if (i === 0) canvasCtx.moveTo(x, y);
        else canvasCtx.lineTo(x, y);
      }
      canvasCtx.stroke();
    } else {
      const timeBuf = new Float32Array(analyserNode.fftSize);
      analyserNode.getFloatTimeDomainData(timeBuf);
      canvasCtx.strokeStyle = "#fbbf24";
      canvasCtx.lineWidth = 2;
      canvasCtx.beginPath();
      const sliceW = w / 512;
      for (let i = 0; i < 512; i++) {
        const x = i * sliceW;
        const y = h * 0.45 - timeBuf[i] * (h * 0.38);
        if (i === 0) canvasCtx.moveTo(x, y);
        else canvasCtx.lineTo(x, y);
      }
      canvasCtx.stroke();
    }
  }

  // =========================================================================
  // 5. Progressive Web App (PWA) Service Worker & Install Prompt
  // =========================================================================
  const installPwaBtn = document.getElementById("btn-install-pwa");
  let deferredInstallPrompt = null;

  if ("serviceWorker" in navigator) {
    window.addEventListener("load", () => {
      navigator.serviceWorker
        .register("./sw.js")
        .then((reg) => {
          logMidi(`PWA Service Worker active (scope: ${reg.scope})`);
        })
        .catch((err) => {
          console.warn("PWA Service Worker registration failed:", err);
        });
    });
  }

  window.addEventListener("beforeinstallprompt", (e) => {
    e.preventDefault();
    deferredInstallPrompt = e;
    if (installPwaBtn) {
      installPwaBtn.style.display = "inline-flex";
    }
  });

  if (installPwaBtn) {
    installPwaBtn.addEventListener("click", async () => {
      if (!deferredInstallPrompt) return;
      deferredInstallPrompt.prompt();
      const { outcome } = await deferredInstallPrompt.userChoice;
      logMidi(`PWA install prompt outcome: ${outcome}`);
      deferredInstallPrompt = null;
      installPwaBtn.style.display = "none";
    });
  }

  window.addEventListener("appinstalled", () => {
    deferredInstallPrompt = null;
    if (installPwaBtn) {
      installPwaBtn.style.display = "none";
    }
    logMidi("BandoPM installed as a standalone PWA!");
  });

  initWebMIDI();
})();
