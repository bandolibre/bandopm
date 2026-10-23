//! Real-time Studio Post-Processing Chain (`PostProcessor`) for **BandoPM**:
//! 1. **3D Binaural / Haas Spatializer & Mid-Side Enhancer**:
//!    - Frequency-dependent interaural time delay (`~0.44 ms`) with `1.7 kHz` head-shadow low-pass
//!      crossfeed between Left (*Bajos*) and Right (*Cantabile*) manuals, sub-155 Hz bass centering,
//!      and upper-harmonic stereo presence expansion.
//! 2. **Program-Dependent Optical / Warm Bus Compressor**:
//!    - Stereo-linked detector with transient-preserving `16 ms` attack (lets the *zapatilla* click
//!      and *marcato* reed pluck punch through) and `165 ms` release, soft-knee gain reduction,
//!      automatic makeup gain, and subtle even/odd harmonic saturation.
//! 3. **Wooden Salon & Concert Hall Stereo Reverberator**:
//!    - Pre-delay + 4-tap wooden room early reflection cluster + 4x2 mutually prime damped
//!      parallel comb filters + 2x2 series Schroeder allpass diffusers with `145 Hz` low-cut
//!      and `4.0 kHz` high-damping so bright free-reed harmonics stay warm and woody.

const HAAS_BUF_LEN: usize = 64;
const PREDELAY_BUF_LEN: usize = 2048;
const COMB_BUF_LEN: usize = 2048;
const ALLPASS_BUF_LEN: usize = 512;

const COMB_LENGTHS_L: [usize; 4] = [1367, 1543, 1723, 1907];
const COMB_LENGTHS_R: [usize; 4] = [1409, 1597, 1777, 1951];
const ALLPASS_LENGTHS_L: [usize; 2] = [241, 379];
const ALLPASS_LENGTHS_R: [usize; 2] = [263, 401];

#[derive(Debug, Clone)]
struct DampedComb {
    buf: [f32; COMB_BUF_LEN],
    len: usize,
    idx: usize,
    filter_store: f32,
}

impl DampedComb {
    fn new(base_len: usize, sr_scale: f32) -> Self {
        let scaled = ((base_len as f32) * sr_scale).round() as usize;
        Self {
            buf: [0.0; COMB_BUF_LEN],
            len: scaled.clamp(128, COMB_BUF_LEN - 1),
            idx: 0,
            filter_store: 0.0,
        }
    }

    #[inline]
    fn reset(&mut self) {
        self.buf.fill(0.0);
        self.idx = 0;
        self.filter_store = 0.0;
    }

    #[inline]
    fn tick(&mut self, input: f32, feedback: f32, damp: f32) -> f32 {
        let output = self.buf[self.idx];
        self.filter_store = output * (1.0 - damp) + self.filter_store * damp + 1e-20 - 1e-20;
        self.buf[self.idx] = input + self.filter_store * feedback;
        self.idx += 1;
        if self.idx >= self.len {
            self.idx = 0;
        }
        output
    }
}

#[derive(Debug, Clone)]
struct AllPassDiffuser {
    buf: [f32; ALLPASS_BUF_LEN],
    len: usize,
    idx: usize,
}

impl AllPassDiffuser {
    fn new(base_len: usize, sr_scale: f32) -> Self {
        let scaled = ((base_len as f32) * sr_scale).round() as usize;
        Self {
            buf: [0.0; ALLPASS_BUF_LEN],
            len: scaled.clamp(32, ALLPASS_BUF_LEN - 1),
            idx: 0,
        }
    }

    #[inline]
    fn reset(&mut self) {
        self.buf.fill(0.0);
        self.idx = 0;
    }

    #[inline]
    fn tick(&mut self, input: f32) -> f32 {
        let delayed = self.buf[self.idx];
        let g = 0.5f32;
        let out = -input + delayed;
        self.buf[self.idx] = input + delayed * g + 1e-20 - 1e-20;
        self.idx += 1;
        if self.idx >= self.len {
            self.idx = 0;
        }
        out
    }
}

/// Real-time stereo post-processing chain (Spatializer + Optical Bus Compressor + Salon/Hall Reverb).
#[derive(Debug, Clone)]
pub struct PostProcessor {
    pub enabled: bool,
    pub comp_amount: f32,
    pub reverb_mix: f32,
    pub reverb_size: f32,
    pub spatial_amount: f32,

