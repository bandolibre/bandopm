use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

pub const NUM_HARMONICS: usize = 16;
pub const NUM_BODY_BANDS: usize = 6;
pub const BODY_BAND_CENTERS_HZ: [f32; NUM_BODY_BANDS] =
    [125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0];

/// Complete acoustic feature vector extracted from either a golden WAV or a synthesized render.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcousticFeatures {
    /// Short-time RMS amplitude envelope sampled every 10 ms (480 frames at 48 kHz).
    pub env_10ms: Vec<f32>,
    /// Detected note onset frame index.
    pub onset_frame: usize,
    /// Peak RMS in dBFS.
    pub peak_rms_db: f32,
    /// Steady-state sustain RMS in dBFS.
    pub steady_rms_db: f32,

    // --- Priority 1: Harmonic Profile & Spectral Balance ---
    /// Relative harmonic amplitudes H1..H16 in dB (normalized so sum of harmonic powers = 0 dB).
    pub harmonics_rel_db: [f32; NUM_HARMONICS],
    /// Energy-weighted harmonic spectral centroid in Hz.
    pub spectral_centroid_hz: f32,

    // --- Priority 2: Attack & Release Transients ---
    /// 10% -> 90% envelope rise time in milliseconds.
    pub attack_rise_ms: f32,
    /// High-frequency transient click energy (dB relative to steady RMS) in first 15 ms of onset.
    pub click_energy_db: f32,

    // --- Priority 3: Dual-Reed (8' + 4') Interaction ---
    /// Ratio of even harmonic power (2f0, 4f0, 6f0 - 4' octave reed + asymmetry)
    /// to odd harmonic power (f0, 3f0, 5f0 - 8' reed) in dB.
    pub octave_4ft_to_8ft_db: f32,
    /// Envelope modulation rate (Hz) of the 2f0 octave component during sustain.
    pub beat_freq_hz: f32,
    /// Onset lag (ms) between 4' octave harmonic (H2) and 8' fundamental (H1) reaching 25% peak.
    pub dual_reed_onset_lag_ms: f32,

    // --- Priority 4: Pressure-Induced Pitch Bend ---
    /// Fine measured fundamental frequency in Hz.
    pub measured_f0_hz: f32,
    /// Pitch offset in cents relative to nominal equal-temperament f0.
    pub measured_cents_offset: f32,

    // --- Priority 5: Wooden Body & Bellows Resonance ---
    /// Relative energy (dB) across 6 octave formant bands (125Hz..4kHz).
    pub octave_band_rel_db: [f32; NUM_BODY_BANDS],
    /// Inter-harmonic air turbulence noise floor (dB relative to harmonic energy).
    pub air_noise_floor_db: f32,
}

#[inline]
fn lin_to_db(x: f32) -> f32 {
    20.0 * (x.max(1e-9)).log10()
}

#[inline]
fn pow_to_db(p: f32) -> f32 {
    10.0 * (p.max(1e-18)).log10()
}

