(() => {
  "use strict";

  // =========================================================================
  // 1. WebMIDI Device Discovery & Auto-Selection (Mirrors Bandolibre midi.html)
  // =========================================================================
  // The firmware's USB strings (code/main-g474/usb/usb_descriptors.c) are
  // product "Bandolibre" and manufacturer "L'Atelier du Bandonéon Libre".
  // Linux/macOS name the MIDI port after the product, Windows after the
  // manufacturer, so a board is recognised by either string. Compared
  // lowercased, without accents or punctuation. Keep identical to the copy in
  // bandolibre docs/tools/midi.html.
  const BANDOLIBRE_RE = /bandolibre|bandoneonlibre/;
  function normalizeUsbString(s) {
    return String(s || "").normalize("NFD").replace(/[̀-ͯ]/g, "")
      .toLowerCase().replace(/[^a-z0-9]/g, "");
  }
  // Matches code/main-g474/app/keyboard.h in bandolibre/bandolibre:
  // L_MIDI_CH = 0 (1-based ch 1 = Left Wing / Bajos)
  // R_MIDI_CH = 1 (1-based ch 2 = Right Wing / Cantabile)
  const KBD_CHANNEL = { left: 1, right: 2 };

  const statusDot = document.getElementById("statusDot");
  const statusText = document.getElementById("statusText");
  const select = document.getElementById("deviceSelect");
  const audioLed = document.getElementById("audioLed");

  let midiAccess = null;
  let activeKey = null;
  let activeName = null;
  let activeInput = null;
  let activeOutput = null;
  let liveCcMsb = 0;
  let liveCcLsb = 0;
  let lastHardwareCcMsb = 0;
  let mouseCcOverride = false;
  let spinePointerActive = false;

  // ---- Bandolibre SysEx Protocol (mirrors bandolibre/docs/tools/midi.html) ----
  const SYSEX_MSG_HELLO = 0x00;
  const SYSEX_MSG_BELLOWS_DIRECTION = 0x05;
  const BELLOWS_PULL = 0;
  const BELLOWS_PUSH = 1;
  const BELLOWS_NEUTRAL = 2;

  let currentBellowsDirection = BELLOWS_NEUTRAL;
  let displayBellowsDirection = BELLOWS_PULL; // Default warm amber (Pull / Abriendo)
  let sysexBuf = [];
  let inSysex = false;

  function sysexChecksum(bytes) {
    let cs = 0;
    let i = 0;
    for (; i + 1 < bytes.length; i += 2) cs ^= bytes[i] | (bytes[i + 1] << 8);
    if (i < bytes.length) cs ^= bytes[i];
    return cs & 0xffff;
  }

  function encode7Sysex(bytes) {
    const out = [];
    for (let i = 0; i < bytes.length; i += 7) {
      const group = bytes.slice(i, i + 7);
      let msb = 0;
      for (let j = 0; j < group.length; j++) if (group[j] & 0x80) msb |= 1 << j;
      out.push(msb);
      for (const b of group) out.push(b & 0x7f);
    }
    return out;
  }

  function decode7Sysex(bytes) {
    const out = [];
    let i = 0;
    while (i < bytes.length) {
      const msb = bytes[i++];
      const group = bytes.slice(i, i + 7);
      if (group.length === 0 && i < bytes.length) return null;
      for (let j = 0; j < group.length; j++) out.push(group[j] | ((msb & (1 << j)) ? 0x80 : 0));
      i += group.length;
    }
    return out;
  }

  function buildSysexFrame(payload) {
    const cs = sysexChecksum(payload);
    const body = encode7Sysex([cs & 0xff, cs >> 8, ...payload]);
    return Uint8Array.from([0xf0, ...body, 0xf7]);
  }

  function unpackSysexFrame(frame) {
    if (frame.length < 5 || frame[0] !== 0xf0 || frame[frame.length - 1] !== 0xf7) return null;
    const body = decode7Sysex(frame.slice(1, frame.length - 1));
    if (body === null || body.length < 3) return null;
    const receivedChecksum = body[0] | (body[1] << 8);
    const covered = Uint8Array.from(body.slice(2));
    if (sysexChecksum(covered) !== receivedChecksum) return null;
    return covered;
  }

  function parseHelloBellowsDirection(payload) {
    // HELLO response layout: [0x00, version_str..., 0x00, count_lo, count_hi, wing_l, wing_r, bellows, ...]
    let pos = 1;
    while (pos < payload.length && payload[pos] !== 0) pos++;
    if (pos >= payload.length) return null;
    pos++; // skip NUL terminator
    pos += 4; // skip uint16 count + 2 wing ID bytes
    if (pos >= payload.length) return null;
    return payload[pos];
  }

  function updateBellowsDirectionUI() {
    const isPush = displayBellowsDirection === BELLOWS_PUSH;
    const dirClass = isPush ? "dir-push" : "dir-pull";
    const oppClass = isPush ? "dir-pull" : "dir-push";

    const spineEl = document.getElementById("bellowsSpine");
    if (spineEl) {
      spineEl.classList.remove(oppClass);
      spineEl.classList.add(dirClass);
    }

    const dirBadge = document.getElementById("bellowsSpineDir");
    if (dirBadge) {
      dirBadge.textContent = isPush ? "Push" : "Pull";
    }

    if (pianoStack) {
      pianoStack.classList.remove(oppClass);
      pianoStack.classList.add(dirClass);
    }
  }

  function setBellowsDirection(dir) {
    currentBellowsDirection = dir;
    if (dir === BELLOWS_PUSH || dir === BELLOWS_PULL) {
      displayBellowsDirection = dir;
    }
    updateBellowsDirectionUI();
  }

  // device: an entry of collectDevices() (name + manufacturer of its ports).
  function isBandolibre(device) {
    return !!device && [device.name, device.manufacturer]
      .some((s) => BANDOLIBRE_RE.test(normalizeUsbString(s)));
  }

  function setStatus(kind, text) {
    if (statusDot) {
      statusDot.className = "led" + (kind ? " " + kind : "");
    }
    if (statusText) {
      statusText.textContent = text;
    }
  }

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
        const manufacturer = (input && input.manufacturer) || (output && output.manufacturer) || "";
        devices.push({ key, name, manufacturer, input, output, index: i, total: count });
      }
    }
    return devices;
  }

  function deviceLabel(device) {
    return device.total > 1 ? `${device.name} (${device.index + 1})` : device.name;
  }

  function renderSelector() {
    if (!select) return;
    const devices = collectDevices();
    const previousValue = activeKey || "";

    select.innerHTML = "";
    const emptyOpt = document.createElement("option");
    emptyOpt.value = "";
    emptyOpt.textContent = devices.length ? "Select MIDI…" : "No MIDI device";
    select.appendChild(emptyOpt);

    for (const device of devices) {
      const opt = document.createElement("option");
      opt.value = device.key;
      opt.textContent = deviceLabel(device);
      select.appendChild(opt);
    }
    select.value = previousValue;
    if (select.value !== previousValue) select.value = "";
  }

  function updateStatus() {
    if (!midiAccess) return;
    if (activeName) {
      setStatus("ok", "MIDI");
    } else {
      const hasAnyDevice = collectDevices().length > 0;
      setStatus(hasAnyDevice ? "warn" : "", "MIDI");
    }
  }

  function disconnect() {
    if (activeInput) activeInput.onmidimessage = null;
    activeKey = null;
    activeName = null;
    activeInput = null;
    activeOutput = null;
    sysexBuf = [];
    inSysex = false;
    sendAllNotesOff();
    renderSelector();
    updateStatus();
  }

  function connectTo(key) {
    const device = collectDevices().find((d) => d.key === key);
    if (!device) return;

    if (activeKey) disconnect();
    activeKey = device.key;
    activeName = device.name;
    activeInput = device.input;
    activeOutput = device.output;
    if (activeInput) activeInput.onmidimessage = handleHardwareMidiMessage;

    // Query initial bellows direction via Bandolibre SYSEX_MSG_HELLO if output is available
    if (activeOutput) {
      try {
        activeOutput.send(buildSysexFrame([SYSEX_MSG_HELLO]));
      } catch (_) {}
    }

    renderSelector();
    updateStatus();
  }

  function maybeAutoConnect() {
    if (activeKey) return;
    const devices = collectDevices();
    const fullBando = devices.find((d) => isBandolibre(d) && d.input && d.output);
    const anyBando = fullBando || devices.find((d) => isBandolibre(d) && d.input);
    if (anyBando) connectTo(anyBando.key);
  }

  function reconcileDevices() {
    if (activeKey && !collectDevices().some((d) => d.key === activeKey)) {
      disconnect();
    }
    renderSelector();
    maybeAutoConnect();
    updateStatus();
  }

  if (select) {
    select.addEventListener("change", () => {
      if (!select.value) {
        disconnect();
      } else {
        connectTo(select.value);
      }
    });
  }

  async function initWebMIDI() {
    if (!navigator.requestMIDIAccess) {
      setStatus("bad", "MIDI");
      return;
    }
    try {
      try {
        midiAccess = await navigator.requestMIDIAccess({ sysex: true });
      } catch (_) {
        midiAccess = await navigator.requestMIDIAccess({ sysex: false });
      }
      midiAccess.onstatechange = () => {
        reconcileDevices();
      };
      reconcileDevices();
      setInterval(reconcileDevices, 1000);
    } catch (_) {
      setStatus("bad", "MIDI");
    }
  }

  function handleHardwareMidiMessage(event) {
    const bytes = Array.from(event.data);
    if (bytes.length === 0) return;

    // Strip Active Sensing (0xFE) before the SysEx accumulator so it never corrupts a frame
    if (bytes.length === 1 && bytes[0] === 0xfe) return;

    const startingSysex = bytes[0] === 0xf0 || inSysex;
    if (!startingSysex) {
      ensureAudioStarted();
      const status = bytes[0] & 0xf0;
      const chZero = bytes[0] & 0x0f;
      const channel = chZero + 1;
      const d1 = bytes[1] || 0;
      const d2 = bytes[2] || 0;

      if (status === 0xb0 && (d1 === 11 || d1 === 43)) {
        const prevHwCc = lastHardwareCcMsb;
        if (d1 === 11) {
          lastHardwareCcMsb = d2;
        }
        const mouseBusy = activePointers.size > 0 || spinePointerActive || latchedNotes.size > 0;
        const physicalBellowsMoved =
          d1 === 11 && d2 > 6 && Math.abs(d2 - prevHwCc) >= 4 && !mouseBusy;
        if (physicalBellowsMoved) {
          mouseCcOverride = false;
        }
        if (mouseBusy || mouseCcOverride) {
          // Ignore continuous idle hardware CC#11 stream while playing with the mouse
          return;
        }
        sendMidiToWorklet(bytes[0], d1, d2);
        if (d1 === 11) {
          liveCcMsb = d2;
          updateCC11UI(liveCcMsb, liveCcLsb, false);
        } else {
          liveCcLsb = d2;
          updateCC11UI(liveCcMsb, liveCcLsb, false);
        }
        return;
      }

      if (status === 0x90 && d2 > 0) {
        // Physical key pressed on hardware: hand expression control back to hardware
        if (activePointers.size === 0 && !spinePointerActive && mouseCcOverride) {
          mouseCcOverride = false;
          updateCC11UI(lastHardwareCcMsb, liveCcLsb, true);
        }
        sendMidiToWorklet(bytes[0], d1, d2);
        const effCh = effectiveChannelForHardwareMidi(channel, d1);
        highlightKeyUI(d1, true, d2, liveCcMsb, null, effCh);
      } else if (status === 0x80 || (status === 0x90 && d2 === 0)) {
        sendMidiToWorklet(bytes[0], d1, d2);
        const effCh = effectiveChannelForHardwareMidi(channel, d1);
        highlightKeyUI(d1, false, 0, liveCcMsb, null, effCh);
      } else {
        sendMidiToWorklet(bytes[0], d1, d2);
      }
      return;
    }

    // Accumulate 0xF0 ... 0xF7 SysEx frames (mirrors handleMidiMessage in Bandolibre midi.html)
    for (const b of bytes) {
      if (b === 0xf0) {
        inSysex = true;
        sysexBuf = [b];
      } else if (inSysex) {
        sysexBuf.push(b);
        if (b === 0xf7) {
          inSysex = false;
          const payload = unpackSysexFrame(Uint8Array.from(sysexBuf));
          if (payload && payload.length >= 1) {
            if (payload[0] === SYSEX_MSG_BELLOWS_DIRECTION && payload.length >= 2) {
              setBellowsDirection(payload[1]);
            } else if (payload[0] === SYSEX_MSG_HELLO) {
              const helloDir = parseHelloBellowsDirection(payload);
              if (helloDir !== null) {
                setBellowsDirection(helloDir);
              }
            }
          }
          sysexBuf = [];
        }
      }
    }
  }

  // =========================================================================
  // 2. WebAudio + WASM AudioWorklet Engine & Synthesis Model Selector
  // =========================================================================
  const SYNTH_VERSIONS = [
    { id: 12, slug: "v5", label: "v5 — Vivo Expressive ★" },
    { id: 11, slug: "v4", label: "v4 — Unified SOTA" },
    { id: 6, slug: "v4_cabinet", label: "v4 — Cabinet 10-Mode IR" },
    { id: 7, slug: "v4_slot", label: "v4 — 3-Regime Slot ADAA" },
    { id: 8, slug: "v4_coupled", label: "v4 — Coupled 2nd-Order ODE" },
    { id: 9, slug: "v4_celda", label: "v4 — Celda Cavity SVF" },
    { id: 10, slug: "v4_zapatilla", label: "v4 — Zapatilla Valve" },
    { id: 5, slug: "v3", label: "v3 — Unified" },
    { id: 2, slug: "v3_aero", label: "v3 — Aero Monopole Jet" },
    { id: 3, slug: "v3_beam", label: "v3 — Clamped-Bar Modes" },
    { id: 4, slug: "v3_transient", label: "v3 — Hopf Onset Lag" },
    { id: 1, slug: "v2", label: "v2 — ADAA + Baffle" },
    { id: 0, slug: "v1", label: "v1 — Lumped Clipper" },
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
  const pendingMainMidi = [];

  // Fallback main-thread WASM mirror so the spectrogram & L/R meters animate from MIDI
  // even while the browser keeps AudioContext suspended before the first click on reload.
  let mirrorWasm = null;
  let mirrorCtx = 0;
  let mirrorLeftPtr = 0;
  let mirrorRightPtr = 0;
  const FFT_N = 2048;
  const FFT_BINS = FFT_N >> 1; // 1024
  const mirrorRingL = new Float32Array(FFT_N);
  const mirrorRingR = new Float32Array(FFT_N);
  const mirrorSmoothMagL = new Float32Array(FFT_BINS);
  const mirrorSmoothMagR = new Float32Array(FFT_BINS);
  const mirrorFreqBinsL = new Uint8Array(FFT_BINS);
  const mirrorFreqBinsR = new Uint8Array(FFT_BINS);
  const fftRe = new Float32Array(FFT_N);
  const fftIm = new Float32Array(FFT_N);
  const hannWin = new Float32Array(FFT_N);
  for (let i = 0; i < FFT_N; i++) {
    hannWin[i] = 0.5 * (1.0 - Math.cos((2.0 * Math.PI * i) / (FFT_N - 1)));
  }

  const startBtn = document.getElementById("btn-start");
  const panicBtn = document.getElementById("btn-panic");
  const versionSelect = document.getElementById("versionSelect");
  const postPresetSelect = document.getElementById("postPresetSelect");
  const knobsRow = document.getElementById("knobsRow");
  const manualSelect = document.getElementById("manualSelect");
  const manualSegBar = document.getElementById("manualSegBar");
  const splitPointGroup = document.getElementById("splitPointGroup");
  const splitNoteSelect = document.getElementById("splitNoteSelect");
  const invertYToggle = document.getElementById("invertYToggle");
  const sustainHoldToggle = document.getElementById("sustainHoldToggle");
  const pianoStack = document.getElementById("pianoStack");
  const canvas = document.getElementById("scope-canvas");
  const canvasCtx = canvas ? canvas.getContext("2d") : null;
  const leftMeterBar = document.getElementById("leftMeterBar");
  const rightMeterBar = document.getElementById("rightMeterBar");

  // =========================================================================
  // 3. Studio Rotary Knobs (Separation, Compression, Reverb, Room Size, Pre-Delay)
  // =========================================================================
  const REVERB_KNOB_COLOR = "#7da2e0";

  const POST_PRESETS = {
    milonga:  { enabled: true,  comp: 45, revMix: 26, revSize: 50, predelay: 35, spatial: 55 },
    concert:  { enabled: true,  comp: 38, revMix: 40, revSize: 78, predelay: 62, spatial: 72 },
    studio:   { enabled: true,  comp: 65, revMix: 14, revSize: 32, predelay: 18, spatial: 35 },
    binaural: { enabled: true,  comp: 42, revMix: 22, revSize: 44, predelay: 28, spatial: 85 },
    off:      { enabled: false, comp: 0,  revMix: 0,  revSize: 0,  predelay: 0,  spatial: 0  },
  };

  let postEnabled = true;
  let postParams = {
    compAmount: 0.45,
    reverbMix: 0.26,
    reverbSize: 0.50,
    reverbPredelay: 0.35,
    spatialAmount: 0.55,
  };

  const KNOB_SPECS = [
    { id: "sep", label: "Separation",  val: 84, defaultVal: 84, color: "#63ada5" },
    { id: "cmp", label: "Compression", val: 45, defaultVal: 45, color: "#e5a952" },
    { id: "rev", label: "Reverb",      val: 26, defaultVal: 26, color: REVERB_KNOB_COLOR },
    { id: "rom", label: "Room Size",   val: 50, defaultVal: 50, color: REVERB_KNOB_COLOR },
    { id: "pdl", label: "Pre-Delay",   val: 35, defaultVal: 35, color: REVERB_KNOB_COLOR },
  ];

  function polarToCartesian(cx, cy, r, angleDeg) {
    const rad = ((angleDeg - 90) * Math.PI) / 180.0;
    return { x: cx + r * Math.cos(rad), y: cy + r * Math.sin(rad) };
  }

  function describeArc(cx, cy, r, startAngle, endAngle) {
    const start = polarToCartesian(cx, cy, r, endAngle);
    const end = polarToCartesian(cx, cy, r, startAngle);
    const largeArcFlag = endAngle - startAngle <= 180 ? "0" : "1";
    return ["M", start.x, start.y, "A", r, r, 0, largeArcFlag, 0, end.x, end.y].join(" ");
  }

  function syncKnobsToWorklet() {
    const sepKnob = KNOB_SPECS.find((k) => k.id === "sep");
    const cmpKnob = KNOB_SPECS.find((k) => k.id === "cmp");
    const revKnob = KNOB_SPECS.find((k) => k.id === "rev");
    const romKnob = KNOB_SPECS.find((k) => k.id === "rom");
    const pdlKnob = KNOB_SPECS.find((k) => k.id === "pdl");

    currentStereoWidth = (sepKnob ? sepKnob.val : 84) / 100.0;
    postParams.compAmount = (cmpKnob ? cmpKnob.val : 45) / 100.0;
    postParams.reverbMix = (revKnob ? revKnob.val : 26) / 100.0;
    postParams.reverbSize = (romKnob ? romKnob.val : 50) / 100.0;
    postParams.reverbPredelay = (pdlKnob ? pdlKnob.val : 35) / 100.0;

    // Subtle binaural crossfeed scales naturally when post-processing is active
    postEnabled =
      postParams.compAmount > 0.005 ||
      postParams.reverbMix > 0.005;

    if (workletNode) {
      workletNode.port.postMessage({
        type: "set-stereo-width",
        stereoWidth: currentStereoWidth,
      });
      workletNode.port.postMessage({
        type: "set-post-enabled",
        enabled: postEnabled,
      });
      workletNode.port.postMessage({
        type: "set-post-params",
        compAmount: postParams.compAmount,
        reverbMix: postParams.reverbMix,
        reverbSize: postParams.reverbSize,
        reverbPredelay: postParams.reverbPredelay,
        spatialAmount: postParams.spatialAmount,
      });
    }
    if (mirrorWasm && mirrorCtx) {
      if (mirrorWasm.synth_set_stereo_width) {
        mirrorWasm.synth_set_stereo_width(mirrorCtx, +currentStereoWidth);
      }
      if (mirrorWasm.synth_set_post_enabled) {
        mirrorWasm.synth_set_post_enabled(mirrorCtx, postEnabled ? 1 : 0);
      }
      if (mirrorWasm.synth_set_post_params) {
        mirrorWasm.synth_set_post_params(
          mirrorCtx,
          +postParams.compAmount,
          +postParams.reverbMix,
          +postParams.reverbSize,
          +postParams.spatialAmount
        );
      }
      if (mirrorWasm.synth_set_reverb_predelay) {
        mirrorWasm.synth_set_reverb_predelay(mirrorCtx, +postParams.reverbPredelay);
      }
    }
  }

  function renderKnobs() {
    if (!knobsRow) return;
    knobsRow.innerHTML = "";
    const startDeg = -135;
    const endDeg = 135;
    const bgArc = describeArc(26, 26, 20, startDeg, endDeg);

    for (const k of KNOB_SPECS) {
      const curDeg = startDeg + (k.val / 100) * (endDeg - startDeg);
      const valArc = k.val > 0 ? describeArc(26, 26, 20, startDeg, curDeg) : "";
      const tip = polarToCartesian(26, 26, 13, curDeg);
      const center = polarToCartesian(26, 26, 5, curDeg);

      const item = document.createElement("div");
      item.className = "knob-item";
      item.title = "Drag up/down to adjust • Double-click to reset";
      item.innerHTML = `
        <svg width="52" height="52" viewBox="0 0 52 52">
          <path d="${bgArc}" fill="none" stroke="rgba(255,255,255,0.10)" stroke-width="3" stroke-linecap="round" />
          ${valArc ? `<path d="${valArc}" fill="none" stroke="${k.color}" stroke-width="3" stroke-linecap="round" />` : ""}
          <circle cx="26" cy="26" r="15" fill="#1e2025" stroke="rgba(255,255,255,0.12)" stroke-width="1" />
          <line x1="${center.x}" y1="${center.y}" x2="${tip.x}" y2="${tip.y}" stroke="#f4f4f6" stroke-width="2" stroke-linecap="round" />
        </svg>
        <span class="knob-label">${k.label}</span>
        <span class="knob-val">${k.val}%</span>
      `;

      let startY = 0;
      let startVal = k.val;
      item.addEventListener("pointerdown", (e) => {
        e.preventDefault();
        ensureAudioStarted();
        startY = e.clientY;
        startVal = k.val;
        item.setPointerCapture(e.pointerId);

        const onMove = (ev) => {
          const dy = startY - ev.clientY;
          const nextVal = Math.max(0, Math.min(100, Math.round(startVal + dy * 0.65)));
          if (nextVal !== k.val) {
            k.val = nextVal;
            if (k.id !== "sep" && postPresetSelect) {
              postPresetSelect.value = "custom";
            }
            renderKnobs();
            syncKnobsToWorklet();
          }
        };

        const onUp = () => {
          item.removeEventListener("pointermove", onMove);
          item.removeEventListener("pointerup", onUp);
          item.removeEventListener("pointercancel", onUp);
        };

        item.addEventListener("pointermove", onMove);
        item.addEventListener("pointerup", onUp);
        item.addEventListener("pointercancel", onUp);
      });

      item.addEventListener("dblclick", () => {
        ensureAudioStarted();
        k.val = k.defaultVal;
        renderKnobs();
        syncKnobsToWorklet();
      });

      item.addEventListener(
        "wheel",
        (e) => {
          e.preventDefault();
          ensureAudioStarted();
          const step = e.deltaY < 0 ? 2 : -2;
          k.val = Math.max(0, Math.min(100, k.val + step));
          if (k.id !== "sep" && postPresetSelect) {
            postPresetSelect.value = "custom";
          }
          renderKnobs();
          syncKnobsToWorklet();
        },
        { passive: false }
      );

      knobsRow.appendChild(item);
    }
  }

  if (postPresetSelect) {
    postPresetSelect.addEventListener("change", (e) => {
      ensureAudioStarted();
      const p = POST_PRESETS[e.target.value];
      if (!p) return;
      postParams.spatialAmount = p.spatial / 100.0;
      const map = { cmp: p.comp, rev: p.revMix, rom: p.revSize, pdl: p.predelay };
      for (const k of KNOB_SPECS) {
        if (map[k.id] !== undefined) k.val = map[k.id];
      }
      renderKnobs();
      syncKnobsToWorklet();
    });
  }

  renderKnobs();

  // =========================================================================
  // 4. Synthesis Version Dropdown
  // =========================================================================
  function renderVersionSelect() {
    if (!versionSelect) return;
    versionSelect.innerHTML = "";
    for (const v of SYNTH_VERSIONS) {
      const opt = document.createElement("option");
      opt.value = String(v.id);
      opt.textContent = v.label;
      versionSelect.appendChild(opt);
    }
    versionSelect.value = String(activeVersionId);
  }

  function setSynthVersion(versionId) {
    const v = SYNTH_VERSIONS.find((x) => x.id === Number(versionId));
    if (!v) return;
    activeVersionId = v.id;
    if (workletNode) {
      workletNode.port.postMessage({ type: "set-version", versionId: v.id });
    }
    if (mirrorWasm && mirrorCtx && mirrorWasm.synth_set_version) {
      mirrorWasm.synth_set_version(mirrorCtx, v.id | 0);
    }
    if (versionSelect) versionSelect.value = String(activeVersionId);
  }

  if (versionSelect) {
    versionSelect.addEventListener("change", (e) => {
      ensureAudioStarted();
      setSynthVersion(Number(e.target.value));
    });
  }

  renderVersionSelect();

  // =========================================================================
  // 5. WebAudio Initialization & Bellows CC#11 Updates
  // =========================================================================
  async function ensureAudioStarted() {
    unlockAudioSynchronously();
    if (workletNode) return;
    if (audioInitPromise) return audioInitPromise;

    audioInitPromise = (async () => {
      try {
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
        analyserNode.smoothingTimeConstant = 0.78;
        analyserL = audioCtx.createAnalyser();
        analyserL.fftSize = 2048;
        analyserL.smoothingTimeConstant = 0.78;
        analyserR = audioCtx.createAnalyser();
        analyserR.fftSize = 2048;
        analyserR.smoothingTimeConstant = 0.78;
        const splitter = audioCtx.createChannelSplitter(2);

        node.connect(audioCtx.destination);
        node.connect(analyserNode);
        node.connect(splitter);
        splitter.connect(analyserL, 0);
        splitter.connect(analyserR, 1);

        node.port.onmessage = (e) => {
          if (e.data.type === "ready") {
            soundReady = true;
            updateAudioLed();
            sendMidiToWorklet(0xb0, 11, liveCcMsb);
          } else if (e.data.type === "error") {
            if (audioLed) audioLed.className = "led bad";
          }
        };

        const isPianistSplit = manualSelect && manualSelect.value === "pianist_split";
        try {
          const mirrorRes = await WebAssembly.instantiate(wasmBytes, {});
          mirrorWasm = mirrorRes.instance.exports;
          mirrorCtx = mirrorWasm.synth_create(audioCtx.sampleRate || 48000);
          mirrorLeftPtr = mirrorWasm.synth_left_buffer_ptr(mirrorCtx);
          mirrorRightPtr = mirrorWasm.synth_right_buffer_ptr(mirrorCtx);
          if (mirrorWasm.synth_set_version) {
            mirrorWasm.synth_set_version(mirrorCtx, activeVersionId | 0);
          }
          if (mirrorWasm.synth_set_keyboard_mode) {
            mirrorWasm.synth_set_keyboard_mode(mirrorCtx, isPianistSplit ? 1 : 0);
          }
          if (mirrorWasm.synth_set_split_note) {
            mirrorWasm.synth_set_split_note(mirrorCtx, currentSplitNote | 0);
          }
          if (mirrorWasm.synth_set_stereo_width) {
            mirrorWasm.synth_set_stereo_width(mirrorCtx, +currentStereoWidth);
          }
          if (mirrorWasm.synth_set_post_enabled) {
            mirrorWasm.synth_set_post_enabled(mirrorCtx, postEnabled ? 1 : 0);
          }
          if (mirrorWasm.synth_set_post_params) {
            mirrorWasm.synth_set_post_params(
              mirrorCtx,
              +postParams.compAmount,
              +postParams.reverbMix,
              +postParams.reverbSize,
              +postParams.spatialAmount
            );
          }
          if (mirrorWasm.synth_set_reverb_predelay) {
            mirrorWasm.synth_set_reverb_predelay(mirrorCtx, +postParams.reverbPredelay);
          }
        } catch (_) {}

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
        while (pendingMainMidi.length > 0) {
          const ev = pendingMainMidi.shift();
          workletNode.port.postMessage({ type: "midi", status: ev[0], data1: ev[1], data2: ev[2] });
          if (mirrorWasm && mirrorCtx) {
            mirrorWasm.synth_handle_midi(mirrorCtx, ev[0], ev[1], ev[2]);
          }
        }
      } catch (_) {
        audioInitPromise = null;
        if (audioLed) audioLed.className = "led bad";
      }
    })();

    return audioInitPromise;
  }

  function updateAudioLed() {
    if (!audioLed) return;
    if (soundReady && audioCtx && audioCtx.state === "running") {
      audioLed.className = "led ok";
      audioLed.title = "Audio engine running";
    } else if (soundReady) {
      audioLed.className = "led warn";
      audioLed.title = "Browser paused audio on reload — click anywhere on the page to enable sound";
    }
  }

  for (const evtName of ["pointerdown", "pointerup", "mousedown", "click", "touchstart", "touchend", "keydown", "keyup", "wheel"]) {
    window.addEventListener(
      evtName,
      () => {
        unlockAudioSynchronously();
        ensureAudioStarted();
      },
      { passive: true }
    );
  }

  window.addEventListener("focus", () => {
    unlockAudioSynchronously();
  });

  function sendMidiToWorklet(status, data1, data2) {
    if (mirrorWasm && mirrorCtx) {
      mirrorWasm.synth_handle_midi(mirrorCtx, status, data1, data2);
    }
    if (workletNode) {
      workletNode.port.postMessage({ type: "midi", status, data1, data2 });
    } else {
      pendingMainMidi.push([status, data1, data2]);
    }
  }

  function setSpineDisplay(ccValue) {
    const clamped = Math.max(0, Math.min(127, Math.round(ccValue)));
    const pct = ((clamped / 127) * 100).toFixed(0);
    const spineVal = document.getElementById("bellowsSpineVal");
    const spineFill = document.getElementById("bellowsSpineFill");
    if (spineVal) spineVal.textContent = `${pct}%`;
    if (spineFill) spineFill.style.height = `${pct}%`;
  }

  function updateSpineFromActivePointers() {
    let sum = 0;
    let count = 0;
    for (const st of activePointers.values()) {
      if (typeof st.cc === "number") {
        sum += st.cc;
        count++;
      }
    }
    if (count > 0) {
      const avgCc = Math.round(sum / count);
      liveCcMsb = avgCc;
      setSpineDisplay(avgCc);
    }
  }

  function updateCC11UI(msb, lsb = 0, sendToSynth = true) {
    liveCcMsb = Math.max(0, Math.min(127, Math.round(msb)));
    liveCcLsb = Math.max(0, Math.min(127, Math.round(lsb)));
    const pct = ((liveCcMsb / 127) * 100).toFixed(0);
    setSpineDisplay(liveCcMsb);

    if (sendToSynth) {
      sendMidiToWorklet(0xb0, 11, liveCcMsb);
      if (lsb > 0) sendMidiToWorklet(0xb0, 43, liveCcLsb);
    }

    document.querySelectorAll(".piano-key.active").forEach((keyEl) => {
      if (keyEl.dataset.perNoteTouch === "1") return;
      const fillEl = keyEl.querySelector(".key-cc-fill");
      if (fillEl) fillEl.style.height = `${pct}%`;
    });
  }

  function unlockAudioSynchronously() {
    if (!audioCtx) {
      audioCtx = new (window.AudioContext || window.webkitAudioContext)({
        latencyHint: "interactive",
      });
      audioCtx.onstatechange = () => {
        updateAudioLed();
      };
    }
    if (audioCtx.state !== "running") {
      audioCtx.resume().then(updateAudioLed).catch(() => {});
    } else {
      updateAudioLed();
    }
  }

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

  if (panicBtn) {
    panicBtn.addEventListener("click", () => {
      sendAllNotesOff();
    });
  }

  // =========================================================================
  // 6. Interactive Dual-Channel Bandolibre Keyboards & Central Bellows Spine
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
    if (mirrorWasm && mirrorCtx) {
      if (mirrorWasm.synth_set_keyboard_mode) {
        mirrorWasm.synth_set_keyboard_mode(mirrorCtx, isPianistSplit ? 1 : 0);
      }
      if (mirrorWasm.synth_set_split_note) {
        mirrorWasm.synth_set_split_note(mirrorCtx, currentSplitNote | 0);
      }
    }
  }

  function yRatioFromKeyEvent(keyEl, clientY) {
    const rect = keyEl.getBoundingClientRect();
    const rawRatio = (clientY - rect.top) / Math.max(1, rect.height);
    const clamped = Math.max(0, Math.min(1, rawRatio));
    return invertYToggle && invertYToggle.checked ? 1.0 - clamped : clamped;
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
        if (badgeEl) badgeEl.textContent = keyEl.dataset.noteLabel;
      } else {
        keyEl.classList.remove("active");
        if (fillEl) fillEl.style.height = "0%";
        if (badgeEl) badgeEl.textContent = keyEl.dataset.noteLabel;
      }
    });
  }

  const activePointers = new Map();
  const latchedNotes = new Map();

  function startNoteFromPointer(keyEl, clientY) {
    const midi = Number(keyEl.dataset.midi);
    const channel = Number(keyEl.dataset.channel) || KBD_CHANNEL.left;
    const statusOn = 0x90 | (channel - 1);
    const statusPolyAftertouch = 0xa0 | (channel - 1);
    const statusOff = 0x80 | (channel - 1);
    const latchKey = `${channel}:${midi}`;

    if (sustainHoldToggle && sustainHoldToggle.checked && latchedNotes.has(latchKey)) {
      const old = latchedNotes.get(latchKey);
      sendMidiToWorklet(0x80 | (old.channel - 1), old.midi, 64);
      latchedNotes.delete(latchKey);
      keyEl.dataset.perNoteTouch = "0";
      highlightKeyUI(midi, false, 0, liveCcMsb, null, channel);
      return null;
    }

    const ratio = yRatioFromKeyEvent(keyEl, clientY);
    const velocity = Math.max(1, Math.min(127, Math.round(1 + ratio * 126)));
    const noteCc = Math.max(8, Math.min(127, Math.round(ratio * 127)));

    keyEl.dataset.perNoteTouch = "1";
    sendMidiToWorklet(statusOn, midi, velocity);
    // Send MPE / Polyphonic Key Pressure (0xA0) so each multi-touch key has its own independent intensity
    sendMidiToWorklet(statusPolyAftertouch, midi, noteCc);

    const visualY = invertYToggle && invertYToggle.checked ? 1.0 - ratio : ratio;
    highlightKeyUI(midi, true, velocity, noteCc, visualY, channel);

    if (sustainHoldToggle && sustainHoldToggle.checked) {
      latchedNotes.set(latchKey, { midi, channel });
    }

    return {
      midi,
      channel,
      keyEl,
      statusOff,
      statusPolyAftertouch,
      cc: noteCc,
      cancelled: false,
    };
  }

  function createBellowsSpineDOM() {
    const pct = ((liveCcMsb / 127) * 100).toFixed(0);
    const isPush = displayBellowsDirection === BELLOWS_PUSH;
    const dirClass = isPush ? "dir-push" : "dir-pull";
    const dirLabel = isPush ? "Push" : "Pull";

    const spine = document.createElement("div");
    spine.id = "bellowsSpine";
    spine.className = `bellows-spine ${dirClass}`;
    spine.title = "Bellows Expression (CC#11) • Color follows Bandolibre Push/Pull SysEx";
    spine.innerHTML = `
      <span id="bellowsSpineVal" class="bellows-spine-val">${pct}%</span>
      <div class="bellows-spine-track">
        <div id="bellowsSpineFill" class="bellows-spine-fill" style="height:${pct}%;"></div>
      </div>
      <div id="bellowsSpineFooter" class="bellows-spine-footer" title="Push (Cerrando) / Pull (Abriendo)">
        <span id="bellowsSpineDir" class="bellows-spine-dir">${dirLabel}</span>
        <span class="bellows-spine-label">Fueye</span>
      </div>
    `;

    const footerEl = spine.querySelector("#bellowsSpineFooter");
    if (footerEl) {
      footerEl.addEventListener("pointerdown", (e) => {
        e.stopPropagation();
        setBellowsDirection(displayBellowsDirection === BELLOWS_PUSH ? BELLOWS_PULL : BELLOWS_PUSH);
      });
    }

    const setFromPointer = (clientY) => {
      const rect = spine.getBoundingClientRect();
      const norm = 1.0 - (clientY - rect.top) / Math.max(1, rect.height);
      const clamped = Math.max(0, Math.min(1, norm));
      updateCC11UI(Math.round(clamped * 127), 0, true);
    };

    spine.addEventListener("pointerdown", (e) => {
      e.preventDefault();
      ensureAudioStarted();
      spinePointerActive = true;
      mouseCcOverride = true;
      spine.setPointerCapture(e.pointerId);
      setFromPointer(e.clientY);
      const onMove = (ev) => setFromPointer(ev.clientY);
      const onUp = () => {
        spinePointerActive = false;
        spine.removeEventListener("pointermove", onMove);
        spine.removeEventListener("pointerup", onUp);
        spine.removeEventListener("pointercancel", onUp);
      };
      spine.addEventListener("pointermove", onMove);
      spine.addEventListener("pointerup", onUp);
      spine.addEventListener("pointercancel", onUp);
    });

    return spine;
  }

  function createPianoLaneDOM({
    laneClass,
    titleText,
    metaText,
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
        <span>${titleText}</span>
      </div>
      <div class="manual-lane-meta">${metaText || ""}</div>
    `;
    laneEl.appendChild(headerEl);

    const scrollEl = document.createElement("div");
    scrollEl.className = "piano-scroll";
    const containerEl = document.createElement("div");
    containerEl.className = `piano-container${compact ? " compact-lane" : ""}`;

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
        const keyEl = createKeyElement(m, false, ch);
        containerEl.appendChild(keyEl);
        whiteIndex++;
      }
    }

    for (let m = startMidi; m <= endMidi; m++) {
      if (BLACK_SEMITONES.has(m % 12)) {
        const prevWhiteIdx = whiteIndexByMidi.get(m - 1);
        if (prevWhiteIdx === undefined) continue;
        const ch = channelResolver(m);
        const keyEl = createKeyElement(m, true, ch);
        const leftPct = (prevWhiteIdx + 0.68) * whiteWidthPct;
        keyEl.style.left = `${leftPct.toFixed(3)}%`;
        keyEl.style.width = `${(whiteWidthPct * 0.64).toFixed(3)}%`;
        containerEl.appendChild(keyEl);
      }
    }

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
      dividerEl.innerHTML = `<div class="split-divider-badge">Split @ ${midiNoteLabel(splitMarkerMidi)}</div>`;
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
      pianoStack.classList.remove("single-mode");
      const leftLane = createPianoLaneDOM({
        laneClass: "left-lane",
        titleText: "Bajos · Ch 1 (C2–B4)",
        metaText: "",
        startMidi: 36,
        endMidi: 71,
        channelResolver: () => KBD_CHANNEL.left,
        compact: true,
      });

      const spine = createBellowsSpineDOM();

      const rightLane = createPianoLaneDOM({
        laneClass: "right-lane",
        titleText: "Cantabile · Ch 2 (C4–C7)",
        metaText: "",
        startMidi: 60,
        endMidi: 96,
        channelResolver: () => KBD_CHANNEL.right,
        compact: true,
      });

      pianoStack.appendChild(leftLane);
      pianoStack.appendChild(spine);
      pianoStack.appendChild(rightLane);
    } else if (mode === "pianist_split") {
      pianoStack.classList.add("single-mode");
      const splitLabel = midiNoteLabel(currentSplitNote);
      const lowMaxLabel = midiNoteLabel(currentSplitNote - 1);
      const splitLane = createPianoLaneDOM({
        laneClass: "split-lane",
        titleText: `Pianist Split · Bajos (C2–${lowMaxLabel}) | Cantabile (${splitLabel}–C7)`,
        metaText: "",
        startMidi: 36,
        endMidi: 96,
        channelResolver: (m) => (m < currentSplitNote ? KBD_CHANNEL.left : KBD_CHANNEL.right),
        compact: false,
        splitMarkerMidi: currentSplitNote,
      });
      pianoStack.appendChild(splitLane);
      pianoStack.appendChild(createBellowsSpineDOM());
    } else if (mode === "left") {
      pianoStack.classList.add("single-mode");
      const leftOnly = createPianoLaneDOM({
        laneClass: "left-lane",
        titleText: "Bajos · Ch 1 (C2–B4)",
        metaText: "",
        startMidi: 36,
        endMidi: 71,
        channelResolver: () => KBD_CHANNEL.left,
        compact: false,
      });
      pianoStack.appendChild(leftOnly);
      pianoStack.appendChild(createBellowsSpineDOM());
    } else {
      pianoStack.classList.add("single-mode");
      const rightOnly = createPianoLaneDOM({
        laneClass: "right-lane",
        titleText: "Cantabile · Ch 2 (C4–C7)",
        metaText: "",
        startMidi: 60,
        endMidi: 96,
        channelResolver: () => KBD_CHANNEL.right,
        compact: false,
      });
      pianoStack.appendChild(rightOnly);
      pianoStack.appendChild(createBellowsSpineDOM());
    }
    updateBellowsDirectionUI();
  }

  function createKeyElement(midi, isBlack, ch) {
    const label = midiNoteLabel(midi);
    const isOverlap = isOverlapMidiNote(midi);

    const keyEl = document.createElement("div");
    keyEl.className = `piano-key ${isBlack ? "black-key" : "white-key"}${isOverlap ? " overlap-key" : ""}`;
    keyEl.dataset.midi = midi;
    keyEl.dataset.channel = ch;
    keyEl.dataset.noteLabel = label;

    keyEl.innerHTML = isBlack
      ? `
      <div class="key-cc-fill"></div>
      <div class="key-vel-marker"></div>
    `
      : `
      <div class="key-cc-fill"></div>
      <div class="key-vel-marker"></div>
      <div class="key-bottom-label">
        <span class="key-note-name">${label}</span>
      </div>
    `;

    keyEl.addEventListener("pointerdown", async (e) => {
      e.preventDefault();
      keyEl.setPointerCapture(e.pointerId);
      mouseCcOverride = true;
      const state = { midi, channel: ch, keyEl, statusOff: 0x80 | (ch - 1), cancelled: false };
      activePointers.set(e.pointerId, state);
      await ensureAudioStarted();
      if (state.cancelled) return;
      const started = startNoteFromPointer(keyEl, e.clientY);
      if (started) {
        activePointers.set(e.pointerId, started);
        updateSpineFromActivePointers();
      } else {
        activePointers.delete(e.pointerId);
        updateSpineFromActivePointers();
      }
    });

    keyEl.addEventListener("pointermove", (e) => {
      const state = activePointers.get(e.pointerId);
      if (!state) return;
      e.preventDefault();
      const ratio = yRatioFromKeyEvent(state.keyEl, e.clientY);
      const noteCc = Math.max(1, Math.min(127, Math.round(ratio * 127)));
      state.cc = noteCc;
      const statusPoly = state.statusPolyAftertouch || (0xa0 | (state.channel - 1));
      sendMidiToWorklet(statusPoly, state.midi, noteCc);
      const visualY = invertYToggle && invertYToggle.checked ? 1.0 - ratio : ratio;
      highlightKeyUI(state.midi, true, Number(state.keyEl.dataset.lastVel) || noteCc, noteCc, visualY, state.channel);
      updateSpineFromActivePointers();
    });

    const releasePointer = (e) => {
      const state = activePointers.get(e.pointerId);
      if (!state) return;
      e.preventDefault();
      state.cancelled = true;
      activePointers.delete(e.pointerId);
      if (!sustainHoldToggle || !sustainHoldToggle.checked) {
        state.keyEl.dataset.perNoteTouch = "0";
        sendMidiToWorklet(state.statusOff, state.midi, 64);
        highlightKeyUI(state.midi, false, 0, liveCcMsb, null, state.channel);
      }
      updateSpineFromActivePointers();
    };

    keyEl.addEventListener("pointerup", releasePointer);
    keyEl.addEventListener("pointercancel", releasePointer);
    return keyEl;
  }

  if (manualSegBar) {
    manualSegBar.querySelectorAll(".seg-btn").forEach((btn) => {
      btn.addEventListener("click", () => {
        const mode = btn.dataset.mode;
        if (!mode) return;
        manualSegBar.querySelectorAll(".seg-btn").forEach((b) => {
          b.classList.toggle("active", b === btn);
        });
        if (manualSelect) {
          manualSelect.value = mode;
        }
        sendAllNotesOff();
        buildInteractivePiano();
      });
    });
  }

  if (manualSelect) {
    manualSelect.addEventListener("change", () => {
      sendAllNotesOff();
      buildInteractivePiano();
    });
  }

  if (splitNoteSelect) {
    splitNoteSelect.addEventListener("change", (e) => {
      currentSplitNote = Number(e.target.value) || 60;
      sendAllNotesOff();
      buildInteractivePiano();
    });
  }

  buildInteractivePiano();

  // =========================================================================
  // 7. Real-Time Stereo Frequency Spectrum Visualizer (No Time-Domain Wave)
  // =========================================================================
  function computeRmsPercent(timeBuf) {
    let sum = 0;
    for (let i = 0; i < timeBuf.length; i++) {
      sum += timeBuf[i] * timeBuf[i];
    }
    const rms = Math.sqrt(sum / Math.max(1, timeBuf.length));
    return Math.min(100, Math.round(rms * 280));
  }

  function drawChannelSpectrumCurve(ctx, freqBins, sampleRate, w, h, strokeColor, fillColor) {
    const binCount = freqBins.length;
    const nyquist = sampleRate * 0.5;
    const fMin = 55.0;
    const fMax = 9600.0;
    const logRatio = Math.log(fMax / fMin);
    const points = 72;

    ctx.beginPath();
    ctx.moveTo(0, h);
    for (let i = 0; i <= points; i++) {
      const u = i / points;
      const freq = fMin * Math.exp(u * logRatio);
      const binFloat = (freq / nyquist) * binCount;
      const b0 = Math.max(1, Math.min(binCount - 2, Math.floor(binFloat)));
      const frac = binFloat - b0;
      const mag = ((1.0 - frac) * freqBins[b0] + frac * freqBins[b0 + 1]) / 255.0;
      const x = u * w;
      const y = h - Math.pow(mag, 0.85) * (h * 0.88) - 1;
      ctx.lineTo(x, y);
    }
    ctx.lineTo(w, h);
    ctx.closePath();
    ctx.fillStyle = fillColor;
    ctx.fill();

    ctx.beginPath();
    for (let i = 0; i <= points; i++) {
      const u = i / points;
      const freq = fMin * Math.exp(u * logRatio);
      const binFloat = (freq / nyquist) * binCount;
      const b0 = Math.max(1, Math.min(binCount - 2, Math.floor(binFloat)));
      const frac = binFloat - b0;
      const mag = ((1.0 - frac) * freqBins[b0] + frac * freqBins[b0 + 1]) / 255.0;
      const x = u * w;
      const y = h - Math.pow(mag, 0.85) * (h * 0.88) - 1;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.strokeStyle = strokeColor;
    ctx.lineWidth = 1.6;
    ctx.stroke();
  }

  function computeMirrorFftBins(ringBuf, smoothMag, outBytes) {
    for (let i = 0; i < FFT_N; i++) {
      fftRe[i] = ringBuf[i] * hannWin[i];
      fftIm[i] = 0.0;
    }
    // Bit-reversal permutation
    let j = 0;
    for (let i = 0; i < FFT_N; i++) {
      if (i < j) {
        const tr = fftRe[i];
        fftRe[i] = fftRe[j];
        fftRe[j] = tr;
      }
      let m = FFT_N >> 1;
      while (m >= 1 && j >= m) {
        j -= m;
        m >>= 1;
      }
      j += m;
    }
    // Cooley-Tukey radix-2 butterflies
    for (let len = 2; len <= FFT_N; len <<= 1) {
      const half = len >> 1;
      const ang = (-2.0 * Math.PI) / len;
      const wMulRe = Math.cos(ang);
      const wMulIm = Math.sin(ang);
      for (let i = 0; i < FFT_N; i += len) {
        let wRe = 1.0;
        let wIm = 0.0;
        for (let k = 0; k < half; k++) {
          const uIdx = i + k;
          const vIdx = uIdx + half;
          const tRe = wRe * fftRe[vIdx] - wIm * fftIm[vIdx];
          const tIm = wRe * fftIm[vIdx] + wIm * fftRe[vIdx];
          fftRe[vIdx] = fftRe[uIdx] - tRe;
          fftIm[vIdx] = fftIm[uIdx] - tIm;
          fftRe[uIdx] += tRe;
          fftIm[uIdx] += tIm;
          const nextWRe = wRe * wMulRe - wIm * wMulIm;
          wIm = wRe * wMulIm + wIm * wMulRe;
          wRe = nextWRe;
        }
      }
    }
    // Convert to WebAudio AnalyserNode-equivalent dB byte bins (-100 dB .. -30 dB -> 0..255)
    const invN = 2.0 / FFT_N;
    for (let b = 0; b < FFT_BINS; b++) {
      const mag = Math.hypot(fftRe[b], fftIm[b]) * invN;
      const smoothed = 0.78 * smoothMag[b] + 0.22 * mag;
      smoothMag[b] = smoothed;
      const db = 20.0 * Math.log10(Math.max(smoothed, 1e-6));
      const norm = Math.max(0.0, Math.min(1.0, (db + 100.0) / 70.0));
      outBytes[b] = Math.round(norm * 255.0);
    }
  }

  function stepMirrorQuantumsForSpectrum() {
    if (!mirrorWasm || !mirrorCtx) return false;
    const quantumsPerFrame = 6; // 6 * 128 = 768 samples (~16 ms at 48 kHz)
    const totalNew = quantumsPerFrame * 128;
    mirrorRingL.copyWithin(0, totalNew);
    mirrorRingR.copyWithin(0, totalNew);
    let writeOffset = FFT_N - totalNew;
    for (let q = 0; q < quantumsPerFrame; q++) {
      mirrorWasm.synth_render_quantum(mirrorCtx);
      const memBuf = mirrorWasm.memory.buffer;
      const wasmL = new Float32Array(memBuf, mirrorLeftPtr, 128);
      const wasmR = new Float32Array(memBuf, mirrorRightPtr, 128);
      mirrorRingL.set(wasmL, writeOffset);
      mirrorRingR.set(wasmR, writeOffset);
      writeOffset += 128;
    }
    computeMirrorFftBins(mirrorRingL, mirrorSmoothMagL, mirrorFreqBinsL);
    computeMirrorFftBins(mirrorRingR, mirrorSmoothMagR, mirrorFreqBinsR);
    return true;
  }

  function drawFrequencySpectrum() {
    requestAnimationFrame(drawFrequencySpectrum);
    if (!canvas || !canvasCtx) return;

    const w = canvas.width;
    const h = canvas.height;
    canvasCtx.clearRect(0, 0, w, h);

    // Subtle horizontal dB reference hairlines
    canvasCtx.strokeStyle = "rgba(255, 255, 255, 0.05)";
    canvasCtx.lineWidth = 1;
    canvasCtx.beginPath();
    canvasCtx.moveTo(0, h * 0.33);
    canvasCtx.lineTo(w, h * 0.33);
    canvasCtx.moveTo(0, h * 0.66);
    canvasCtx.lineTo(w, h * 0.66);
    canvasCtx.stroke();

    const sr = audioCtx ? audioCtx.sampleRate : 48000;

    if (audioCtx && audioCtx.state === "running" && analyserL && analyserR) {
      const timeL = new Float32Array(analyserL.fftSize);
      const timeR = new Float32Array(analyserR.fftSize);
      analyserL.getFloatTimeDomainData(timeL);
      analyserR.getFloatTimeDomainData(timeR);

      const pctL = computeRmsPercent(timeL);
      const pctR = computeRmsPercent(timeR);
      if (leftMeterBar) leftMeterBar.style.width = `${pctL}%`;
      if (rightMeterBar) rightMeterBar.style.width = `${pctR}%`;

      const freqL = new Uint8Array(analyserL.frequencyBinCount);
      const freqR = new Uint8Array(analyserR.frequencyBinCount);
      analyserL.getByteFrequencyData(freqL);
      analyserR.getByteFrequencyData(freqR);

      drawChannelSpectrumCurve(
        canvasCtx,
        freqL,
        sr,
        w,
        h,
        "rgba(99, 173, 165, 0.92)",
        "rgba(99, 173, 165, 0.22)"
      );
      drawChannelSpectrumCurve(
        canvasCtx,
        freqR,
        sr,
        w,
        h,
        "rgba(229, 169, 82, 0.95)",
        "rgba(229, 169, 82, 0.22)"
      );
    } else if (stepMirrorQuantumsForSpectrum()) {
      const pctL = computeRmsPercent(mirrorRingL);
      const pctR = computeRmsPercent(mirrorRingR);
      if (leftMeterBar) leftMeterBar.style.width = `${pctL}%`;
      if (rightMeterBar) rightMeterBar.style.width = `${pctR}%`;

      drawChannelSpectrumCurve(
        canvasCtx,
        mirrorFreqBinsL,
        sr,
        w,
        h,
        "rgba(99, 173, 165, 0.92)",
        "rgba(99, 173, 165, 0.22)"
      );
      drawChannelSpectrumCurve(
        canvasCtx,
        mirrorFreqBinsR,
        sr,
        w,
        h,
        "rgba(229, 169, 82, 0.95)",
        "rgba(229, 169, 82, 0.22)"
      );
    }
  }

  // =========================================================================
  // 8. Progressive Web App (PWA) Service Worker & Install Prompt
  // =========================================================================
  const installPwaBtn = document.getElementById("btn-install-pwa");
  let deferredInstallPrompt = null;

  if ("serviceWorker" in navigator) {
    window.addEventListener("load", () => {
      navigator.serviceWorker.register("./sw.js").catch(() => {});
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
      await deferredInstallPrompt.userChoice;
      deferredInstallPrompt = null;
      installPwaBtn.style.display = "none";
    });
  }

  window.addEventListener("appinstalled", () => {
    deferredInstallPrompt = null;
    if (installPwaBtn) {
      installPwaBtn.style.display = "none";
    }
  });

  // =========================================================================
  // 9. About / Information Modal
  // =========================================================================
  const infoBtn = document.getElementById("btn-info");
  const infoModal = document.getElementById("info-modal");
  const infoCloseBtn = document.getElementById("btn-info-close");

  if (infoBtn && infoModal) {
    infoBtn.addEventListener("click", () => {
      infoModal.classList.add("open");
    });
    if (infoCloseBtn) {
      infoCloseBtn.addEventListener("click", () => {
        infoModal.classList.remove("open");
      });
    }
    infoModal.addEventListener("click", (e) => {
      if (e.target === infoModal) {
        infoModal.classList.remove("open");
      }
    });
    window.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && infoModal.classList.contains("open")) {
        infoModal.classList.remove("open");
      }
    });
  }

  ensureAudioStarted();
  drawFrequencySpectrum();
  initWebMIDI();
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      document.body.classList.remove("preload");
    });
  });
})();
