use core::f32::consts::TAU;

/// State of a single blown-closed free reed (either 8' primary or 4' octave reed)
/// using Rationalized Conjugate 1st-Order Antiderivative Anti-Aliasing (ADAA),
/// Millot-Baumann two-sided slot constriction, Cottingham transient harmonic bloom,
/// finite-box baffle diffraction, and 2-pole boundary-layer jet transit filtering.
#[derive(Debug, Clone)]
pub struct FreeReed {
    /// Nominal fundamental frequency in Hz.
    freq_hz: f32,
    /// Instantaneous phase in `[0, 1)`.
    phase: f32,
    /// Dimensionless oscillation amplitude envelope `a(t)` of the cantilever tip.
    osc_amp: f32,
    /// Cottingham transient harmonic bloom envelope in `[0, 1]` (models linear-to-nonlinear
    /// slot wall engagement over the first ~18 ms of reed oscillation).
    harmonic_bloom: f32,
    /// Previous dimensionless slot argument `u[n-1]` for 1st-order ADAA.
    prev_u: f32,
    /// Previous antiderivative `sqrt(1 + u[n-1]^2)` for branchless rationalized ADAA.
    prev_hypot: f32,
    /// One-pole low-pass state for finite-box baffle high-pass diffraction shelf.
    baffle_lp_state: f32,
    /// First-stage low-pass state for boundary-layer jet-transit filter.
    lp_state: f32,
    /// Second-stage low-pass state for boundary-layer jet-transit filter.
    lp_state2: f32,
    /// True once reed has crossed `P_th` (enables subcritical hysteresis down to `P_ext`).
    is_oscillating: bool,
}

/// Instantaneous physical parameters feeding `FreeReed::tick`.
#[derive(Debug, Clone, Copy)]
pub struct ReedTickParams {
    pub sample_rate: f32,
    /// Effective pressure drop across the reed orifice (`0.0 .. 1.5`).
    pub delta_p: f32,
    /// Pallet valve aperture (`0.0 .. 1.0`).
    pub valve_aperture: f32,
    /// Activation threshold pressure `P_th`.
    pub p_th: f32,
    /// Two-sided slot constriction / even-harmonic asymmetry factor.
    pub asymmetry: f32,
    /// Slot edge sharpness `k_slot` at low pressure.
    pub slot_sharpness: f32,
    /// Dynamic brightness boost with pressure.
    pub pressure_brightness_mod: f32,
    /// Finite wooden box baffle high-pass corner frequency (Hz).
    pub baffle_cutoff_hz: f32,
    /// Boundary-layer jet transit 2-pole low-pass corner frequency (Hz).
    pub jet_corner_hz: f32,
    /// Mechanical damping coefficient.
    pub reed_damping: f32,
    /// Aerodynamic piston startup coupling.
    pub piston_coupling: f32,
    /// Aerodynamic pitch bend in cents per unit pressure.
    pub pitch_bend_cents: f32,
    /// Relative tongue inertia startup multiplier (1.0 for 8', ~2.1 for lower-mass 4' reed).
    pub inertia_startup_mult: f32,
}

impl FreeReed {
    pub fn new(freq_hz: f32, initial_phase: f32) -> Self {
        Self {
            freq_hz: freq_hz.clamp(20.0, 12000.0),
            phase: initial_phase.fract(),
            osc_amp: 0.0,
            harmonic_bloom: 0.0,
            prev_u: 0.0,
            prev_hypot: 1.0,
            baffle_lp_state: 0.0,
            lp_state: 0.0,
            lp_state2: 0.0,
            is_oscillating: false,
        }
    }

    pub fn reset(&mut self, freq_hz: f32, initial_phase: f32) {
        self.freq_hz = freq_hz.clamp(20.0, 12000.0);
        self.phase = initial_phase.fract();
        self.osc_amp = 0.0;
        self.harmonic_bloom = 0.0;
        self.prev_u = 0.0;
        self.prev_hypot = 1.0;
        self.baffle_lp_state = 0.0;
        self.lp_state = 0.0;
        self.lp_state2 = 0.0;
        self.is_oscillating = false;
    }

