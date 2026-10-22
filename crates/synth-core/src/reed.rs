use crate::preset::{ResolvedReedParams, SynthVersion};
use core::f32::consts::TAU;

/// State of a single blown-closed free reed (either 8' primary or 4' octave reed)
/// supporting multi-version physical synthesis (`v1`, `v2`, `v3_aero`, `v3_beam`, `v3_transient`, `v3`).
#[derive(Debug, Clone)]
pub struct FreeReed {
    /// Nominal fundamental frequency in Hz.
    freq_hz: f32,
    /// Precomputed mechanical startup rate scale `(freq_hz / 180.0).sqrt().clamp(0.65, 2.8)` for v1/v2.
    base_omega_v2: f32,
    /// Precomputed unclamped low-bass tip-weight (*lastre*) inertia scale for v3.
    base_omega_v3: f32,
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
    /// First-stage one-pole low-pass state for finite-box baffle high-pass diffraction shelf.
    baffle_lp_state: f32,
    /// Second-stage one-pole low-pass state for 2nd-order sub-baffle dipole diffraction (`v3_aero` / `v3`).
    baffle_lp_state2: f32,
    /// First-stage low-pass state for boundary-layer jet-transit / celda cavity filter.
    lp_state: f32,
    /// Second-stage low-pass state for boundary-layer jet-transit / celda cavity filter.
    lp_state2: f32,
    /// True once reed has crossed `P_th` (enables subcritical hysteresis down to `P_ext`).
    is_oscillating: bool,
}

impl FreeReed {
    pub fn new(freq_hz: f32, initial_phase: f32) -> Self {
        let mut reed = Self {
            freq_hz: 261.63,
            base_omega_v2: 1.0,
            base_omega_v3: 1.0,
            phase: 0.0,
            osc_amp: 0.0,
            harmonic_bloom: 0.0,
            prev_u: 0.0,
            prev_hypot: 1.0,
            baffle_lp_state: 0.0,
            baffle_lp_state2: 0.0,
            lp_state: 0.0,
            lp_state2: 0.0,
            is_oscillating: false,
        };
        reed.reset(freq_hz, initial_phase);
        reed
    }