    // Smoothed internal controls (prevents zipper clicks)
    enable_smooth: f32,
    comp_smooth: f32,
    rev_mix_smooth: f32,
    rev_size_smooth: f32,
    spatial_smooth: f32,
    param_alpha: f32,

    // Stage 1: Binaural Haas Crossfeed & Sub-Bass Crossover
    haas_l: [f32; HAAS_BUF_LEN],
    haas_r: [f32; HAAS_BUF_LEN],
    haas_idx: usize,
    haas_delay_samples: usize,
    head_shadow_l: f32,
    head_shadow_r: f32,
    head_shadow_alpha: f32,
    sub_low_l: f32,
    sub_low_r: f32,
    sub_alpha: f32,

    // Stage 2: Optical Stereo-Linked Compressor
    comp_env: f32,
    comp_attack_coeff: f32,
    comp_release_coeff: f32,
    last_gain_reduction_db: f32,

    // Stage 3: Pre-delay, Early Reflections & Stereo FDN/Schroeder Reverb
    predelay_l: [f32; PREDELAY_BUF_LEN],
    predelay_r: [f32; PREDELAY_BUF_LEN],
    predelay_idx: usize,
    er_taps_l: [usize; 4],
    er_taps_r: [usize; 4],
    rev_hp_l: f32,
    rev_hp_r: f32,
    rev_hp_alpha: f32,
    rev_lp_l: f32,
    rev_lp_r: f32,
    rev_lp_alpha: f32,
    combs_l: [DampedComb; 4],
    combs_r: [DampedComb; 4],
    allpass_l: [AllPassDiffuser; 2],
    allpass_r: [AllPassDiffuser; 2],
}

impl PostProcessor {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        let sr_scale = sr / 48000.0;
        let param_alpha = (-1.0 / (0.008 * sr)).exp();

        // ~0.44 ms interaural time difference (about 15 cm acoustic path delta)
        let haas_delay_samples = ((0.00044 * sr).round() as usize).clamp(4, HAAS_BUF_LEN - 2);
        let head_shadow_alpha = (-core::f32::consts::TAU * 1750.0 / sr).exp();
        let sub_alpha = (-core::f32::consts::TAU * 155.0 / sr).exp();

        // 16 ms attack (preserves marcato & pallet click transients), 165 ms release
        let comp_attack_coeff = (-1.0 / (0.016 * sr)).exp();
        let comp_release_coeff = (-1.0 / (0.165 * sr)).exp();

        // Early reflection taps (~7.3 ms, 13.9 ms, 21.4 ms, 31.8 ms)
        let er_taps_l = [
            ((350.0 * sr_scale) as usize).clamp(16, PREDELAY_BUF_LEN - 2),
            ((667.0 * sr_scale) as usize).clamp(32, PREDELAY_BUF_LEN - 2),
            ((1027.0 * sr_scale) as usize).clamp(64, PREDELAY_BUF_LEN - 2),
            ((1526.0 * sr_scale) as usize).clamp(96, PREDELAY_BUF_LEN - 2),
        ];
        let er_taps_r = [
            ((389.0 * sr_scale) as usize).clamp(16, PREDELAY_BUF_LEN - 2),
            ((719.0 * sr_scale) as usize).clamp(32, PREDELAY_BUF_LEN - 2),
            ((1091.0 * sr_scale) as usize).clamp(64, PREDELAY_BUF_LEN - 2),
            ((1597.0 * sr_scale) as usize).clamp(96, PREDELAY_BUF_LEN - 2),
        ];

        let rev_hp_alpha = (-core::f32::consts::TAU * 145.0 / sr).exp();
        let rev_lp_alpha = (-core::f32::consts::TAU * 3900.0 / sr).exp();

        let combs_l = core::array::from_fn(|i| DampedComb::new(COMB_LENGTHS_L[i], sr_scale));
        let combs_r = core::array::from_fn(|i| DampedComb::new(COMB_LENGTHS_R[i], sr_scale));
        let allpass_l =
            core::array::from_fn(|i| AllPassDiffuser::new(ALLPASS_LENGTHS_L[i], sr_scale));
        let allpass_r =
            core::array::from_fn(|i| AllPassDiffuser::new(ALLPASS_LENGTHS_R[i], sr_scale));

