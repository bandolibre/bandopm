use crate::preset::{BandoneonPreset, BodyModeParam};
use core::f32::consts::TAU;

/// Direct Form II Transposed 2nd-order Biquad peaking EQ filter (real-time safe, branchless).
#[derive(Debug, Clone, Copy)]
struct BiquadPeaking {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl BiquadPeaking {
    fn identity() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn configure(&mut self, sample_rate: f32, freq_hz: f32, q: f32, gain_db: f32) {
        let f = freq_hz.clamp(20.0, sample_rate * 0.45);
        let q_val = q.clamp(0.2, 18.0);
        let a = 10.0f32.powf(gain_db.clamp(-24.0, 24.0) / 40.0);
        let w0 = TAU * f / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q_val);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha / a;

        let inv_a0 = 1.0 / a0;
        let new_b0 = b0 * inv_a0;
        if self.z1 != 0.0 || self.z2 != 0.0 {
            let old_d = self.b0 - 1.0;
            let new_d = new_b0 - 1.0;
            let scale = if old_d.abs() > 1e-4 {
                (new_d / old_d).clamp(0.0, 1.0)
            } else {
                0.0
            };
            self.z1 *= scale;
            self.z2 *= scale;
        }
        self.b0 = new_b0;
        self.b1 = b1 * inv_a0;
        self.b2 = b2 * inv_a0;
        self.a1 = a1 * inv_a0;
        self.a2 = a2 * inv_a0;
    }

    #[inline(always)]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    fn reset_state(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// Dual-manual wooden *cabezal* (Left *Bajos* bass box and Right *Cantabile* treble box)
/// and bellows acoustic enclosure filter bank.
///
/// In `v4_cabinet` and `v4`, each manual adds 4 high-Q structural modes (total 10 modes/manual)
/// plus a 32-sample early-reflection internal wood boundary reflection network.
#[derive(Debug, Clone)]
pub struct BodyResonator {
    sample_rate: f32,
    left_stages: [BiquadPeaking; 6],
    right_stages: [BiquadPeaking; 6],
    left_cab_stages: [BiquadPeaking; 4],
    right_cab_stages: [BiquadPeaking; 4],
    use_cabinet: bool,
    ir_buf_l: [f32; 32],
    ir_buf_r: [f32; 32],
    ir_idx: usize,
    ir_lp_l: f32,
    ir_lp_r: f32,
    ir_mix_l: f32,
    ir_mix_r: f32,
    dc_prev_in_l: f32,
    dc_prev_out_l: f32,
    dc_prev_in_r: f32,
    dc_prev_out_r: f32,
}

impl BodyResonator {
    pub fn new(
        sample_rate: f32,
        left_modes: &[BodyModeParam; 6],
        right_modes: &[BodyModeParam; 6],
    ) -> Self {
        let mut body = Self {
            sample_rate: sample_rate.max(8000.0),
            left_stages: [BiquadPeaking::identity(); 6],
            right_stages: [BiquadPeaking::identity(); 6],
            left_cab_stages: [BiquadPeaking::identity(); 4],
            right_cab_stages: [BiquadPeaking::identity(); 4],
            use_cabinet: false,
            ir_buf_l: [0.0; 32],
            ir_buf_r: [0.0; 32],
            ir_idx: 0,
            ir_lp_l: 0.0,
            ir_lp_r: 0.0,
            ir_mix_l: 0.0,
            ir_mix_r: 0.0,
            dc_prev_in_l: 0.0,
            dc_prev_out_l: 0.0,
            dc_prev_in_r: 0.0,
            dc_prev_out_r: 0.0,
        };
        body.update_modes(left_modes, right_modes);
        body
    }

    pub fn new_from_preset(sample_rate: f32, preset: &BandoneonPreset) -> Self {
        let mut body = Self::new(sample_rate, &preset.left_body_modes, &preset.body_modes);
        body.update_from_preset(preset);
        body
    }

    pub fn update_modes(
        &mut self,
        left_modes: &[BodyModeParam; 6],
        right_modes: &[BodyModeParam; 6],
    ) {
        for i in 0..6 {
            let ml = &left_modes[i];
            let mr = &right_modes[i];
            self.left_stages[i].configure(self.sample_rate, ml.freq_hz, ml.q, ml.gain_db);
            self.right_stages[i].configure(self.sample_rate, mr.freq_hz, mr.q, mr.gain_db);
        }
    }

