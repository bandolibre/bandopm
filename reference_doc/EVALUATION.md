# Autonomous AI Evaluation & Iteration Architecture (`EVALUATION.md`)

This document specifies the **test-driven, metric-guided autonomous iteration loop** for developing the Bandolibre physical modeling synthesizer.

Because an AI coding agent cannot "listen" to audio in real time, this repository replaces subjective ear-tuning with an **Automated Acoustic Oscilloscope & Scorecard (`synth-eval`)**. In a **Single-Loop Architecture**, the AI agent directly modifies both the **Rust DSP equations (`synth-core`)** and the **physical parameter presets**, guided by a structured multi-tier scorecard comparing synthesized audio against golden recordings of a real bandoneon.

---

## 1. The Autonomous Hill-Climbing Loop

Every DSP or parameter iteration performed by an AI agent follows a strict scientific loop:

```mermaid
flowchart TD
    Read["1. Read Latest Scorecard\n(target/eval/scorecard.md & .json)"] --> Hypothesize["2. Formulate Physical Hypothesis\n(Identify worst category & signed error deltas)"]
    Hypothesize --> Edit["3. Edit Rust DSP or Scaling Laws\n(crates/synth-core or assets/presets/)"]
    Edit --> Run["4. Run Evaluation & Perf Harness\n(cargo run --release -p synth-eval)"]
    Run --> Gate{"5. Check Hard Gates:\n- 0 NaNs / Clipping?\n- CPU < Budget?\n- Val Score Improved?"}
    Gate -- "Pass" --> Commit["6a. Keep Change & Log Iteration"]
    Gate -- "Fail" --> Revert["6b. Revert or Refine Hypothesis"]
    Commit --> Read
    Revert --> Hypothesize
```

### Why Single-Loop Works Here: Signed Physical Error Deltas
If the evaluation tool only output a single scalar loss (e.g., `Loss: 0.412`), the AI would be forced to guess blindly. Instead, `synth-eval` outputs **signed physical deltas** alongside the loss score:
- *Bad feedback*: `harmonic_loss = 0.52`
- *Actionable feedback*: `H2_error_at_p = -9.4 dB (synth 4' octave reed is too quiet at low pressure; lower P_th_4 or increase 4' orifice coupling)`

This allows an LLM agent to reason directly from acoustics and mechanics to code and parameter adjustments in a single step.

---

## 2. Golden Dataset & Train/Validation Split

### 2.1 Target Sample Types
The golden dataset (`assets/samples/golden/`) consists of standard **microphone audio recordings (`.wav`)** of an acoustic bandoneon playing isolated single notes across registers (Low, Mid, High) under two primary articulation classes:
1. **Steady Dynamics (`p`, `mf`, `ff`)**: Sustained notes held at soft, medium, and loud bellows pressure.
2. **Staccato & Marcato Attacks (`staccato`, `marcato`)**: Crisp, accented note onsets capturing pallet valve clicks, initial reed tongue chirp, chamber pressure spike, and release decay.

### 2.2 File Naming & Manifest Convention
Samples are indexed in `assets/samples/golden/manifest.json` with standardized metadata:
```text
assets/samples/golden/
├── manifest.json
├── train/
│   ├── C3_cerrando_p.wav
│   ├── C3_cerrando_mf.wav
│   ├── C3_cerrando_ff.wav
│   ├── C3_cerrando_marcato.wav
│   ├── G3_abriendo_p.wav
│   └── ...
└── val/
    ├── D#3_cerrando_p.wav
    ├── D#3_cerrando_ff.wav
    ├── A4_abriendo_marcato.wav
    └── ...
```

### 2.3 Anti-Overfitting Guardrails
Because the AI agent edits both DSP code and parameter presets in a single loop, we enforce two strict guardrails against "metric hacking" (such as hardcoding per-sample wavetables or overfitting specific WAV files):

