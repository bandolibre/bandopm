# Bandolibre Physical Modeling Synthesizer

A real-time, physics-informed bandoneon sound synthesizer written in **Rust**, designed specifically for the **[Bandolibre](https://bandolibre.github.io)** open-source MIDI bandoneon controller.

It adopts a **Browser-First (`WebAssembly` + `AudioWorklet` + `WebMIDI`)** deployment strategy while maintaining a strictly portable, platform-agnostic Rust DSP core ready for **Linux Standalone** (`cpal`/`midir` or `JACK`/`PipeWire`) and **Cross-Platform DAW Plugins** (`CLAP` via `nih-plug` or `clack`).

---

## Documentation Index

This repository is structured for AI-assisted and human collaborative development:

| Document | Purpose |
| :--- | :--- |
| [**`AGENTS.md`**](file:///usr/local/google/home/plabatut/projects/synth/AGENTS.md) | **AI Development Guide**: Architectural invariants, real-time audio thread rules, autonomous hill-climbing loop, and workflow instructions for coding agents. |
| [**`docs/VISION.md`**](file:///usr/local/google/home/plabatut/projects/synth/docs/VISION.md) | **Project Vision & Roadmap**: Goals, target hardware (Bandolibre), acoustic philosophy, calibration strategy using real bandoneon samples, and phased target platforms. |
| [**`docs/DESIGN.md`**](file:///usr/local/google/home/plabatut/projects/synth/docs/DESIGN.md) | **System & DSP Architecture**: Rust workspace structure, MIDI/CC#11 control mapping, free-reed physical modeling equations, dual-reed coupling, and body resonance. |
| [**`docs/EVALUATION.md`**](file:///usr/local/google/home/plabatut/projects/synth/docs/EVALUATION.md) | **Autonomous AI Evaluation & Iteration**: Single-loop AI optimization protocol, envelope inversion from audio-only golden WAVs, 5-tier acoustic scorecard, anti-overfitting regularization, and native CPU benchmarks. |
| [**`docs/REFERENCES.md`**](file:///usr/local/google/home/plabatut/projects/synth/docs/REFERENCES.md) | **Research & Reference Library**: Curated academic papers on free-reed acoustics, Bandoneon 2.0 research (UNQ), Bandolibre hardware specs, and the Rust/WASM/CLAP audio ecosystem. |

---

## Core Highlights

1. **Tailored for Bandolibre**:
   - Processes `Note On` / `Note Off` with initial velocity determined by the bellows blade sensor, and continuous `CC#11` (Expression) representing instantaneous bellows pressure.
2. **Physical Modeling First**:
   - Models the self-sustained oscillation of **blown-closed free reeds** driven by unsteady Bernoulli airflow, paired octave reed activation thresholds ($8'$ and $4'$ reeds), mechanical valve noises, and wooden enclosure resonances.
   - Incorporates insights from the **Bandoneon 2.0** research group (Universidad Nacional de Quilmes) on pressure thresholds and physics-informed synthesis.
3. **Sample-Guided Calibration**:
   - Designed alongside an offline calibration suite to fit physical parameters (reed stiffness, damping, pressure thresholds, and body impulse responses) against acoustic measurements from a real bandoneon.
4. **Write Once, Run Everywhere**:
   - Pure Rust DSP core (`synth-core`) with zero OS/runtime dependencies, compiling cleanly to WebAssembly (`wasm32-unknown-unknown`) for zero-install browser playability, native Linux standalone binaries, and `CLAP` plugins.
