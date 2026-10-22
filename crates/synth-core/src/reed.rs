use crate::preset::{ResolvedReedParams, SynthVersion};
use core::f32::consts::TAU;

/// 100% branchless triangle-folded minimax polynomial approximation of
/// `(sin(2*pi*phase), cos(2*pi*phase))` for `phase in [0.0, 1.0)`.
///
/// Uses the exact identities `uc = |phase - 0.5| - 0.25` and `us = copysign(0.25 - |uc|, 0.5 - phase)`
/// so both sine and cosine reduce to the same odd 7th-degree minimax polynomial on `[-1, 1]`
/// with zero branches, zero integer casts, exact C0/C1 seam continuity, and max error `< 1.8e-6`.
#[inline(always)]
fn fast_sin_cos_unit(phase: f32) -> (f32, f32) {
    let x = phase * 4.0 - 2.0;
    let tc = x.abs() - 1.0;
    let ts = (1.0 - tc.abs()).copysign(-x);
    let ts2 = ts * ts;
    let tc2 = tc * tc;
    let s = ts * (1.570_794_8 + ts2 * (-0.645_921 + ts2 * (0.079_487_66 - ts2 * 0.004_361_53)));
    let c = tc * (1.570_794_8 + tc2 * (-0.645_921 + tc2 * (0.079_487_66 - tc2 * 0.004_361_53)));
    (s, c)
}

#[inline(always)]
pub fn fast_sin_cos(phase: f32) -> (f32, f32) {
    let p = if (0.0..1.0).contains(&phase) {
        phase
    } else {
        phase.rem_euclid(1.0)
    };
    fast_sin_cos_unit(p)
}

/// State of a single blown-closed free reed (either 8' primary or 4' octave reed)
/// supporting multi-version physical synthesis (`v1`..`v4`).
#[derive(Debug, Clone)]
pub struct FreeReed {
    /// Nominal fundamental frequency in Hz.
    freq_hz: f32,
    /// Precomputed mechanical startup rate scale `(freq_hz / 180.0).sqrt().clamp(0.65, 2.8)` for v1/v2.
    base_omega_v2: f32,
    /// Precomputed unclamped low-bass tip-weight (*lastre*) inertia scale for v3/v4.
    base_omega_v3: f32,
    /// Instantaneous phase in `[0, 1)`.
    phase: f32,
    /// Dimensionless oscillation amplitude envelope `a(t)` of the cantilever tip.
    osc_amp: f32,
    /// Second-order mechanical envelope velocity `da/dt` (`v4_coupled` / `v4`).
    osc_vel: f32,
    /// Cottingham transient harmonic bloom envelope in `[0, 1]` (models linear-to-nonlinear
    /// slot wall engagement over the initial cycles of reed oscillation).
    harmonic_bloom: f32,
    /// Previous dimensionless slot argument `u[n-1]` for 1st-order ADAA.
    prev_u: f32,
    /// Previous antiderivative `sqrt(1 + u[n-1]^2)` for branchless rationalized ADAA.
    prev_hypot: f32,
    /// First-stage one-pole low-pass state for finite-box baffle high-pass diffraction shelf.
    baffle_lp_state: f32,
    /// Second-stage one-pole low-pass state for 2nd-order sub-baffle dipole diffraction (`v3_aero` / `v3` / `v4`).
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
            osc_vel: 0.0,
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
        self.phase = initial_phase.rem_euclid(1.0);
        self.osc_amp = 0.0;
        self.osc_vel = 0.0;
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

    #[inline]
    pub fn harmonic_bloom(&self) -> f32 {
        self.harmonic_bloom
    }