    pub fn reset(&mut self, freq_hz: f32, initial_phase: f32) {
        let f = freq_hz.clamp(20.0, 12000.0);
        self.freq_hz = f;
        let sqrt_ratio = (f / 180.0).sqrt();
        self.base_omega_v2 = sqrt_ratio.clamp(0.65, 2.8);
        // Heavy tip-weighted (*lastre*) low-bass reeds (< 118 Hz, C0..A#0) have larger tip inertia
        self.base_omega_v3 = if f < 118.0 {
            sqrt_ratio * (0.76 + 0.24 * (f / 118.0))
        } else {
            sqrt_ratio.clamp(0.65, 2.8)
        };
        self.phase = initial_phase.fract();
        self.osc_amp = 0.0;
        self.harmonic_bloom = 0.0;
        self.prev_u = 0.0;
        self.prev_hypot = 1.0;
        self.baffle_lp_state = 0.0;
        self.baffle_lp_state2 = 0.0;
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

    /// Advances the free-reed fluid-structure solver by one sample using pre-resolved `ResolvedReedParams`.
    /// Returns `(volume_flow_u, radiated_acoustic_pressure)`.
    #[inline(always)]
    pub fn tick(
        &mut self,
        version: SynthVersion,
        _sample_rate: f32,
        dt: f32,
        inv_sr_38: f32,
        inv_sr_42: f32,
        delta_p: f32,
        valve_aperture: f32,
        rp: &ResolvedReedParams,
    ) -> (f32, f32) {
        let effective_p = delta_p * valve_aperture;

        let is_v1 = version == SynthVersion::V1Baseline;
        let use_aero = matches!(
            version,
            SynthVersion::V3AeroRadiation | SynthVersion::V3Unified
        );
        let use_beam = matches!(
            version,
            SynthVersion::V3ClampedBar | SynthVersion::V3Unified
        );
        let use_tran = matches!(
            version,
            SynthVersion::V3TransientEntrain | SynthVersion::V3Unified
        );

        // 1. Subcritical Hopf bifurcation threshold with hysteresis:
        //    Reed starts oscillating at P_th and sustains down to P_ext = 0.72 * P_th.
        if !self.is_oscillating && effective_p >= rp.p_th {
            self.is_oscillating = true;
            if self.osc_amp < 0.004 {
                self.osc_amp = 0.004;
            }
        } else if self.is_oscillating && effective_p < rp.p_ext {
            self.is_oscillating = false;
        }

        // 2. Cantilever limit-cycle amplitude ODE & Cottingham harmonic bloom:
        if self.is_oscillating {
            let excess_p = (effective_p - rp.p_ext).max(0.0);
            let target_amp = (excess_p * rp.inv_one_minus_p_ext).clamp(0.0, 1.45);
            let base_omega = if use_tran {
                self.base_omega_v3
            } else {
                self.base_omega_v2
            };
            let rate = if target_amp > self.osc_amp {
                (135.0 + 500.0 * rp.piston_coupling * (0.40 + excess_p))
                    * base_omega
                    * rp.inertia_mult
            } else {
                // Free mechanical ring-down when pressure drops
                (75.0 + 210.0 * rp.reed_damping) * base_omega
            };
            let alpha = (rate * dt).clamp(0.0001, 0.35);
            self.osc_amp += alpha * (target_amp - self.osc_amp);

            // Harmonic bloom: faster at higher excess pressure / velocity, gentle at pianissimo
            let bloom_rate = 65.0 + 185.0 * excess_p;
            self.harmonic_bloom += (bloom_rate * dt) * (1.0 - self.harmonic_bloom);
        } else {
            let decay_rate = 55.0 + 340.0 * rp.reed_damping;
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
            self.baffle_lp_state2 *= 0.95;
            self.lp_state *= 0.95;
            self.lp_state2 *= 0.95;
            if self.lp_state2.abs() < 1e-12 {
                self.baffle_lp_state = 0.0;
                self.baffle_lp_state2 = 0.0;
                self.lp_state = 0.0;
                self.lp_state2 = 0.0;
            }
            return (0.0, self.lp_state2);
        }

        // 3. Instantaneous pitch with aerodynamic compliance pitch bend
        let bend_cents = rp.pitch_bend_cents * effective_p;
        let pitch_ratio = 1.0 + 0.00057762265 * bend_cents;
        let inst_freq = self.freq_hz * pitch_ratio;

        self.phase += inst_freq * dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        let (s0, c0) = (self.phase * TAU).sin_cos();

        // Baseline v1 path: simple memoryless odd clipper + 1-pole LP (no ADAA, no baffle shelf)
        if is_v1 {
            let k_v1 = (rp.slot_sharpness + 0.5 * rp.pressure_brightness_mod * effective_p).max(1.0);
            let u = k_v1 * s0;
            let hypot = (1.0 + u * u).sqrt();
            let odd = u / hypot;
            let s2_v1 = 2.0 * s0 * c0;
            let flow = self.osc_amp * (odd + rp.reed_asymmetry * s2_v1);
            let lp_alpha = (TAU * rp.jet_corner_hz * dt).clamp(0.03, 0.92);
            self.lp_state2 += lp_alpha * (flow - self.lp_state2);
            // Keep ADAA and filter states synchronized for click-free live switching from v1 -> v2/v3
            self.prev_u = u;
            self.prev_hypot = hypot;
            self.baffle_lp_state = flow;
            self.baffle_lp_state2 = 0.0;
            self.lp_state = self.lp_state2;
            return (flow, self.lp_state2);
        }

        // 4. Clamped-bar flexural displacement with single-trig fluid-inertia skew
        let nyquist_guard = (1.0 - inst_freq * inv_sr_38).clamp(0.15, 1.0);
        let hb2 = self.harmonic_bloom * self.harmonic_bloom;
        let bloom = 0.18 + 0.82 * hb2;
        let skew = (0.13 + 0.14 * effective_p) * nyquist_guard * bloom;

        let s2_raw = 2.0 * s0 * c0;
        let c2_raw = c0 * c0 - s0 * s0;
        let s1 = s0 + 0.45 * skew * s2_raw;
        let c1 = c0 - 0.45 * skew * (1.0 - c2_raw);
        let s2 = 2.0 * s1 * c1;
        let y_norm = s1 + 0.14 * nyquist_guard * bloom * s2;

        // 5. Orifice Nonlinearity:
        let k_slot = (1.15
            + (rp.slot_sharpness - 1.15 + rp.pressure_brightness_mod * effective_p).max(0.0)
                * bloom)
            * nyquist_guard;

        let u_n = k_slot * y_norm;
        let hypot_n = (1.0 + u_n * u_n).sqrt();
        let odd_adaa = (u_n + self.prev_u) / (hypot_n + self.prev_hypot);
        self.prev_u = u_n;
        self.prev_hypot = hypot_n;

        let slot_pulse = (u_n / (1.0 + 1.85 * u_n * u_n)) * bloom;
        let u_even = (0.90 * k_slot) * y_norm;
        let even_constriction = (u_even * u_even - 0.5) / (1.0 + u_even * u_even);

        let (odd_wave, even_wave) = if use_beam {
            // Direction 2: Zero-sum barycentric Poiseuille-Bernoulli boundary softening (`c_lin`),
            // pressure-driven blown-closed duty sharpening (`h3_notch_inject`), and 2nd flexural
            // Euler-Bernoulli clamped-bar mode (~6f0, `clamped_bar_h6_gain`).
            let guard_h6 = (1.0 - inst_freq * inv_sr_42).clamp(0.0, 1.0);
            let c_eff = rp.c_lin - rp.h3_notch_inject * (effective_p - 0.38);
            let s3_skew = s1 * (3.0 - 4.0 * s1 * s1);
            let odd_w = (0.08 + 0.15 * c_eff) * s1
                + (0.74 + 0.65 * c_eff) * odd_adaa
                + (0.18 - 0.80 * c_eff) * slot_pulse
                + 0.12 * rp.h3_notch_inject * nyquist_guard * bloom * (effective_p - 0.32) * s3_skew;

            let g_bar = rp.clamped_bar_h6_gain * guard_h6;
            let s6_raw = s2_raw * (3.0 - 4.0 * s2_raw * s2_raw);
            let even_w = ((0.70 + 0.65 * g_bar) * even_constriction
                + (0.30 - 0.85 * g_bar) * s2
                + 0.20 * g_bar * s6_raw)
                * bloom;
            (odd_w, even_w)
        } else {
            (
                0.08 * s1 + 0.74 * odd_adaa + 0.18 * slot_pulse,
                (0.70 * even_constriction + 0.30 * s2) * bloom,
            )
        };

        let volume_flow =
            self.osc_amp * (odd_wave + rp.reed_asymmetry * even_wave - 0.04 * rp.piston_coupling * c1);

        // 7. Stage A: Finite wooden box baffle high-pass diffraction shelf (corner `baffle_cutoff_hz`)
        let baffle_alpha = TAU * rp.baffle_cutoff_hz * dt;
        self.baffle_lp_state += baffle_alpha * (volume_flow - self.baffle_lp_state);
        let baffle_hp1 = volume_flow - self.baffle_lp_state;

        let rad_baffle = if use_aero && rp.dipole_baffle_mix.abs() > 1e-5 {
            // 2nd-order sub-baffle dipole diffraction high-pass below cabinet cutoff
            let dipole_alpha = 0.55 * baffle_alpha;
            self.baffle_lp_state2 += dipole_alpha * (baffle_hp1 - self.baffle_lp_state2);
            baffle_hp1 - rp.dipole_baffle_mix * self.baffle_lp_state2 + 0.15 * self.baffle_lp_state
        } else {
            baffle_hp1 + 0.15 * self.baffle_lp_state
        };

        // 8. Stage B: Viscous boundary-layer / jet-transit 2-pole low-pass filter & Monopole dU/dt radiation
        let lp_alpha = (TAU * dt * rp.jet_corner_hz * (0.72 + 0.52 * effective_p) * (0.45 + 0.55 * bloom))
            .clamp(0.03, 0.92);
        self.lp_state += lp_alpha * (rad_baffle - self.lp_state);
        self.lp_state2 += lp_alpha * (self.lp_state - self.lp_state2);

        let rad_out = if use_aero && rp.monopole_rad_boost.abs() > 1e-5 {
            // Quadrature band-limited monopole time-derivative dU/dt (orthogonal to lp_state2)
            let jet_monopole = self.lp_state - self.lp_state2;
            self.lp_state2 + rp.monopole_rad_boost * (0.78 + 0.44 * effective_p) * jet_monopole
        } else {
            self.lp_state2
        };

        (volume_flow, rad_out)
    }
}