1. **Train / Validation Split Across Pitches**:
   - **Train Set (`train/`)**: Used to print detailed per-note, per-harmonic signed error diagnostics in the scorecard.
   - **Validation Set (`val/`)**: Held-out notes at intermediate pitches and dynamics. Only the aggregate validation sub-scores are reported. **A change is only accepted if the Validation Composite Score improves (or stays within $0.5\%$ while Train improves).**
2. **Smooth Physical Scaling Regularization ($L_{\text{reg}}$)**:
   - Physical reed parameters ($f_0$ stiffness, damping $Q$, threshold pressures $P_{\text{th}, 8'}, P_{\text{th}, 4'}$, orifice area $S_{\text{eff}}$, chamber resonance $f_{\text{chamber}}$) **must be expressed as smooth functions of MIDI pitch / reed length** (e.g., power-law or cubic spline across the compass).
   - If per-note trim offsets $\delta_n$ are used, the evaluator adds a second-difference roughness penalty:
     $$L_{\text{reg}} = \lambda_{\text{smooth}} \sum_{n} \left\| \theta_{n+1} - 2\theta_n + \theta_{n-1} \right\|_2^2$$
     This mathematically prevents jagged per-note lookup table hacks.

---

## 3. Envelope Inversion: Driving the Synth from Audio-Only WAVs

Because the golden recordings contain **audio only** (without a physical pressure sensor inside the acoustic bellows), the test harness uses **Envelope Inversion** to reconstruct the deterministic control stream (`Note On` velocity + continuous `CC#11(t)` + `Note Off`) that drives `synth-core`:

```mermaid
flowchart LR
    Wav["Golden WAV\nx_gold(t)"] --> Env["1. Extract Frame Features\nRMS E(t), Spectral Centroid C(t),\nOnset/Offset Times (t_on, t_off)"]
    Env --> Inv["2. Calibrated Inverse Mapping\n(E(t), C(t)) -> CC#11(t) & Vel"]
    Inv --> Synth["3. Drive synth-core\nRender x_synth(t)"]
    Wav --> Compare["4. Acoustic Feature Comparison\nScorecard & Signed Deltas"]
    Synth --> Compare
```

### Step-by-Step Inversion Protocol
1. **Onset & Offset Detection ($t_{\text{on}}, t_{\text{off}}$)**:
   - Detects the exact sample frame where key click / acoustic energy begins ($t_{\text{on}}$) and where the pallet closes ($t_{\text{off}}$).
2. **Loudness & Brightness Trajectory Extraction**:
   - Computes short-time RMS amplitude $A_{\text{gold}}(t)$ (using a $5\text{ ms}$ causal window) and normalized spectral centroid $C_{\text{gold}}(t)$.
3. **Mapping to Control Stream (`CC#11(t)` and `velocity`)**:
   - During calibration initialization, `synth-eval` measures the synth's steady-state monotonic transfer curve $A_{\text{synth}}(\text{CC11})$ for the target pitch.
   - For every control frame ($1\text{ ms}$ step), `synth-eval` inverts the transfer curve:
     $$\text{CC11}(t) = \text{ clamp}\left( A_{\text{synth}}^{-1}\left(A_{\text{gold}}(t)\right),\, 0,\, 127 \right)$$
   - **`Note On` Velocity**: Derived from the initial slope $\left.\frac{d A_{\text{gold}}}{dt}\right|_{t_{\text{on}}}$ and peak amplitude in the first $25\text{ ms}$ after $t_{\text{on}}$ (capturing whether the note was struck as a sharp *marcato* or a soft *p* entry).
4. **Time-Aligned Rendering**:
   - `synth-core` renders $x_{\text{synth}}(t)$ at the exact sample rate of $x_{\text{gold}}(t)$, ensuring that every time slice $t$ compares the synth and the real bandoneon at matched acoustic power.

---

## 4. Hierarchy of Acoustic Fidelity Metrics

The Composite Acoustic Loss $L_{\text{total}}$ is a weighted sum of **5 domain-specific sub-metrics**, ordered by perceptual and physical priority:

$$L_{\text{total}} = 0.35 \, L_{\text{harmonic}} + 0.25 \, L_{\text{transient}} + 0.20 \, L_{\text{dual\_reed}} + 0.10 \, L_{\text{pitch}} + 0.10 \, L_{\text{body}} + L_{\text{reg}}$$

We also report an intuitive **Composite Fidelity Score** from `0.0` to `100.0`:
$$\text{Score} = 100 \cdot \exp\left(-L_{\text{total}}\right)$$

---

### Priority 1: Harmonic Profile & Spectral Balance ($35\%$ Weight)
Measures how accurately the free-reed orifice nonlinearity reproduces the rich harmonic spectrum across `p`, `mf`, and `ff` dynamics.
- **Calculation**:
  - Pitch-synchronous harmonic extraction measures the log-decibel amplitude of the first $K = 32$ harmonics ($H_1 \dots H_{32}$, where $H_1 = f_0/2$ or $f_0$ depending on $8'$ fundamental) normalized to total frame energy:
    $$L_{\text{harmonic}} = \frac{1}{K} \sum_{k=1}^{K} w_k \left| \text{dB}\left(H_k^{\text{synth}}\right) - \text{dB}\left(H_k^{\text{gold}}\right) \right|$$
    with perceptual weighting $w_k$ emphasizing $H_1 \dots H_{16}$ and high-frequency spectral tilt ($1\text{–}6\text{ kHz}$ "bite").
- **Signed Diagnostics for AI**:
  - Per-harmonic dB error vector (`H1_err_db` .. `H16_err_db`) at `p`, `mf`, and `ff`.
  - `spectral_centroid_err_hz`: Positive means synth is too bright/buzzy; negative means synth is too dull/muffled.
  - `odd_to_even_ratio_err_db`: Direct indicator of reed frame symmetry ($y_0$ resting gap offset).

---

### Priority 2: Attack & Release Transients ($25\%$ Weight)
Measures the dynamic responsiveness of reed startup (hopf bifurcation across $P_{\text{th}}$), initial tongue chirp, and pallet valve articulation.
- **Calculation**:
  - **Attack Rise Time ($\Delta t_{10\to 90}$)**: Error in milliseconds for the envelope to rise from $10\%$ to $90\%$ on `staccato` and `marcato` samples.
  - **Multi-Resolution Onset STFT Distance**: High-time-resolution ($2\text{ ms}$, $5\text{ ms}$, $10\text{ ms}$ windows) log-spectral convergence over the first $80\text{ ms}$ of note onset and first $80\text{ ms}$ of note release.
  - **Onset Transient Overshoot Ratio**: Ratio of peak attack energy to steady-state sustain energy in `marcato` articulations.
- **Signed Diagnostics for AI**:
  - `attack_rise_time_err_ms`: Positive means synth reed starts oscillating too slowly (damping too high or piston coupling $S_{\text{piston}}$ too weak).
  - `click_energy_err_db`: Pre-oscillation broadband impulse energy error ($0\text{–}15\text{ ms}$).

---

### Priority 3: Dual-Reed ($8'$ + $4'$) Interaction ($20\%$ Weight)
Captures the hallmark bandoneon pairing of the fundamental ($8'$) and octave ($4'$) reeds sharing a chamber.
- **Calculation**:
  - **Octave Balance Trajectory**: Ratio of $4'$ reed energy (even harmonics $2f_0, 4f_0, \dots$) to $8'$ reed energy (odd harmonics $f_0, 3f_0, 5f_0$) across `p`, `mf`, and `ff`.
  - **Low-Pressure Sequential Activation**: Difference in onset delay $\Delta t_{8'\text{ vs }4'}$ during soft `p` entries (verifying that one reed speaks slightly before the other as pressure crosses $P_{\text{th}, 1}$ then $P_{\text{th}, 2}$).
  - **Octave Beating / Amplitude Modulation**: Envelope modulation depth and beating rate ($\text{Hz}$) at $2f_0$ caused by slight natural detuning and chamber pressure coupling.
- **Signed Diagnostics for AI**:
  - `octave_4ft_to_8ft_err_db` at `p`, `mf`, `ff`.
  - `beat_freq_err_hz`: Difference in slow amplitude modulation rate during sustained notes.
  - `dual_reed_onset_lag_err_ms`: Difference between $8'$ and $4'$ startup delay.

---

### Priority 4: Pressure-Induced Pitch Bend ($10\%$ Weight)
Captures the subtle aerodynamic frequency shift (typically a slight flattening of $3\text{–}15\text{ cents}$ as bellows pressure increases from `p` to `ff` or during a hard `marcato` stab).
- **Calculation**:
  - High-precision autocorrelation / YIN pitch tracker extracts $f_0(t)$ in cents relative to nominal pitch at `p`, `mf`, `ff`, and during `marcato` attacks:
    $$L_{\text{pitch}} = \left| \Delta c_{\text{p}\to\text{ff}}^{\text{synth}} - \Delta c_{\text{p}\to\text{ff}}^{\text{gold}} \right| + \left| \Delta c_{\text{attack}}^{\text{synth}} - \Delta c_{\text{attack}}^{\text{gold}} \right|$$
- **Signed Diagnostics for AI**:
  - `pitch_bend_p_to_ff_err_cents`: Tells the AI how much to adjust the aerodynamic compliance coupling term in the reed ODE.

---

### Priority 5: Wooden Body & Bellows Resonance ($10\%$ Weight)
Measures the broad formant filtering of the wooden *cabezal* and the breathy inter-harmonic air turbulence noise.
- **Calculation**:
  - **Spectral Envelope Formant Distance**: Cepstral / smoothed LPC envelope distance ($100\text{ Hz}\text{–}5\text{ kHz}$), insensitive to individual harmonic peaks, measuring whether the global body resonator matches the woody peaks and notches of the real instrument.
  - **Harmonic-to-Noise Ratio (HNR)**: Compares the broadband inter-harmonic air hiss floor between harmonic peaks.
- **Signed Diagnostics for AI**:
  - `formant_band_err_db`: Signed dB error across 8 perceptual bark/octave bands ($125\text{ Hz}, 250\text{ Hz}, 500\text{ Hz}, 1\text{ kHz}, 2\text{ kHz}, 4\text{ kHz}, \dots$) to directly guide tuning of the body modal biquad bank.
  - `air_noise_floor_err_db`: Positive means too much air turbulence noise; negative means too sterile/clean.

---

## 5. Native CPU Performance Score & Stability Gates

Per architectural decision, **fast native Rust CPU benchmarks** and a continuous **Performance Score (`0.0 .. 100.0`)** evaluate every iteration alongside the acoustic Train and Validation scores:

1. **Quantum Render Benchmark (`48 kHz`, 128-sample AudioWorklet quantum = $2666.7\ \mu\text{s}$ real-time budget)**:
   - `1_voice_us_per_quantum`: Time to render 1 active note (2 coupled reeds + body filter). Target: **$< 30\ \mu\text{s}$** ($\approx 1\%$ real-time).
   - `16_voices_us_per_quantum`: Time to render 16 simultaneous voices (32 coupled reeds + body filter). Hard Gate: **$< 350\ \mu\text{s}$** native ($\approx 13\%$ of one core, leaving comfortable headroom for `wasm32` execution in a browser `AudioWorklet`).
   - `realtime_headroom_factor`: Ratio of real-time quantum window to 16-voice render time ($2666.7 / T_{16\text{v}}$, e.g. $8.1\times$ real-time for `v4`).
   - **`performance_score` (`0.0 .. 100.0`)**: Smoothly maps 16-voice CPU render time $T_{16\text{v}}$ ($\mu\text{s}$) relative to the $350\ \mu\text{s}$ budget (`voice_16_budget_us`) onto a `0..100` scale, with a hard penalty cap ($< 50.0$) if any stability or CPU gate fails (`compute_performance_score` in `crates/synth-eval/src/scorecard.rs`):
     $$\text{PerfScore} = \begin{cases} 100 \cdot \exp\!\left(-0.35 \cdot \dfrac{T_{16\text{v}}}{350.0}\right) & \text{if all gates pass } (T_{16\text{v}} \le 350\ \mu\text{s}) \\[8pt] \min\!\left(49.9,\ 40 \cdot \exp\!\left(-\dfrac{\max(0,\ T_{16\text{v}} - 350.0)}{350.0}\right)\right) & \text{if any gate fails} \end{cases}$$
2. **Numerical Stability Stress Test**:
   - Runs a 2-second torture sequence (rapid `CC#11` step jumps $0 \to 127 \to 0$, velocity 127 stabs, 16-voice polyphonic cluster, silence decay).
   - **Hard Gate**: Zero `NaN`s, zero `Inf`s, peak output bounded within $[-1.0, 1.0]$ (with limiter/saturation), and decay to $< -90\text{ dBFS}$ within $500\text{ ms}$ of `CC#11 = 0`.

---

## 6. Structured Scorecard Schema (`target/eval/scorecard.json`) & 13-Version Comparison

Every run of `cargo run --release -p synth-eval` (or `cargo run --release -p synth-eval -- diagnose` for detailed per-manual, per-dynamic, and per-pitch-region error breakdowns, and `cargo run --release -p synth-eval -- compare` to evaluate all 13 synthesis versions side by side) writes both human/LLM-friendly Markdown summaries (`target/eval/scorecard.md`, `target/eval/version_comparison.md`) and machine-readable JSON files (`target/eval/scorecard.json`, `target/eval/version_comparison.json`).

### 13-Version (`v1`..`v5`) Evaluation Progression
Across all 328 golden Alfred Arnold bandoneon recordings (`219 train`, `109 held-out val`), evaluating all 13 physical modeling versions (`SynthVersion`) demonstrates monotonic progression from `v1` (`59.75` Val) to `v2` (`63.52` Val), across `v3` (`63.84` Val), across `v4` (`63.87` Val), and culminating in the default expressive **`v5` (`V5Vivo`)** physical model (**`65.36` Train / `64.14` Held-Out Validation / `71.67` Perf**, `+0.27` Val over `v4`, `+0.62` Val over `v2`, and `+4.39` Val over `v1`, with zero roughness regularization penalty $L_{\text{reg}} = 0.0000$):

| Version | Physical Model & Investigation Direction | Train (/100) | Held-Out Val (/100) | $\Delta$ Val vs `v2` | Perf (/100) | Harm (35%) | Tran (25%) | Dual $8'+4'$ (20%) | Pitch (10%) | Body (10%) | 16-Voice CPU | Gates |
| :--- | :--- | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| **`v1`** | Baseline Lumped Odd-Clipper + Shared Body EQ | 61.36 | 59.75 | `-3.77` | 87.01 | 55.66 | 60.41 | 53.72 | 82.66 | 66.23 | `139.0 us` | PASS |
| **`v2`** | 1st-Order ADAA + 1-Pole Baffle + Coupled Celda | 65.06 | 63.52 | `0.00` (base) | 79.31 | 63.26 | 61.99 | 56.11 | 82.66 | 68.18 | `232.0 us` | PASS |
| **`v3_aero`** | **v3 Dir 1**: Quadrature Monopole $dU/dt$ Radiation & 2-Pole Dipole Baffle | 65.11 | 63.70 | `+0.18` | 77.78 | 63.39 | 62.66 | 56.08 | 82.66 | 68.25 | `252.6 us` | PASS |
| **`v3_beam`** | **v3 Dir 2**: Zero-Sum Barycentric Slot Softening & Clamped-Bar Modes | 65.08 | 63.57 | `+0.05` | 76.29 | 63.31 | 62.17 | 56.15 | 82.66 | 68.20 | `272.7 us` | PASS |
| **`v3_transient`** | **v3 Dir 3**: Low-Bass *Lastre* Inertia, Hopf Lag & Staccato Chiff | 65.10 | 63.58 | `+0.06` | 79.16 | 63.34 | 61.99 | 56.25 | 82.66 | 68.24 | `234.2 us` | PASS |
| **`v3`** | **v3 Unified**: v3 Dir 1 + Dir 2 + Dir 3 Co-Calibrated | 65.24 | 63.84 | `+0.32` | 73.26 | 63.55 | 62.71 | 56.30 | 82.66 | 68.33 | `312.3 us` | PASS |
| **`v4_cabinet`** | **v4 Dir 1**: Deconvolved 10-Mode Wooden Box & Early-Reflection FIR | 65.25 | 63.85 | `+0.33` | 73.28 | 63.57 | 62.71 | 56.30 | 82.66 | 68.35 | `312.0 us` | PASS |
| **`v4_slot`** | **v4 Dir 2**: 3-Regime Plate-Channel Slot Transit & Vowel Swell ADAA | 65.25 | 63.85 | `+0.33` | 73.25 | 63.57 | 62.71 | 56.30 | 82.66 | 68.34 | `312.5 us` | PASS |
| **`v4_coupled`** | **v4 Dir 3**: 2nd-Order Coupled Reed ODE, Startup Chirp & Celda Feedback | 65.24 | 63.84 | `+0.32` | 73.51 | 63.57 | 62.71 | 56.30 | 82.66 | 68.35 | `309.0 us` | PASS |
| **`v4_celda`** | **v4 Dir 4**: Pitch-Scaled Individual *Celda* Cavity Resonator (*Okada 2025*) | 65.24 | 63.84 | `+0.32` | 73.50 | 63.56 | 62.71 | 56.30 | 82.66 | 68.34 | `309.1 us` | PASS |
| **`v4_zapatilla`** | **v4 Dir 5**: Leather Pallet Thump & Closed-Valve Muffled Ring-Down | 65.24 | 63.84 | `+0.32` | 73.47 | 63.55 | 62.71 | 56.30 | 82.66 | 68.35 | `309.6 us` | PASS |
| **`v4`** | **Unified SOTA v4**: All 5 v4 Directions Co-Calibrated + Fast ADAA/Trig | 65.27 | 63.87 | `+0.35` | 72.60 | 63.61 | 62.72 | 56.30 | 82.66 | 68.39 | `320.9 us` | PASS |
| **`v5` (Default)** | **v5 Vivo Expressive**: Staged $8' \to 4'$ Attack + Sustain Pulsation + Low-Mid Body & Treble Clarity | **65.36** | **64.14** | **`+0.62`** | **71.67** | **63.61** | **62.91** | **56.79** | **82.66** | **68.56** | **`333.0 us`** | **PASS** |

All 13 versions pass every deterministic stability, silence convergence ($< -90\text{ dBFS}$), smoothness ($L_{\text{reg}} = 0.0000$), and native CPU gate ($< 350\ \mu\text{s}$ per 128-sample quantum for 16 active voices / 32 coupled reeds at `48 kHz`).

```json
{
  "preset_name": "Alfred Arnold (AA) Physical Model v5 Vivo (Staged 8'->4' Attack + Sustain Pulsation + Low-Mid Body & Treble Clarity)",
  "gates": {
    "passed_all": true,
    "stability_no_nan_or_inf": true,
    "silence_convergence": true,
    "cpu_budget_passed": true
  },
  "performance": {
    "sample_rate_hz": 48000,
    "quantum_samples": 128,
    "realtime_quantum_us": 2666.7,
    "voice_1_us_per_quantum": 27.6,
    "voice_16_us_per_quantum": 333.0,
    "voice_16_budget_us": 350.0,
    "core_utilization_16v_pct": 12.49,
    "realtime_headroom_factor": 8.01,
    "performance_score": 71.67
  },
  "scores": {
    "composite_train_score": 65.36,
    "composite_val_score": 64.14,
    "performance_score": 71.67,
    "regularization_penalty": 0.0
  }
}
```