        Self {
            enabled: false,
            comp_amount: 0.45,
            reverb_mix: 0.26,
            reverb_size: 0.50,
            spatial_amount: 0.55,
            enable_smooth: 0.0,
            comp_smooth: 0.45,
            rev_mix_smooth: 0.26,
            rev_size_smooth: 0.50,
            spatial_smooth: 0.55,
            param_alpha,
            haas_l: [0.0; HAAS_BUF_LEN],
            haas_r: [0.0; HAAS_BUF_LEN],
            haas_idx: 0,
            haas_delay_samples,
            head_shadow_l: 0.0,
            head_shadow_r: 0.0,
            head_shadow_alpha,
            sub_low_l: 0.0,
            sub_low_r: 0.0,
            sub_alpha,
            comp_env: 0.0,
            comp_attack_coeff,
            comp_release_coeff,
            last_gain_reduction_db: 0.0,
            predelay_l: [0.0; PREDELAY_BUF_LEN],
            predelay_r: [0.0; PREDELAY_BUF_LEN],
            predelay_idx: 0,
            er_taps_l,
            er_taps_r,
            rev_hp_l: 0.0,
            rev_hp_r: 0.0,
            rev_hp_alpha,
            rev_lp_l: 0.0,
            rev_lp_r: 0.0,
            rev_lp_alpha,
            combs_l,
            combs_r,
            allpass_l,
            allpass_r,
        }
    }

    pub fn reset(&mut self) {
        self.haas_l.fill(0.0);
        self.haas_r.fill(0.0);
        self.haas_idx = 0;
        self.head_shadow_l = 0.0;
        self.head_shadow_r = 0.0;
        self.sub_low_l = 0.0;
        self.sub_low_r = 0.0;
        self.comp_env = 0.0;
        self.last_gain_reduction_db = 0.0;
        self.predelay_l.fill(0.0);
        self.predelay_r.fill(0.0);
        self.predelay_idx = 0;
        self.rev_hp_l = 0.0;
        self.rev_hp_r = 0.0;
        self.rev_lp_l = 0.0;
        self.rev_lp_r = 0.0;
        for c in &mut self.combs_l {
            c.reset();
        }
        for c in &mut self.combs_r {
            c.reset();
        }
        for a in &mut self.allpass_l {
            a.reset();
        }
        for a in &mut self.allpass_r {
            a.reset();
        }
    }

    #[inline]
    pub fn last_gain_reduction_db(&self) -> f32 {
        self.last_gain_reduction_db
    }

    /// Processes a single stereo sample frame through Spatializer -> Bus Compressor -> Salon Reverb.
    #[inline]
    pub fn tick_stereo(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        let target_en = if self.enabled { 1.0 } else { 0.0 };
        self.enable_smooth =
            self.param_alpha * self.enable_smooth + (1.0 - self.param_alpha) * target_en;

        if self.enable_smooth < 1e-4 {
            self.last_gain_reduction_db = 0.0;
            return (in_l, in_r);
        }

        self.comp_smooth = self.param_alpha * self.comp_smooth
            + (1.0 - self.param_alpha) * self.comp_amount.clamp(0.0, 1.0);
        self.rev_mix_smooth = self.param_alpha * self.rev_mix_smooth
            + (1.0 - self.param_alpha) * self.reverb_mix.clamp(0.0, 1.0);
        self.rev_size_smooth = self.param_alpha * self.rev_size_smooth
            + (1.0 - self.param_alpha) * self.reverb_size.clamp(0.0, 1.0);
        self.spatial_smooth = self.param_alpha * self.spatial_smooth
            + (1.0 - self.param_alpha) * self.spatial_amount.clamp(0.0, 1.0);

        // =====================================================================
        // 1. Binaural Haas Crossfeed + Sub-155Hz Centering + Mid/Side Presence
        // =====================================================================
        let sp = self.spatial_smooth;
        self.haas_l[self.haas_idx] = in_l;
        self.haas_r[self.haas_idx] = in_r;
        let read_haas = (self.haas_idx + HAAS_BUF_LEN - self.haas_delay_samples) & (HAAS_BUF_LEN - 1);
        self.haas_idx = (self.haas_idx + 1) & (HAAS_BUF_LEN - 1);

        let delayed_l = self.haas_l[read_haas];
        let delayed_r = self.haas_r[read_haas];

        // 1.75 kHz head-shadow low-pass filter on the contralateral delayed signal
        self.head_shadow_l = self.head_shadow_alpha * self.head_shadow_l
            + (1.0 - self.head_shadow_alpha) * delayed_l
            + 1e-20
            - 1e-20;
        self.head_shadow_r = self.head_shadow_alpha * self.head_shadow_r
            + (1.0 - self.head_shadow_alpha) * delayed_r
            + 1e-20
            - 1e-20;

        // Separate sub-155 Hz bass vs upper register
        self.sub_low_l =
            self.sub_alpha * self.sub_low_l + (1.0 - self.sub_alpha) * in_l + 1e-20 - 1e-20;
        self.sub_low_r =
            self.sub_alpha * self.sub_low_r + (1.0 - self.sub_alpha) * in_r + 1e-20 - 1e-20;
        let high_l = in_l - self.sub_low_l;
        let high_r = in_r - self.sub_low_r;

        // Sub-bass is gently anchored toward center; upper frequencies get Mid/Side expansion + Haas crossfeed
        let sub_mono = 0.5 * (self.sub_low_l + self.sub_low_r);
        let bass_l = self.sub_low_l * (1.0 - 0.35 * sp) + sub_mono * (0.35 * sp);
        let bass_r = self.sub_low_r * (1.0 - 0.35 * sp) + sub_mono * (0.35 * sp);

        let mid_hi = 0.5 * (high_l + high_r);
        let side_hi = 0.5 * (high_l - high_r) * (1.0 + 0.42 * sp);
        let crossfeed_gain = 0.22 * sp;

        let spat_l = bass_l + (mid_hi + side_hi) + crossfeed_gain * self.head_shadow_r;
        let spat_r = bass_r + (mid_hi - side_hi) + crossfeed_gain * self.head_shadow_l;

        // =====================================================================
        // 2. Program-Dependent Optical Bus Compressor + Warm Harmonic Glue
        // =====================================================================
        let comp = self.comp_smooth;
        let peak = spat_l.abs().max(spat_r.abs());
        let env_coeff = if peak > self.comp_env {
            self.comp_attack_coeff
        } else {
            self.comp_release_coeff
        };
        self.comp_env = env_coeff * self.comp_env + (1.0 - env_coeff) * peak + 1e-20 - 1e-20;

        let (comp_l, comp_r) = if comp > 0.005 {
            // Threshold moves from 0.42 (-7.5 dB) down to 0.11 (-19 dB) with comp_amount
            let threshold = 0.42 - 0.31 * comp;
            let slope = 0.25 + 0.50 * comp; // Ratio from 1.33:1 up to 4.0:1
            let gain_lin = if self.comp_env > threshold {
                let ratio_over = self.comp_env / threshold.max(1e-4);
                ratio_over.powf(-slope)
            } else {
                1.0
            };
            // Track gain reduction in dB for UI metering
            self.last_gain_reduction_db = -20.0 * gain_lin.max(0.05).log10();

            // Automatic makeup gain + subtle tube/tape even/odd harmonic warmth
            let makeup = 1.0 + 0.68 * comp;
            let g = gain_lin * makeup;
            let wl = spat_l * g;
            let wr = spat_r * g;
            let drive = 1.0 + 0.35 * comp;
            let warm_l = (wl * drive + 0.045 * comp * wl * wl) / (1.0 + 0.35 * comp * wl.abs());
            let warm_r = (wr * drive + 0.045 * comp * wr * wr) / (1.0 + 0.35 * comp * wr.abs());
            (warm_l, warm_r)
        } else {
            self.last_gain_reduction_db = 0.0;
            (spat_l, spat_r)
        };

        // =====================================================================
        // 3. Tango Salon / Concert Hall Stereo Reverberator
        // =====================================================================
        let rev_mix = self.rev_mix_smooth;
        let (final_l, final_r) = if rev_mix > 0.002 {
            // Input conditioning: 145 Hz high-pass + 3.9 kHz low-pass
            self.rev_hp_l = self.rev_hp_alpha * self.rev_hp_l
                + (1.0 - self.rev_hp_alpha) * comp_l
                + 1e-20
                - 1e-20;
            self.rev_hp_r = self.rev_hp_alpha * self.rev_hp_r
                + (1.0 - self.rev_hp_alpha) * comp_r
                + 1e-20
                - 1e-20;
            let hp_l = comp_l - self.rev_hp_l;
            let hp_r = comp_r - self.rev_hp_r;

            self.rev_lp_l = self.rev_lp_alpha * self.rev_lp_l
                + (1.0 - self.rev_lp_alpha) * hp_l
                + 1e-20
                - 1e-20;
            self.rev_lp_r = self.rev_lp_alpha * self.rev_lp_r
                + (1.0 - self.rev_lp_alpha) * hp_r
                + 1e-20
                - 1e-20;

            // Write conditioned stereo signal into pre-delay / early-reflection circular buffer
            self.predelay_l[self.predelay_idx] = self.rev_lp_l;
            self.predelay_r[self.predelay_idx] = self.rev_lp_r;

            // Early reflections (wooden salon walls)
            let er_weights: [f32; 4] = [0.36, -0.25, 0.19, -0.14];
            let mut er_l = 0.0f32;
            let mut er_r = 0.0f32;
            for (t, &w_er) in er_weights.iter().enumerate() {
                let idx_l =
                    (self.predelay_idx + PREDELAY_BUF_LEN - self.er_taps_l[t]) & (PREDELAY_BUF_LEN - 1);
                let idx_r =
                    (self.predelay_idx + PREDELAY_BUF_LEN - self.er_taps_r[t]) & (PREDELAY_BUF_LEN - 1);
                er_l += w_er * self.predelay_l[idx_l];
                er_r += w_er * self.predelay_r[idx_r];
            }

            // Pre-delay tap for late FDN/Schroeder tail (12 ms .. 26 ms based on reverb_size)
            let predelay_samples =
                (520 + (self.rev_size_smooth * 680.0) as usize).clamp(64, PREDELAY_BUF_LEN - 2);
            let pd_idx =
                (self.predelay_idx + PREDELAY_BUF_LEN - predelay_samples) & (PREDELAY_BUF_LEN - 1);
            self.predelay_idx = (self.predelay_idx + 1) & (PREDELAY_BUF_LEN - 1);

            // Stereo cross-feed into late reverb tank
            let tank_in_l = (self.predelay_l[pd_idx] + 0.35 * self.predelay_r[pd_idx]) * 0.25;
            let tank_in_r = (self.predelay_r[pd_idx] + 0.35 * self.predelay_l[pd_idx]) * 0.25;

            let feedback = (0.70 + 0.23 * self.rev_size_smooth).clamp(0.55, 0.935);
            let damp = (0.42 - 0.12 * self.rev_size_smooth).clamp(0.20, 0.60);

            let mut comb_sum_l = 0.0f32;
            let mut comb_sum_r = 0.0f32;
            for i in 0..4 {
                comb_sum_l += self.combs_l[i].tick(tank_in_l, feedback, damp);
                comb_sum_r += self.combs_r[i].tick(tank_in_r, feedback, damp);
            }

            let mut diff_l = comb_sum_l;
            let mut diff_r = comb_sum_r;
            for i in 0..2 {
                diff_l = self.allpass_l[i].tick(diff_l);
                diff_r = self.allpass_r[i].tick(diff_r);
            }

            let wet_l = 0.38 * er_l + 0.82 * diff_l;
            let wet_r = 0.38 * er_r + 0.82 * diff_r;

            // Equal-power-inspired dry/wet blend so the direct bandoneon stays present
            let dry_gain = 1.0 - 0.28 * rev_mix;
            let wet_gain = 0.85 * rev_mix;
            (
                comp_l * dry_gain + wet_l * wet_gain,
                comp_r * dry_gain + wet_r * wet_gain,
            )
        } else {
            (comp_l, comp_r)
        };

        let en = self.enable_smooth;
        (
            in_l * (1.0 - en) + final_l * en,
            in_r * (1.0 - en) + final_r * en,
        )
    }
}
