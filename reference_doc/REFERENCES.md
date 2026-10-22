# Research & Reference Library (`REFERENCES.md`)

This document collects the key hardware documentation, academic papers on free-reed and bandoneon acoustics, existing synthesis projects, and Rust audio engineering resources that inform this synthesizer.

---

## 1. Target Hardware: Bandolibre (Open-Source MIDI Bandoneon)

- **Project Name**: **Bandolibre** (formerly *Bandonéo*), developed by **L'Atelier du Bandonéon Libre**.
- **Official Hub**: [https://bandolibre.github.io](https://bandolibre.github.io)
- **GitHub Repositories**:
  - [`bandolibre/bandolibre`](https://github.com/bandolibre/bandolibre) — Main project hub, documentation, and hardware/firmware links.
  - [`piwicode/bandoneo`](https://github.com/piwicode/bandoneo) — Original open-source hardware/firmware foundation.
- **Hardware & MIDI Characteristics**:
  - **142-Button Layout**: Full *Rheinische Lage* (Argentine tango standard) as well as Peguri and Manoury layouts.
  - **Key Sensors**: Contactless magnetic Hall-effect sensors scanned at high frequency (thousands of Hz), enabling reliable key actuation without mechanical switch bounce.
  - **Bellows Expression Mechanism**: Uses flexible blade springs coupled with magnetic sensors to measure both push (*cerrando*) and pull (*abriendo*) force continuously.
  - **MIDI Implementation**:
    - Outputs standard `Note On` and `Note Off` messages where **velocity** and **release velocity** are dynamically derived from the bellows blade deflection at the instant of note triggering.
    - Outputs continuous **`CC#11` (Expression)** reflecting real-time bellows pressure.
    - Includes a **Table Mode** that sends fixed velocities without requiring bellows movement (for silent practice or score entry).

---

## 2. The "Bandoneon 2.0" Research Initiative (UNQ, Argentina)

The **Bandoneon 2.0** group at the *Universidad Nacional de Quilmes (UNQ)* and *CONICET* (Argentina) has conducted the most directly relevant modern research into measuring real bandoneon acoustics and translating them into real-time digital sound synthesis.

### Key Publications & Findings
1. **Ramos, J. M., Calcagno, E., Vergara, R. O., Rizza, J., & Riera, P. (2022/2023).**
   *"An Electronic Bandoneon with a Dynamic Sound Synthesis System Based on Measured Acoustic Parameters."*
   (Published in *New Interfaces for Musical Expression (NIME)* & *Computer Music Journal*, MIT Press, 47(3), 2023).
   - **Core Contributions**:
     - Built an **Integral Measurement System** to simultaneously record internal bellows air pressure and radiated acoustic sound on an acoustic bandoneon.
     - Demonstrated that each button activates a **pair of reeds tuned an octave apart ($8'$ and $4'$)** which begin oscillating at **distinct minimum pressure thresholds ($P_{\text{th}}$)** depending on reed mass, stiffness, and slot geometry.
     - Characterized the **nonlinear subcritical Hopf hysteresis** between pressure ramp-up (activation threshold $P_{\text{th}}$) and pressure ramp-down (extinction threshold $P_{\text{ext}} \approx 0.72 P_{\text{th}}$).
     - Quantified how the **spectral centroid** (harmonic richness / "bite") and the **$4'$-to-$8'$ octave energy balance** shift nonlinearly with bellows pressure as the primary $8'$ slot saturates while the $4'$ octave reed continues growing.
     - Implemented real-time synthesis models in **Faust DSP** (deployed on Bela Mini hardware for their "Alfa" prototype) combining physical pressure-threshold models with dynamic multi-wavetable interpolation.

---

## 3. Academic Literature: Free-Reed Physical Modeling & Acoustics

Unlike woodwind or brass reeds (which are strongly coupled to a resonant pipe bore that dictates pitch), bandoneon and accordion reeds are **blown-closed free reeds** (*anches libres*) whose pitch is primarily governed by the mechanical resonance of the metal cantilever tongue itself, excited by unsteady airflow through a narrow rectangular slot.

### 3.1 Foundational Aerodynamics & Lumped (1-DOF) Models
1. **Ricot, D., Caussé, R., & Misdariis, N. (2005).**
   *"Aerodynamic excitation and sound production of blown-closed free reeds without acoustic coupling: The example of the accordion reed."*
   **The Journal of the Acoustical Society of America (JASA)**, 117(4), 2279–2290.
   - **Relevance**: The foundational paper for modern accordion/bandoneon physical modeling. Derives the time-domain fluid-structure interaction between a 1-DOF cantilever reed and unsteady Bernoulli orifice flow, including:
     - **Piston flow displaced by the moving reed tongue** ($S_{\text{piston}} \dot{y}$), which breaks phase symmetry and enables self-sustained oscillation without an upstream acoustic pipe resonator.
     - **Unsteady inertial fluid loading** inside the slot channel, which introduces a pressure-dependent phase skew in the reed displacement trajectory $y(t)$.
     - **Ffowcs Williams–Hawkings / Curle dipole aeroacoustic radiation**, showing that inter-harmonic broadband jet turbulence scales with the instantaneous modulated orifice volume flow rather than stationary white noise.

2. **Millot, L., & Baumann, C. (2007).**
   *"A proposal for a minimal model of free reeds."*
   **Acta Acustica united with Acustica**, 93(1), 122–144.
   - **Relevance**: Formulates a computationally tractable "minimal physical model" suitable for real-time numerical simulation of chromatic harmonicas, accordions, and bandoneons.
   - **Key Insight for Harmonic Generation**: Models the **3-regime rectangular slot clearance** as the reed tongue of thickness $e$ traverses a plate slot of thickness $h$. Because the flow area $S_u(y)$ contracts to a minimum inside the slot ($|y| < h/2$) and opens on both the upstream ($y > h/2$) and downstream ($y < -h/2$) sides, the orifice area modulation contains both an **odd-symmetric switching component** (asymmetric equilibrium gap $y_0 > 0$) and an **even-symmetric two-sided slot constriction component** ($\propto \frac{u^2 - c}{1 + u^2}$), which naturally generates the even harmonics ($2f, 4f, 6f, 8f$) of each individual reed.

3. **Cottingham, J. P. (2001–2013).**
   Various papers in *JASA* and *Proceedings of Meetings on Acoustics (POMA)* on *"Acoustics of free-reed instruments"* and *"Reed vibration and airflow in free-reed instruments"*.
   - **Relevance**: Provides empirical laser vibrometry and airflow measurements showing:
     - **Transient Harmonic Bloom**: During note onset, the cantilever tongue begins vibrating in a nearly sinusoidal linear regime while its tip amplitude $a(t)$ is still smaller than the frame plate thickness; over the first $12\text{–}25\text{ ms}$, as the tip clears the slot edges, high harmonics ($H_3 \dots H_{16}$) rapidly bloom ("slot chopping").
     - **Coupled Reed Pairs & Mode Entrainment**: Two reeds sharing a common reed chamber (*celda*) experience mutual aerodynamic forcing through the chamber pressure drop, leading to 1:2 harmonic entrainment alongside slow aeroacoustic amplitude beating.

### 3.2 Distributed Reed Mechanics, Enclosure Radiation & Source-Filter Synthesis
4. **Puranik, N. V., & Scavone, G. P. (2022).**
   *"Physical modelling synthesis of a harmonium."*
   **Proceedings of the International Conference on Digital Audio Effects (DAFx-22)**.
5. **Puranik, N. V., & Scavone, G. P. (2023).**
   *"Clamped bar model for free reeds"* (**Forum Acusticum 2023**) & *"Physically inspired signal model for harmonium sound synthesis"* (**POMA**).
   - **Relevance**: Written at McGill University's Computational Acoustic Modeling Laboratory (CAML). Demonstrates how to combine a physical free-reed source model (both lumped and 4th-order Euler-Bernoulli clamped bar PDEs) with a **bellows chamber reservoir model**, **finite-box baffle diffraction radiation** ($+6\text{ dB/oct}$ below the cabinet diffraction cutoff $f_{\text{baffle}} = c / (\pi L_{\text{box}}) \approx 410\text{–}490\text{ Hz}$), and **viscous boundary-layer jet-transit damping** at high frequencies.

### 3.3 Reed-Box Coupling, Pitch Bending, and Multi-Reed Interaction
6. **Okada, H. (2025).**
   *"Physical Modeling of Pitch Bending in Accordions Involving Resonance Effects of Reed Box."*
   **IEEE Global Conference on Consumer Electronics (GCCE)**.
   - **Relevance**: Models how the small air cavity inside the reed chamber (*celda*) couples with the reed's mechanical compliance to produce pressure-dependent pitch bending ($\Delta c(\Delta P)$). Explains why heavy, tip-weighted low-bass reeds (`C0..B0`) exhibit much stronger flatward pitch drift from *piano* to *forte* ($-9\text{ to }-14\text{ cents}$) than stiffer high-register reeds ($0\text{ to }-2\text{ cents}$).

### 3.4 Antiderivative Anti-Aliasing (ADAA) for Nonlinear Virtual Acoustics
7. **Parker, J. D., Zavalishin, V., & Le Bivic, E. (2016).**
   *"Reducing the Aliasing of Nonlinear Waveshapers Using Continuous-Time Convolution."*
   **Proceedings of the 19th International Conference on Digital Audio Effects (DAFx-16)**, Brno, Czech Republic, 137–144.
8. ** Bilbao, S., Esqueda, F., Parker, J. D., & Välimäki, V. (2017).**
   *"Antiderivative Antialiasing for Memoryless Nonlinearities"* (**IEEE Signal Processing Letters**, 24(7), 1049–1053).
   - **Relevance**: Introduces **First-Order Antiderivative Anti-Aliasing (ADAA)**, which evaluates the continuous-time integral of a sharp orifice waveshaper $f(u)$ over each sample interval $[u_{n-1}, u_n]$ via $\frac{F(u_n) - F(u_{n-1})}{u_n - u_{n-1}}$, suppressing alias foldback at `48 kHz` without requiring $4\times$ or $8\times$ oversampling.
   - **Algebraic Rationalization in `synth-core`**: For the algebraic orifice saturation function $f(u) = \frac{u}{\sqrt{1 + u^2}}$ with antiderivative $F(u) = \sqrt{1 + u^2}$, multiplying numerator and denominator by the conjugate $(\sqrt{1 + u_n^2} + \sqrt{1 + u_{n-1}^2})$ yields the exact identity:
     $$\frac{\sqrt{1 + u_n^2} - \sqrt{1 + u_{n-1}^2}}{u_n - u_{n-1}} \equiv \frac{u_n + u_{n-1}}{\sqrt{1 + u_n^2} + \sqrt{1 + u_{n-1}^2}}$$
     This completely eliminates the $0/0$ numerical cancellation singularity when $u_n \approx u_{n-1}$, requires zero branches, and computes only a single `sqrt` per sample.

---

## 4. Existing Software & Synthesis References

- **Faust Physical Modeling Standard Library (`physmodels.lib`)**:
  - Contains reference primitives for reed tables, aerodynamics, and modal resonators (`pm.modeFilter`).
- **Audio Modeling SWAM (Accordion / Bandoneon)**:
  - Commercial closed-source reference frequently paired with Bandolibre hardware; demonstrates how continuous `CC#11` expression and `Note On` velocity decoupling create convincing articulation.
- **Helmholtz / Chaigne & Kergomard (*Acoustics of Musical Instruments*, Springer, 2016)**:
  - Chapter on Self-Sustained Oscillators and Free-Reed Instruments provides complete dimensional analysis and dimensionless parameter groups for tuning reed stability.

---

## 5. Rust Audio, WebAssembly, and CLAP Ecosystem

### 5.1 Core DSP & Math in Rust
- **`core::simd` / `wide`**: Portable SIMD vectorization (compiles to `wasm32` 128-bit SIMD intrinsics and native `AVX2`/`NEON`), ideal for parallelizing the $8'$ and $4'$ reed pairs or modal biquad banks across voices.
- **`rtrb` / `ringbuf`**: Lock-free Single-Producer Single-Consumer (SPSC) ring buffers for passing MIDI events and parameter changes from the main thread to the real-time audio thread without mutexes or allocations.

### 5.2 Browser Target (`WebAssembly` + `WebAudio` + `WebMIDI`)
- **`wasm-bindgen` & `wasm-pack`**: Tooling to compile `crates/synth-wasm` to `wasm32-unknown-unknown`.
- **WebAudio `AudioWorkletProcessor`**: Runs the compiled WASM module on the browser's dedicated high-priority audio rendering thread in 128-sample blocks (`2.67 ms` at `48 kHz`).
- **WebMIDI API (`navigator.requestMIDIAccess`)**: Provides direct, low-jitter access to USB-MIDI class-compliant devices like Bandolibre in Chromium-based browsers and Firefox.

### 5.3 Native Standalone & DAW Plugin Targets
- **`cpal` (Cross-Platform Audio Library)**: Pure-Rust low-level audio I/O supporting Linux `ALSA`, `PipeWire`, and `JACK` (as well as macOS `CoreAudio` and Windows `WASAPI`/`ASIO`).
- **`midir`**: Cross-platform real-time MIDI input library in Rust.
- **`clack` ([`prokopyl/clack`](https://github.com/prokopyl/clack)) & `nih-plug` ([`robbert-vdh/nih-plug`](https://github.com/robbert-vdh/nih-plug))**:
  - Rust frameworks for building native **CLAP** (*CLever Audio Plugin*) instruments.
  - Note: Because `nih-plug` is currently in maintenance mode, keeping `synth-core` completely decoupled from the plugin wrapper allows us to bind to either `clack` (pure CLAP) or `nih-plug` (CLAP + VST3) with minimal glue code.