    #[inline]
    pub fn is_silent(&self) -> bool {
        self.osc_amp < 1e-6 && self.lp_state2.abs() < 1e-6
    }

    #[inline]
    pub fn osc_amp(&self) -> f32 {
        self.osc_amp
    }

    /// Advances the free-reed fluid-structure solver by one sample.
    /// Returns `(volume_flow_u, radiated_acoustic_pressure)`.
    #[inline]
    pub fn tick(&mut self, p: &ReedTickParams) -> (f32, f32) {
        let effective_p = p.delta_p * p.valve_aperture;

        // 1. Subcritical Hopf bifurcation threshold with hysteresis:
        //    Reed starts oscillating at P_th and sustains down to P_ext = 0.72 * P_th.
        let p_ext = 0.72 * p.p_th;
        if !self.is_oscillating && effective_p >= p.p_th {
            self.is_oscillating = true;
            if self.osc_amp < 0.004 {
                self.osc_amp = 0.004;
            }
        } else if self.is_oscillating && effective_p < p_ext {
            self.is_oscillating = false;
        }

        // 2. Cantilever limit-cycle amplitude ODE & Cottingham harmonic bloom:
        let dt = 1.0 / p.sample_rate;
        if self.is_oscillating {
            let excess_p = (effective_p - p_ext).max(0.0);
            let target_amp = (excess_p / (1.0 - p_ext).max(0.1)).clamp(0.0, 1.45);
            let base_omega = (self.freq_hz / 180.0).sqrt().clamp(0.65, 2.8);
            let rate = if target_amp > self.osc_amp {
                (135.0 + 500.0 * p.piston_coupling * (0.40 + excess_p))
                    * base_omega
                    * p.inertia_startup_mult
            } else {
                // Free mechanical ring-down when pressure drops (do not multiply by inertia_startup_mult
                // so the 4' reed sustains naturally through staccato releases!)
                (75.0 + 210.0 * p.reed_damping) * base_omega
            };
            let alpha = (rate * dt).clamp(0.0001, 0.35);
            self.osc_amp += alpha * (target_amp - self.osc_amp);

            // Harmonic bloom: faster at higher excess pressure / velocity, gentle at pianissimo
            let bloom_rate = 65.0 + 185.0 * excess_p;
            self.harmonic_bloom += (bloom_rate * dt) * (1.0 - self.harmonic_bloom);
        } else {
            let decay_rate = 55.0 + 340.0 * p.reed_damping;
            let alpha = (decay_rate * dt).clamp(0.0001, 0.35);
            self.osc_amp *= 1.0 - alpha;
            self.harmonic_bloom *= 0.998;
            if self.osc_amp < 1e-7 {
                self.osc_amp = 0.0;
                self.harmonic_bloom = 0.0;
            }
        }

        if self.osc_amp <= 1e-7 {
            self.prev_u = 0.0;
            self.prev_hypot = 1.0;
            self.baffle_lp_state *= 0.95;
            self.lp_state *= 0.95;
            self.lp_state2 *= 0.95;
            if self.lp_state2.abs() < 1e-12 {
                self.baffle_lp_state = 0.0;
                self.lp_state = 0.0;
                self.lp_state2 = 0.0;
            }
            return (0.0, self.lp_state2);
        }

        // 3. Instantaneous pitch with aerodynamic compliance pitch bend
        let bend_cents = p.pitch_bend_cents * effective_p;
        let pitch_ratio = 1.0 + 0.00057762265 * bend_cents;
        let inst_freq = (self.freq_hz * pitch_ratio).clamp(20.0, p.sample_rate * 0.45);

        self.phase += inst_freq * dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        // 4. Clamped-bar flexural displacement with single-trig fluid-inertia skew
        let theta = self.phase * TAU;
        let (s0, c0) = theta.sin_cos();
        let nyquist_guard = (1.0 - (inst_freq / (p.sample_rate * 0.38))).clamp(0.15, 1.0);
        let bloom = (0.18 + 0.82 * self.harmonic_bloom * self.harmonic_bloom).clamp(0.18, 1.0);
        let skew = (0.13 + 0.14 * effective_p) * nyquist_guard * bloom;

        let s2_raw = 2.0 * s0 * c0;
        let c2_raw = c0 * c0 - s0 * s0;
        let s1 = s0 + 0.45 * skew * s2_raw;
        let c1 = c0 - 0.45 * skew * (1.0 - c2_raw);
        let s2 = 2.0 * s1 * c1;
        let y_norm = s1 + 0.14 * nyquist_guard * bloom * s2;

        // 5. Rationalized Conjugate 1st-Order ADAA Orifice Clipper + Narrow Slot Edge Pulse (Odd Harmonics)
        //    Note: For k_slot <= 1.15, odd_adaa is a near-pure fundamental sine wave, while higher
        //    k_slot blooms into nonlinear rectangular slot chopping (`bloom`).
        let k_slot = (1.15
            + (p.slot_sharpness - 1.15 + p.pressure_brightness_mod * effective_p).max(0.0)
                * bloom)
            * nyquist_guard;
        let u_n = k_slot * y_norm;
        let hypot_n = (1.0 + u_n * u_n).sqrt();
        // Exact branchless 1st-order ADAA of u / sqrt(1 + u^2):
        let odd_adaa = (u_n + self.prev_u) / (hypot_n + self.prev_hypot);
        self.prev_u = u_n;
        self.prev_hypot = hypot_n;

        // Narrow vena-contracta slot-wall impulse (excites higher harmonics H5..H16 while keeping H3 moderate)
        let slot_pulse = (u_n / (1.0 + 1.85 * u_n * u_n)) * bloom;
        let odd_wave = 0.08 * s1 + 0.74 * odd_adaa + 0.18 * slot_pulse;

        // 6. Millot-Baumann Two-Sided Slot Constriction (Even Harmonics: 2f, 4f, 6f, 8f of reed)
        let u_even = (0.90 * k_slot) * y_norm;
        let even_constriction = (u_even * u_even - 0.5) / (1.0 + u_even * u_even);
        let even_wave = (0.70 * even_constriction + 0.30 * s2) * bloom;

        let volume_flow = self.osc_amp
            * (odd_wave + p.asymmetry * even_wave - 0.04 * p.piston_coupling * c1);

        // 7. Stage A: Finite wooden box baffle high-pass diffraction shelf (corner `baffle_cutoff_hz`)
        let baffle_alpha = (TAU * p.baffle_cutoff_hz * dt).clamp(0.002, 0.45);
        self.baffle_lp_state += baffle_alpha * (volume_flow - self.baffle_lp_state);
        if self.baffle_lp_state.abs() < 1e-15 {
            self.baffle_lp_state = 0.0;
        }
        let baffle_hp = volume_flow - self.baffle_lp_state;
        let rad_baffle = baffle_hp + 0.15 * self.baffle_lp_state;

        // 8. Stage B: Viscous boundary-layer / jet-transit 2-pole low-pass filter (corner `jet_corner_hz`)
        let dyn_jet_hz = (p.jet_corner_hz * (0.72 + 0.52 * effective_p) * (0.45 + 0.55 * bloom))
            .clamp(400.0, p.sample_rate * 0.44);
        let lp_alpha = (TAU * dyn_jet_hz * dt).clamp(0.03, 0.92);
        self.lp_state += lp_alpha * (rad_baffle - self.lp_state);
        self.lp_state2 += lp_alpha * (self.lp_state - self.lp_state2);
        if self.lp_state2.abs() < 1e-15 {
            self.lp_state = 0.0;
            self.lp_state2 = 0.0;
        }

        (volume_flow, self.lp_state2)
    }
}
