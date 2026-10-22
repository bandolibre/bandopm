# AI Development Guide (`AGENTS.md`)

This document defines the architectural rules, engineering invariants, and development workflows for AI coding agents working in this repository.

## 1. Project Context

We are building a **real-time physical modeling bandoneon synthesizer in Rust** tailored for the **Bandolibre** MIDI instrument:
- **Primary Target (Phase 1)**: Web Browser via `WebAssembly` (`wasm32-unknown-unknown`), `WebAudio` (`AudioWorkletProcessor`), and `WebMIDI`.
- **Secondary Targets (Phase 2 & 3)**: Linux Standalone (`cpal` + `midir` / `PipeWire` / `JACK`) and Cross-Platform DAW Plugin (`CLAP` via `clack` or `nih-plug`).
- **Control Input**: MIDI `Note On`, `Note Off` (with velocity sensed from the bellows blade), and continuous `CC#11` (Expression, driven by the bellows pressure sensor).
- **Acoustic Grounding**: Physical modeling of free-reed acoustics (blown-closed cantilever reed + Bernoulli orifice flow + octave reed pair coupling + wooden box resonance), calibrated against real bandoneon recordings.

Always read [`docs/VISION.md`](file:///usr/local/google/home/plabatut/projects/synth/docs/VISION.md), [`docs/DESIGN.md`](file:///usr/local/google/home/plabatut/projects/synth/docs/DESIGN.md), [`docs/EVALUATION.md`](file:///usr/local/google/home/plabatut/projects/synth/docs/EVALUATION.md), and [`docs/REFERENCES.md`](file:///usr/local/google/home/plabatut/projects/synth/docs/REFERENCES.md) before proposing architectural or DSP changes.

---

## 2. Non-Negotiable Architectural Invariants

### 2.1 Strict Separation of Core DSP and Platform Wrappers
- **`crates/synth-core` MUST remain platform-agnostic**:
  - Do **not** import `wasm-bindgen`, `web-sys`, `cpal`, `nih_plug`, or OS-specific crates inside `synth-core`.
  - Keep `synth-core` compatible with `#![no_std]` (or minimal `core` + `alloc` only during initialization) so it compiles effortlessly to `wasm32-unknown-unknown`, embedded targets, and native SIMD targets.
  - All audio processing in `synth-core` operates on plain slices (`&mut [f32]`) and structured MIDI/parameter events.

### 2.2 Real-Time Audio Thread Safety
The `process()` / `render()` function in `synth-core` runs inside a real-time audio callback (e.g., WebAudio `AudioWorklet` 128-sample quantum or DAW/ALSA buffer callback). **Inside the audio render path, you MUST NEVER:**
1. **Allocate or deallocate heap memory** (`Box::new`, `Vec::push` without pre-reserved capacity, `String`, etc.). Pre-allocate all voice states, delay lines, and scratch buffers during engine initialization.
2. **Acquire blocking locks** (`Mutex::lock`, `RwLock`). Use lock-free ring buffers (SPSC queues) or atomic parameter cells for UI/MIDI-to-audio communication.
3. **Perform I/O, logging, or system calls** (`println!`, file reads, network, `Instant::now()`).
4. **Leave unbounded loops or unguarded nonlinearities**:
   - Physical modeling ODE solvers (e.g., Newton-Raphson for reed-flow coupling) must have a fixed, small maximum iteration count (e.g., 3–5 iterations) and sanitizing clamps (`f32::clamp`, NaN/denormal protection) so numerical blow-ups cannot freeze the audio thread.

### 2.3 Denormal & Numerical Stability
- Flush denormals to zero (FTZ/DAZ where available, or add tiny DC offset / explicit threshold checks `< 1e-15`) inside recursive IIR filters, waveguide delay lines, and reed state integrators.
- Every physical parameter update (especially `CC#11` bellows pressure) must pass through a one-pole smoothing filter at audio rate to prevent zipper noise and numerical instability in the ODE solver.

---

## 3. Repository Structure Convention

```text
synth/
├── README.md
├── AGENTS.md
├── Cargo.toml                  # Rust workspace manifest
├── docs/
│   ├── VISION.md               # Product vision, goals, and phased roadmap
│   ├── DESIGN.md               # Detailed DSP & software architecture
│   ├── EVALUATION.md           # Autonomous AI evaluation, scorecard, and perf gates
│   └── REFERENCES.md           # Academic papers, Bandoneon 2.0, Bandolibre specs
├── crates/
│   ├── synth-core/             # Pure Rust DSP engine (voices, reed model, body filter, MIDI handler)
│   ├── synth-wasm/             # WebAssembly bindings for AudioWorklet
│   ├── synth-eval/             # Autonomous evaluation harness (envelope inversion, scorecard, CPU bench)
│   ├── synth-standalone/       # (Phase 2) Linux standalone binary (cpal + midir)
│   └── synth-clap/             # (Phase 3) CLAP plugin wrapper
├── web/                        # Browser frontend (AudioWorklet host, WebMIDI, UI telemetry)
└── assets/
    ├── samples/
    │   └── golden/             # Train & Validation golden bandoneon WAVs (p, mf, ff, staccato, marcato)
    └── presets/                # Smooth physical scaling presets & body resonator parameters
```

---

## 4. Autonomous AI Iteration Protocol (Single-Loop Architecture)

This project is architected for **autonomous, metric-driven AI iteration** (detailed in [`docs/EVALUATION.md`](file:///usr/local/google/home/plabatut/projects/synth/docs/EVALUATION.md)). When iterating on DSP equations or physical presets, AI agents MUST follow these rules:

1. **Single-Loop Guided by the Multi-Tier Scorecard**:
   - Run `cargo run --release -p synth-eval` to generate `target/eval/scorecard.json` and `target/eval/scorecard.md`.
   - Use the **signed physical error deltas** (e.g., `H1..H16` dB error, `attack_rise_time_err_ms`, `octave_4ft_to_8ft_err_db`, `drift_p_to_ff_err_cents`, `octave_band_err_db`) to diagnose *why* the model differs from the golden bandoneon recordings and modify either the Rust DSP equations (`synth-core`) or the parameter scaling laws (`assets/presets/`).
2. **Never Cheat the Metric (Anti-Overfitting Rules)**:
   - **No Per-Sample or Per-Note Lookup Hacks**: Physical parameters across the 142-key compass MUST be defined by smooth physical scaling functions of MIDI pitch / reed dimensions (penalized by the second-difference roughness regularizer $L_{\text{reg}}$).
   - **Respect the Train / Validation Split**: You may inspect detailed per-harmonic diagnostics on the `train/` note split, but a change may only be kept if the held-out **`composite_val_score`** improves (or does not regress beyond tolerance).
3. **Hard Stability & CPU Performance Gates**:
   - Every iteration must pass deterministic unit and stress tests:
     - Zero `NaN`s / `Inf`s under extreme MIDI inputs (rapid `CC#11` jumps $0 \leftrightarrow 127$, max velocity, zero pressure).
     - Silence convergence ($< -90\text{ dBFS}$) when `CC#11` drops to 0 and all notes are off.
     - **Native CPU Budget**: Rendering a 128-sample quantum at `48 kHz` ($2666.7\ \mu\text{s}$ real-time window) must stay under **$350\ \mu\text{s}$ for 16 active voices** (32 coupled reeds) in native `--release` benchmarks.
4. **Keep Documentation Synchronized**:
   - When adding or modifying physical modeling equations, evaluation metrics, MIDI mappings, or crate boundaries, update [`docs/DESIGN.md`](file:///usr/local/google/home/plabatut/projects/synth/docs/DESIGN.md) and [`docs/EVALUATION.md`](file:///usr/local/google/home/plabatut/projects/synth/docs/EVALUATION.md) in the same change.