    /// Advances the free-reed fluid-structure solver by one sample using pre-resolved `ResolvedReedParams`.
    /// Returns `(volume_flow_u, radiated_acoustic_pressure)`.
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
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
            let base_omega = if version.use_v3_tran() {
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
            let diff_amp = target_amp - self.osc_amp;
            let first_order_step = alpha * diff_amp;
            if self.harmonic_bloom < 0.999 && rp.coupled_ode_momentum != 0.0 {
                let accel = rate * (diff_amp - 1.65 * self.osc_vel);
                self.osc_vel = (self.osc_vel + accel * dt).clamp(-8.0, 8.0);
                self.osc_amp = (self.osc_amp
                    + first_order_step
                    + 0.35 * rp.coupled_ode_momentum * (self.osc_vel * dt - 0.5 * first_order_step))
                    .clamp(0.0, 1.50);
            } else {
                self.osc_amp += first_order_step;
                self.osc_vel = 0.0;
            }

            // Harmonic bloom: faster at higher excess pressure / velocity
            if self.harmonic_bloom < 0.9995 {
                let bloom_rate = 65.0 + 185.0 * excess_p;
                self.harmonic_bloom += (bloom_rate * dt) * (1.0 - self.harmonic_bloom);
            } else {
                self.harmonic_bloom = 1.0;
            }
        } else {
            let decay_rate = 55.0 + 340.0 * rp.reed_damping;
            let alpha = (decay_rate * dt).clamp(0.0001, 0.35);
            self.osc_amp *= 1.0 - alpha;
            self.osc_vel *= 1.0 - alpha;
            self.harmonic_bloom *= 0.998;
            if self.osc_amp < 1e-7 {
                self.osc_amp = 0.0;
                self.osc_vel = 0.0;
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

        // 3. Instantaneous pitch with aerodynamic compliance pitch bend + v4_coupled startup chirp
        let bend_cents = if version.use_v4_coupled() && self.harmonic_bloom < 0.999 {
            rp.pitch_bend_cents * effective_p
                + rp.startup_chirp_cents * (1.0 - self.harmonic_bloom) * (0.35 + 0.65 * effective_p)
        } else {
            rp.pitch_bend_cents * effective_p
        };
        let pitch_ratio = 1.0 + 0.000_577_622_7 * bend_cents;
        let inst_freq = self.freq_hz * pitch_ratio;

        self.phase += inst_freq * dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        let (s0, c0) = fast_sin_cos_unit(self.phase);

        // Baseline v1 path: simple memoryless odd clipper + 1-pole LP (no ADAA, no baffle shelf)
        if version == SynthVersion::V1Baseline {
            let k_v1 = (rp.slot_sharpness + 0.5 * rp.pressure_brightness_mod * effective_p).max(1.0);
            let u = k_v1 * s0;
            let hypot = (1.0 + u * u).sqrt();
            let odd = u / hypot;
            let s2_v1 = 2.0 * s0 * c0;
            let flow = self.osc_amp * (odd + rp.reed_asymmetry * s2_v1);
            let lp_alpha = (TAU * rp.jet_corner_hz * dt).clamp(0.03, 0.92);
            self.lp_state2 += lp_alpha * (flow - self.lp_state2);
            // Keep ADAA and filter states synchronized for click-free live switching from v1 -> v2..v4
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

        // 5. Orifice Nonlinearity & 3-Regime Plate-Channel Slot Transit (single-division reciprocal):
        let k_slot = (1.15
            + (rp.slot_sharpness - 1.15 + rp.pressure_brightness_mod * effective_p).max(0.0)
                * bloom)
            * nyquist_guard;

        let u_n = k_slot * y_norm;
        let u_sq = u_n * u_n;
        let hypot_n = (1.0 + u_sq).sqrt();
        let d1 = hypot_n + self.prev_hypot;
        let d2 = 1.0 + 1.85 * u_sq;
        let u_even_sq = 0.81 * u_sq;
        let d3 = 1.0 + u_even_sq;
        let d12 = d1 * d2;
        let inv_all = 1.0 / (d12 * d3);
        let inv_d1 = d2 * d3 * inv_all;
        let inv_d2 = d1 * d3 * inv_all;
        let inv_d3 = d12 * inv_all;

        let odd_adaa = (u_n + self.prev_u) * inv_d1;
        self.prev_u = u_n;
        self.prev_hypot = hypot_n;

        let slot_pulse = u_n * inv_d2 * bloom;
        let even_constriction = (u_even_sq - 0.5) * inv_d3;

        let (odd_wave, even_wave) = if version.use_v3_beam() || version.use_v4_slot() {
            let guard_h6 = (1.0 - inst_freq * inv_sr_42).clamp(0.0, 1.0);
            let hb4 = hb2 * hb2;
            let ep_slot = hb4 * (0.45 + 0.55 * effective_p);
            let swell = rp.slot_emergence_swell * (0.35 + 0.65 * effective_p) * hb2;
            let c_eff = rp.c_lin - rp.h3_notch_inject * (effective_p - 0.38) - 0.35 * swell;
            let s1_sq = s1 * s1;
            let s3_skew = s1 * (3.0 - 4.0 * s1_sq);
            let h3_eff = rp.h3_notch_inject * (effective_p - 0.32)
                - 1.3333334 * rp.slot_channel_notch * ep_slot
                - rp.slot_pinch_adaa * hb2 * (1.0 - 2.0 * s1_sq);
            let g_bar = (rp.clamped_bar_h6_gain + 0.35 * swell) * guard_h6;
            let s6_raw = s2_raw * (3.0 - 4.0 * s2_raw * s2_raw);

            let odd_w = (0.08 + 0.15 * c_eff) * s1
                + (0.74 + 0.65 * c_eff) * odd_adaa
                + (0.18 - 0.80 * c_eff) * slot_pulse
                + 0.12 * nyquist_guard * bloom * h3_eff * s3_skew;

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
        let tau_dt = TAU * dt;
        let baffle_alpha = tau_dt * rp.baffle_cutoff_hz;
        self.baffle_lp_state += baffle_alpha * (volume_flow - self.baffle_lp_state);
        let baffle_hp1 = volume_flow - self.baffle_lp_state;
        let rad_baffle = if version.use_v3_aero() {
            let dipole_alpha = 0.55 * baffle_alpha;
            self.baffle_lp_state2 += dipole_alpha * (baffle_hp1 - self.baffle_lp_state2);
            baffle_hp1 - rp.dipole_baffle_mix * self.baffle_lp_state2 + 0.15 * self.baffle_lp_state
        } else {
            self.baffle_lp_state2 = 0.0;
            baffle_hp1 + 0.15 * self.baffle_lp_state
        };

        // 8. Stage B: Viscous boundary-layer / jet-transit 2-pole low-pass filter & Monopole dU/dt radiation
        //    Includes branchless leather pallet (*zapatilla*) overhang & closure occlusion muffling
        let occlusion_scale = 1.0
            - rp.valve_occlusion_muffle
                * (1.0 - 0.45 * valve_aperture)
                * (1.0 - 0.65 * self.harmonic_bloom);
        let lp_alpha = (tau_dt
            * rp.jet_corner_hz
            * (0.72 + 0.52 * effective_p)
            * (0.45 + 0.55 * bloom)
            * occlusion_scale)
            .clamp(0.02, 0.92);
        self.lp_state += lp_alpha * (rad_baffle - self.lp_state);
        self.lp_state2 += lp_alpha * (self.lp_state - self.lp_state2);

        let rad_out = if version.use_v3_aero() {
            let jet_monopole = self.lp_state - self.lp_state2;
            self.lp_state2
                + rp.monopole_rad_boost
                    * occlusion_scale
                    * (0.78 + 0.44 * effective_p)
                    * jet_monopole
        } else {
            self.lp_state2
        };

        (volume_flow, rad_out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fast_sin_cos_minimax_accuracy() {
        for i in 0..10_000 {
            let phase = i as f32 / 10_000.0;
            let (s_fast, c_fast) = fast_sin_cos(phase);
            let (s_ref, c_ref) = (TAU * phase).sin_cos();
            assert!(
                (s_fast - s_ref).abs() < 3.0e-6,
                "sin error at phase {phase}: fast={s_fast}, ref={s_ref}"
            );
            assert!(
                (c_fast - c_ref).abs() < 3.0e-6,
                "cos error at phase {phase}: fast={c_fast}, ref={c_ref}"
            );
        }
        // Check exact quadrant boundaries
        let (s0, c0) = fast_sin_cos(0.0);
        assert!(s0.abs() < 1e-6 && (c0 - 1.0).abs() < 2e-6);
        let (s25, c25) = fast_sin_cos(0.25);
        assert!((s25 - 1.0).abs() < 2e-6 && c25.abs() < 1e-6);
        let (s50, c50) = fast_sin_cos(0.5);
        assert!(s50.abs() < 1e-6 && (c50 + 1.0).abs() < 2e-6);
        let (s75, c75) = fast_sin_cos(0.75);
        assert!((s75 + 1.0).abs() < 2e-6 && c75.abs() < 1e-6);

        // Out-of-range phases (negative and >= 1.0) must wrap cleanly into [0, 1)
        for &phase in &[-2.37, -1.75, -0.25, -1e-7, 1.0, 1.25, 2.5, 10.63] {
            let (s_fast, c_fast) = fast_sin_cos(phase);
            let (s_ref, c_ref) = (TAU * phase.rem_euclid(1.0)).sin_cos();
            assert!(
                (s_fast - s_ref).abs() < 3.0e-6 && (c_fast - c_ref).abs() < 3.0e-6,
                "out-of-range phase {phase} failed wrapping: fast=({s_fast}, {c_fast}), ref=({s_ref}, {c_ref})"
            );
        }
    }
}