    pub fn update_from_preset(&mut self, preset: &BandoneonPreset) {
        self.update_modes(&preset.left_body_modes, &preset.body_modes);
        self.use_cabinet = preset.version.use_v4_cabinet();
        for i in 0..4 {
            let ml = &preset.left_cabinet_modes[i];
            let mr = &preset.right_cabinet_modes[i];
            let gl = if self.use_cabinet { ml.gain_db } else { 0.0 };
            let gr = if self.use_cabinet { mr.gain_db } else { 0.0 };
            self.left_cab_stages[i].configure(self.sample_rate, ml.freq_hz, ml.q, gl);
            self.right_cab_stages[i].configure(self.sample_rate, mr.freq_hz, mr.q, gr);
        }
        if self.use_cabinet {
            self.ir_mix_l = preset.cabinet_ir_mix_left.clamp(-0.35, 0.35);
            self.ir_mix_r = preset.cabinet_ir_mix_right.clamp(-0.35, 0.35);
        } else {
            self.ir_mix_l = 0.0;
            self.ir_mix_r = 0.0;
        }
    }

    pub fn reset(&mut self) {
        for s in &mut self.left_stages {
            s.reset_state();
        }
        for s in &mut self.right_stages {
            s.reset_state();
        }
        for s in &mut self.left_cab_stages {
            s.reset_state();
        }
        for s in &mut self.right_cab_stages {
            s.reset_state();
        }
        self.ir_buf_l = [0.0; 32];
        self.ir_buf_r = [0.0; 32];
        self.ir_idx = 0;
        self.ir_lp_l = 0.0;
        self.ir_lp_r = 0.0;
        self.dc_prev_in_l = 0.0;
        self.dc_prev_out_l = 0.0;
        self.dc_prev_in_r = 0.0;
        self.dc_prev_out_r = 0.0;
    }

    /// Processes Left (*Bajos*) and Right (*Cantabile*) manual signals through their respective
    /// wooden body resonators, optional `v4_cabinet` deconvolved modal stages + early reflections,
    /// and output DC blockers.
    #[inline(always)]
    pub fn tick_stereo(&mut self, mut l: f32, mut r: f32) -> (f32, f32) {
        // Inject tiny DC anti-denormal offset (+1e-20, -400 dBFS) so recursive biquad states
        // never flush to slow hardware subnormals; removed at the end by the high-pass DC blocker.
        l += 1e-20;
        r += 1e-20;

        if self.use_cabinet {
            let idx = self.ir_idx & 31;
            // Compute internal wooden cabezal early reflections (13, 23, 31 samples for Left Bajos box;
            // 9, 19, 29 samples for Right Cantabile box + metal grill)
            let tap_l = 0.55 * self.ir_buf_l[idx.wrapping_sub(13) & 31]
                - 0.35 * self.ir_buf_l[idx.wrapping_sub(23) & 31]
                + 0.25 * self.ir_buf_l[idx.wrapping_sub(31) & 31];
            let tap_r = 0.55 * self.ir_buf_r[idx.wrapping_sub(9) & 31]
                - 0.35 * self.ir_buf_r[idx.wrapping_sub(19) & 31]
                + 0.25 * self.ir_buf_r[idx.wrapping_sub(29) & 31];
            self.ir_lp_l += 0.38 * (tap_l - self.ir_lp_l);
            self.ir_lp_r += 0.46 * (tap_r - self.ir_lp_r);
            self.ir_buf_l[idx] = l;
            self.ir_buf_r[idx] = r;
            self.ir_idx = (idx + 1) & 31;
            l += self.ir_mix_l * self.ir_lp_l;
            r += self.ir_mix_r * self.ir_lp_r;
        }

        for stage in &mut self.left_stages {
            l = stage.tick(l);
        }
        for stage in &mut self.right_stages {
            r = stage.tick(r);
        }

        if self.use_cabinet {
            for stage in &mut self.left_cab_stages {
                l = stage.tick(l);
            }
            for stage in &mut self.right_cab_stages {
                r = stage.tick(r);
            }
        }

        // High-pass DC blocker (~15 Hz at 48 kHz)
        let r_dc = 0.998;
        let out_l = l - self.dc_prev_in_l + r_dc * self.dc_prev_out_l;
        self.dc_prev_in_l = l;
        self.dc_prev_out_l = if out_l.abs() < 1e-15 { 0.0 } else { out_l };

        let out_r = r - self.dc_prev_in_r + r_dc * self.dc_prev_out_r;
        self.dc_prev_in_r = r;
        self.dc_prev_out_r = if out_r.abs() < 1e-15 { 0.0 } else { out_r };

        (self.dc_prev_out_l, self.dc_prev_out_r)
    }
}