/// Computes normalized Goertzel / DFT magnitude squared at `freq_hz` over `slice` with Hann window.
fn goertzel_power_hann(slice: &[f32], sample_rate: f32, freq_hz: f32) -> f32 {
    if slice.is_empty() || freq_hz >= sample_rate * 0.48 {
        return 1e-18;
    }
    let n = slice.len();
    let omega = TAU * freq_hz / sample_rate;
    let coeff = 2.0 * omega.cos();
    let mut s1 = 0.0f32;
    let mut s2 = 0.0f32;
    let inv_n = TAU / (n as f32);

    for (i, &x) in slice.iter().enumerate() {
        let w = 0.5 - 0.5 * (inv_n * (i as f32)).cos();
        let s0 = x * w + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let power = s1 * s1 + s2 * s2 - coeff * s1 * s2;
    let norm = (n as f32) * 0.5;
    (power / (norm * norm).max(1.0)).max(1e-18)
}

/// Extracts `AcousticFeatures` from a mono audio signal at `sample_rate`.
pub fn extract_features(
    samples: &[f32],
    sample_rate: u32,
    nominal_f0_hz: f32,
    is_staccato: bool,
) -> AcousticFeatures {
    let sr = sample_rate as f32;
    let step_10ms = (sample_rate as usize / 100).max(1); // 480 samples at 48kHz

    // 1. Compute 10ms RMS envelope
    let mut env_10ms = Vec::with_capacity(samples.len() / step_10ms);
    let mut peak_rms = 1e-9f32;
    for chunk in samples.chunks(step_10ms) {
        let sq_sum: f32 = chunk.iter().map(|&x| x * x).sum();
        let rms = (sq_sum / (chunk.len() as f32)).sqrt();
        if rms > peak_rms {
            peak_rms = rms;
        }
        env_10ms.push(rms);
    }

    // 2. Detect onset frame (first 2ms window exceeding max(6% of peak_rms, -58 dBFS))
    let step_2ms = (sample_rate as usize / 500).max(1);
    let onset_thresh = (peak_rms * 0.06).max(1.2e-3);
    let mut onset_frame = 0usize;
    for (idx, chunk) in samples.chunks(step_2ms).enumerate() {
        let rms = (chunk.iter().map(|&x| x * x).sum::<f32>() / (chunk.len() as f32)).sqrt();
        if rms >= onset_thresh {
            onset_frame = idx * step_2ms;
            break;
        }
    }

    // 3. Measure 10% -> 90% attack rise time in ms within first 350ms after onset
    let search_end_2ms = ((onset_frame + (0.35 * sr) as usize).min(samples.len())) / step_2ms;
    let onset_idx_2ms = onset_frame / step_2ms;
    let mut env_2ms = Vec::new();
    let mut attack_peak = 1e-9f32;
    for i in onset_idx_2ms..search_end_2ms.max(onset_idx_2ms + 1) {
        let start = i * step_2ms;
        let end = (start + step_2ms).min(samples.len());
        if start < end {
            let rms = (samples[start..end].iter().map(|&x| x * x).sum::<f32>()
                / ((end - start) as f32))
                .sqrt();
            if rms > attack_peak {
                attack_peak = rms;
            }
            env_2ms.push(rms);
        }
    }
    let t10_idx = env_2ms
        .iter()
        .position(|&r| r >= 0.10 * attack_peak)
        .unwrap_or(0);
    let t90_idx = env_2ms
        .iter()
        .position(|&r| r >= 0.90 * attack_peak)
        .unwrap_or(env_2ms.len().saturating_sub(1));
    let attack_rise_ms = ((t90_idx.saturating_sub(t10_idx) as f32) * 2.0).max(2.0);

    // 4. Select steady-state analysis window
    let (win_start, win_len) = if is_staccato {
        // For staccato: 85ms window centered near the peak after onset
        let len = (0.085 * sr) as usize;
        let start = (onset_frame + (0.015 * sr) as usize).min(samples.len().saturating_sub(len));
        (start, len.min(samples.len().saturating_sub(start)))
    } else {
        // For sustain: 350ms window starting 280ms after onset (well into steady state)
        let len = (0.35 * sr) as usize;
        let start = (onset_frame + (0.28 * sr) as usize).min(samples.len().saturating_sub(len));
        (start, len.min(samples.len().saturating_sub(start)))
    };

    let core_slice = if win_len > 64 {
        &samples[win_start..win_start + win_len]
    } else {
        samples
    };
    let steady_rms =
        (core_slice.iter().map(|&x| x * x).sum::<f32>() / (core_slice.len().max(1) as f32)).sqrt();

    // 5. High-frequency click energy in the first 16ms around onset
    let click_start = onset_frame.saturating_sub((0.004 * sr) as usize);
    let click_end = (onset_frame + (0.016 * sr) as usize).min(samples.len());
    let mut click_hp_sq = 0.0f32;
    if click_end > click_start + 2 {
        for w in samples[click_start..click_end].windows(2) {
            let diff = w[1] - w[0]; // First-order high-pass emphasizing click transient
            click_hp_sq += diff * diff;
        }
        click_hp_sq /= (click_end - click_start) as f32;
    }
    let click_energy_db = lin_to_db(click_hp_sq.sqrt() / steady_rms.max(1e-6));

    // 6. Fine pitch search around nominal_f0_hz (+/- 36 cents in 3-cent steps)
    let mut best_cents = 0.0f32;
    let mut best_f0 = nominal_f0_hz;
    let mut best_harm_pow = -1.0f32;

    for step in -12..=12 {
        let cents = (step as f32) * 3.0;
        let cand_f0 = nominal_f0_hz * 2.0f32.powf(cents / 1200.0);
        let mut p_sum = 0.0f32;
        for k in 1..=4 {
            p_sum += goertzel_power_hann(core_slice, sr, cand_f0 * (k as f32));
        }
        if p_sum > best_harm_pow {
            best_harm_pow = p_sum;
            best_cents = cents;
            best_f0 = cand_f0;
        }
    }

    // 7. Extract H1..H16 powers and inter-harmonic air noise powers
    let mut harm_powers = [1e-18f32; NUM_HARMONICS];
    let mut total_harm_power = 1e-18f32;
    let mut weighted_freq_sum = 0.0f32;
    let mut odd_power = 1e-18f32;
    let mut even_power = 1e-18f32;
    let mut noise_power_sum = 1e-18f32;
    let mut noise_count = 0usize;

    for k in 1..=NUM_HARMONICS {
        let fk = best_f0 * (k as f32);
        if fk < sr * 0.46 {
            let p = goertzel_power_hann(core_slice, sr, fk);
            harm_powers[k - 1] = p;
            total_harm_power += p;
            weighted_freq_sum += p * fk;
            if k % 2 == 1 {
                odd_power += p;
            } else {
                even_power += p;
            }

            // Inter-harmonic noise probe at (k + 0.5) * f0
            let f_noise = best_f0 * (k as f32 + 0.5);
            if f_noise < sr * 0.44 && k <= 10 {
                noise_power_sum += goertzel_power_hann(core_slice, sr, f_noise);
                noise_count += 1;
            }
        }
    }

    let mut harmonics_rel_db = [-60.0f32; NUM_HARMONICS];
    for k in 0..NUM_HARMONICS {
        harmonics_rel_db[k] = pow_to_db(harm_powers[k] / total_harm_power).clamp(-65.0, 0.0);
    }

    let spectral_centroid_hz = (weighted_freq_sum / total_harm_power).clamp(best_f0, 12000.0);
    let octave_4ft_to_8ft_db = pow_to_db(even_power / odd_power).clamp(-35.0, 35.0);
    let avg_noise_pow = noise_power_sum / (noise_count.max(1) as f32);
    let avg_harm_pow = total_harm_power / (NUM_HARMONICS as f32);
    let air_noise_floor_db = pow_to_db(avg_noise_pow / avg_harm_pow).clamp(-70.0, 0.0);

    // 8. Dual-reed onset lag (time difference between H2 and H1 reaching 25% of their sustain power)
    let win_12ms = (0.012 * sr) as usize;
    let hop_6ms = (0.006 * sr).max(1.0) as usize;
    let mut h1_on_ms = 0.0f32;
    let mut h2_on_ms = 0.0f32;
    let mut h1_found = false;
    let mut h2_found = false;
    let h1_thresh = harm_powers[0] * 0.15;
    let h2_thresh = harm_powers[1] * 0.15;

    for step_i in 0..20 {
        let st = onset_frame + step_i * hop_6ms;
        if st + win_12ms >= samples.len() {
            break;
        }
        let sl = &samples[st..st + win_12ms];
        if !h1_found && goertzel_power_hann(sl, sr, best_f0) >= h1_thresh {
            h1_on_ms = (step_i as f32) * 6.0;
            h1_found = true;
        }
        if !h2_found && goertzel_power_hann(sl, sr, best_f0 * 2.0) >= h2_thresh {
            h2_on_ms = (step_i as f32) * 6.0;
            h2_found = true;
        }
        if h1_found && h2_found {
            break;
        }
    }
    let dual_reed_onset_lag_ms = (h2_on_ms - h1_on_ms).clamp(-60.0, 60.0);

    // 9. Octave beating rate (Hz) from slow envelope modulation of 2*f0 over sustain
    let mut beat_freq_hz = 0.8f32;
    if !is_staccato && core_slice.len() >= (0.30 * sr) as usize {
        let sub_win = (0.025 * sr) as usize;
        let sub_hop = (0.0125 * sr) as usize;
        let mut h2_env = Vec::new();
        let mut pos = 0;
        while pos + sub_win <= core_slice.len() {
            let p2 = goertzel_power_hann(&core_slice[pos..pos + sub_win], sr, best_f0 * 2.0).sqrt();
            h2_env.push(p2);
            pos += sub_hop;
        }
        if h2_env.len() >= 6 {
            let mean = h2_env.iter().sum::<f32>() / (h2_env.len() as f32);
            let mut zc = 0usize;
            for w in h2_env.windows(2) {
                if (w[0] - mean).signum() != (w[1] - mean).signum() {
                    zc += 1;
                }
            }
            let dur_s = (h2_env.len() as f32) * 0.0125;
            beat_freq_hz = (0.5 * (zc as f32) / dur_s).clamp(0.1, 8.0);
        }
    }

    // 10. 6-Band Octave Formant Profile (125Hz .. 4kHz)
    //     Assigns harmonics + broadband spectral probes into the 6 octave bands.
    let mut band_powers = [1e-12f32; NUM_BODY_BANDS];
    for (i, &center) in BODY_BAND_CENTERS_HZ.iter().enumerate() {
        let lo = center * 0.7071;
        let hi = center * 1.4142;
        // Sum any harmonics falling in this band plus 3 fixed probe frequencies per band
        for k in 1..=NUM_HARMONICS {
            let fk = best_f0 * (k as f32);
            if fk >= lo && fk < hi {
                band_powers[i] += harm_powers[k - 1];
            }
        }
        for mult in [0.82f32, 1.0, 1.22] {
            let fp = center * mult;
            if fp < sr * 0.45 {
                band_powers[i] += 0.25 * goertzel_power_hann(core_slice, sr, fp);
            }
        }
    }
    let total_band_pow: f32 = band_powers.iter().sum::<f32>().max(1e-12);
    let mut octave_band_rel_db = [0.0f32; NUM_BODY_BANDS];
    for i in 0..NUM_BODY_BANDS {
        octave_band_rel_db[i] = pow_to_db(band_powers[i] / total_band_pow).clamp(-45.0, 0.0);
    }

    AcousticFeatures {
        env_10ms,
        onset_frame,
        peak_rms_db: lin_to_db(peak_rms),
        steady_rms_db: lin_to_db(steady_rms),
        harmonics_rel_db,
        spectral_centroid_hz,
        attack_rise_ms,
        click_energy_db,
        octave_4ft_to_8ft_db,
        beat_freq_hz,
        dual_reed_onset_lag_ms,
        measured_f0_hz: best_f0,
        measured_cents_offset: best_cents,
        octave_band_rel_db,
        air_noise_floor_db,
    }
}
