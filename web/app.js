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
      highlightKeyUI(d1, true, d2, liveCcMsb);
      logMidi(`Note On  | Ch ${channel} (${channel === KBD_CHANNEL.left ? "Bajos" : "Cantabile"}) Note ${d1} Vel ${d2}`);
    } else if (status === 0x80 || (status === 0x90 && d2 === 0)) {
      highlightKeyUI(d1, false, 0, liveCcMsb);
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
  // 2. WebAudio + WASM AudioWorklet Engine
  // =========================================================================
  let audioCtx = null;
  let workletNode = null;
  let analyserNode = null;
  let audioInitPromise = null;
  let soundReady = false;

  const startBtn = document.getElementById("btn-start");
  const demoBtn = document.getElementById("btn-demo");
  const toneBtn = document.getElementById("btn-tone");
  const panicBtn = document.getElementById("btn-panic");
  const audioStatusBadge = document.getElementById("audioStatusBadge");
  const cc11Slider = document.getElementById("cc11Slider");
  const cc11Readout = document.getElementById("cc11Readout");
  const cc11Bar = document.getElementById("cc11Bar");
  const velReadout = document.getElementById("velReadout");
  const velBar = document.getElementById("velBar");
  const invertYToggle = document.getElementById("invertYToggle");
  const sustainHoldToggle = document.getElementById("sustainHoldToggle");
  const manualSelect = document.getElementById("manualSelect");
  const pianoContainer = document.getElementById("pianoContainer");
  const canvas = document.getElementById("scope-canvas");
  const canvasCtx = canvas.getContext("2d");

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
        // Connect worklet directly to destination AND in parallel to analyserNode
        node.connect(audioCtx.destination);
        node.connect(analyserNode);

        node.port.onmessage = (e) => {
          if (e.data.type === "ready") {
            soundReady = true;
            audioStatusBadge.textContent = `Audio: ${e.data.sampleRate} Hz WASM (${audioCtx.state})`;
            startBtn.textContent = "Sound Active";
            startBtn.disabled = true;
            sendMidiToWorklet(0xb0, 11, liveCcMsb);
          } else if (e.data.type === "error") {
            audioStatusBadge.textContent = `WASM Error`;
            logMidi(`AudioWorklet Error: ${e.data.message}`);
          }
        };

        node.port.postMessage({
          type: "init-wasm",
          wasmBytes,
          tableMode: false,
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
          audioStatusBadge.textContent = `Audio: ${audioCtx.sampleRate} Hz WASM (${audioCtx.state})`;
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
  // 3. Interactive Keyboard:
  //    - Vertical Position of Click -> Note-On Velocity (1..127)
  //    - Moving Mouse Up & Down While Pressed -> Continuous CC#11 Variation
  // =========================================================================
  const NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
  const BLACK_SEMITONES = new Set([1, 3, 6, 8, 10]);

  function midiNoteLabel(midi) {
    const name = NOTE_NAMES[midi % 12];
    const oct = Math.floor(midi / 12) - 1;
    return `${name}${oct}`;
  }

  function channelForMidiNote(midi) {
    const mode = manualSelect.value;
    if (mode === "left") return KBD_CHANNEL.left;   // Ch 1 (0x90)
    if (mode === "right") return KBD_CHANNEL.right; // Ch 2 (0x91)
    return midi < 60 ? KBD_CHANNEL.left : KBD_CHANNEL.right;
  }

  /**
   * Maps a pointer event's vertical Y coordinate within `keyEl` to `[1..127]`.
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

  function highlightKeyUI(midi, isActive, velocity, cc11, clickRatioY = null) {
    const keyEl = pianoContainer.querySelector(`.piano-key[data-midi="${midi}"]`);
    if (!keyEl) return;
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
  }

  // Active pointer tracking: pointerId -> { midi, channel, keyEl }
  const activePointers = new Map();
  // Latched notes when "Latch / Hold Notes" is enabled: midi -> channel
  const latchedNotes = new Map();

  function startNoteFromPointer(keyEl, clientY) {
    const midi = Number(keyEl.dataset.midi);
    const channel = channelForMidiNote(midi);
    const statusOn = 0x90 | (channel - 1);
    const statusOff = 0x80 | (channel - 1);

    // Toggle off if Latch mode is enabled and note is already latched
    if (sustainHoldToggle.checked && latchedNotes.has(midi)) {
      const oldCh = latchedNotes.get(midi);
      sendMidiToWorklet(0x80 | (oldCh - 1), midi, 64);
      latchedNotes.delete(midi);
      highlightKeyUI(midi, false, 0, liveCcMsb);
      logMidi(`Latch Off | Note ${midiNoteLabel(midi)} (${midi})`);
      return null;
    }

    const ratio = yRatioFromKeyEvent(keyEl, clientY);
    const velocity = Math.max(1, Math.min(127, Math.round(1 + ratio * 126)));
    const initialCc11 = Math.max(8, Math.min(127, Math.round(ratio * 127)));

    updateVelocityUI(velocity);
    updateCC11UI(initialCc11, 0, true);
    sendMidiToWorklet(statusOn, midi, velocity);

    const visualY = invertYToggle.checked ? 1.0 - ratio : ratio;
    highlightKeyUI(midi, true, velocity, initialCc11, visualY);

    if (sustainHoldToggle.checked) {
      latchedNotes.set(midi, channel);
    }

    const handLabel = channel === KBD_CHANNEL.left ? "Bajos" : "Cantabile";
    logMidi(
      `Key Press | ${midiNoteLabel(midi)} (${handLabel}) -> Vel=${velocity} (from click height), CC#11=${initialCc11}`
    );

    return { midi, channel, keyEl, statusOff };
  }

  function buildInteractivePiano() {
    pianoContainer.innerHTML = "";
    const mode = manualSelect.value;
    const startMidi = mode === "right" ? 60 : 36; // C4 or C2
    const endMidi = mode === "left" ? 71 : 84;    // B4 or C6

    // First pass: count white keys so black keys can be positioned accurately
    const whiteMidis = [];
    for (let m = startMidi; m <= endMidi; m++) {
      if (!BLACK_SEMITONES.has(m % 12)) whiteMidis.push(m);
    }
    const totalWhites = whiteMidis.length;

    // Create white keys
    let whiteIndex = 0;
    const whiteIndexByMidi = new Map();
    for (let m = startMidi; m <= endMidi; m++) {
      if (!BLACK_SEMITONES.has(m % 12)) {
        whiteIndexByMidi.set(m, whiteIndex);
        const keyEl = createKeyElement(m, false);
        pianoContainer.appendChild(keyEl);
        whiteIndex++;
      }
    }

    // Create black keys overlaid between adjacent white keys
    for (let m = startMidi; m <= endMidi; m++) {
      if (BLACK_SEMITONES.has(m % 12)) {
        const prevWhiteIdx = whiteIndexByMidi.get(m - 1);
        if (prevWhiteIdx === undefined) continue;
        const keyEl = createKeyElement(m, true);
        const whiteWidthPct = 100 / totalWhites;
        const leftPct = (prevWhiteIdx + 0.68) * whiteWidthPct;
        keyEl.style.left = `${leftPct.toFixed(3)}%`;
        keyEl.style.width = `${(whiteWidthPct * 0.64).toFixed(3)}%`;
        pianoContainer.appendChild(keyEl);
      }
    }
  }

  function createKeyElement(midi, isBlack) {
    const label = midiNoteLabel(midi);
    const ch = channelForMidiNote(midi);
    const handTag = ch === KBD_CHANNEL.left ? "Bajos" : "Cant";

    const keyEl = document.createElement("div");
    keyEl.className = `piano-key ${isBlack ? "black-key" : "white-key"}`;
    keyEl.dataset.midi = midi;
    keyEl.dataset.noteLabel = label;

    keyEl.innerHTML = `
      <div class="key-cc-fill"></div>
      <div class="key-vel-marker"></div>
      <div class="key-top-badge">${label}</div>
      <div class="key-bottom-label">
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
        highlightKeyUI(state.midi, false, 0, liveCcMsb);
      }
    };

    keyEl.addEventListener("pointerup", releasePointer);
    keyEl.addEventListener("pointercancel", releasePointer);
    return keyEl;
  }

  manualSelect.addEventListener("change", () => {
    sendAllNotesOff();
    buildInteractivePiano();
  });

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
      highlightKeyUI(c.midi, true, c.vel, 28, c.vel / 127);
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
          highlightKeyUI(c.midi, false, 0, 85);
        }
        updateCC11UI(85, 0, true);
        clearInterval(demoInterval);
        demoInterval = null;
      }
    }, 25);
  });

  // =========================================================================
  // 4. Real-Time Waveform & Harmonic Spectrum Visualizer
  // =========================================================================
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

  initWebMIDI();
})();
