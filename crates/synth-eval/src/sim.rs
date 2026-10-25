use crate::features::{extract_features, AcousticFeatures};
use serde::{Deserialize, Serialize};
use synth_core::{BandoneonPreset, Hand, MidiEvent, SynthEngine};

/// Metadata entry for a single golden sample from `assets/samples/golden/manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenSampleEntry {
    pub id: String,
    pub rel_path: String,
    pub sample_label: String,
    pub hand: String,
    pub midi_note: u8,
    pub fundamental_hz: f32,
    pub articulation: String,
    pub dynamic: String,
    pub split: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenManifest {
    pub sample_rate_hz: u32,
    pub total_samples: usize,
    pub samples: Vec<GoldenSampleEntry>,
}

/// Cached golden sample features so we don't re-decode 700MB of 24-bit WAVs on every iteration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedGoldenSample {
    pub meta: GoldenSampleEntry,
    pub features: AcousticFeatures,
}

impl GoldenSampleEntry {
    pub fn parsed_hand(&self) -> Hand {
        if self.hand == "left_bajos" {
            Hand::LeftBajos
        } else {
            Hand::RightCantabile
        }
    }

    pub fn is_staccato(&self) -> bool {
        self.articulation == "stacc"
    }
}

/// How long the recording takes to fall from half its peak to a tenth of it after the peak,
/// in ms (10 ms resolution, at least 10 ms): how fast the note was stopped.
fn release_fall_ms(env_10ms: &[f32]) -> f32 {
    let Some((peak_i, &peak)) = env_10ms
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
    else {
        return 30.0;
    };
    let after = &env_10ms[peak_i..];
    let half = after.iter().position(|&e| e < 0.5 * peak);
    let tenth = after.iter().position(|&e| e < 0.1 * peak);
    match (half, tenth) {
        (Some(h), Some(t)) if t >= h => ((t - h) as f32 * 10.0).max(10.0),
        _ => 30.0,
    }
}

/// Simulates a single note on `SynthEngine` using **Continuous Envelope Inversion** from the
/// golden sample's `AcousticFeatures` and returns the synthesized `AcousticFeatures`
/// (and optionally the rendered audio buffer).
pub fn simulate_and_extract(
    entry: &GoldenSampleEntry,
    gold_feat: &AcousticFeatures,
    preset: &BandoneonPreset,
    sample_rate: u32,
    return_audio: bool,
) -> (AcousticFeatures, Option<Vec<f32>>) {
    let mut engine = SynthEngine::new(sample_rate as f32, preset.clone());
    let hand = entry.parsed_hand();
    let is_stacc = entry.is_staccato();

    // Base dynamic scale factor anchored by the recording's dynamic marking (p, mf, f)
    let base_dyn_norm: f32 = match entry.dynamic.as_str() {
        "p" => 0.28,
        "mf" => 0.58,
        "f" => 0.92,
        _ => 0.58,
    };

    // Velocities as a Bandolibre in key-speed mode would send them, with the synth in
    // `VelocityMode::Articulation`: the key's speed, here inferred from how fast the recording
    // starts and stops. From v5 the synth turns velocity into the key's travel time, inverse to
    // its speed, which the Bandolibre maps linearly to velocity (factory settings: 3.3..100 ms
    // down, 2.5..100 ms up). The key time is taken as the recording's rise or fall minus
    // REED_MS for the reed's own onset; it is only an estimate, since the eval also replays the
    // recording's attack as bellows pressure. The times are fixed here, not read from the
    // preset, so that the targets do not move with it. Before v5 velocity also stood for the
    // bellows level, so it keeps its old proxy, which grows with the dynamic marking.
    const REED_MS: f32 = 15.0;
    let key_velocity = |time_ms: f32, fast_ms: f32, slow_ms: f32, lo: f32, hi: f32| -> u8 {
        let t = (time_ms - REED_MS).clamp(fast_ms, slow_ms);
        let v = 1.0 + 126.0 * (1.0 / t - 1.0 / slow_ms) / (1.0 / fast_ms - 1.0 / slow_ms);
        v.clamp(lo, hi).round() as u8
    };
    let vel = if preset.version.use_v5_vivo() {
        if is_stacc {
            key_velocity(gold_feat.attack_rise_ms, 3.3, 100.0, 1.0, 127.0)
        } else {
            // A sustained note's slow start is the bellows swelling (CC#11), not the key.
            key_velocity(gold_feat.attack_rise_ms, 3.3, 100.0, 40.0, 100.0)
        }
    } else {
        let attack_speed_norm = (45.0 / gold_feat.attack_rise_ms.max(15.0)).clamp(0.18, 1.25);
        let vel_f = if is_stacc {
            (55.0 + 62.0 * base_dyn_norm * attack_speed_norm.min(1.0)).clamp(32.0, 124.0)
        } else {
            (24.0 + 78.0 * base_dyn_norm * attack_speed_norm).clamp(18.0, 118.0)
        };
        vel_f as u8
    };
    let release_vel = if preset.version.use_v5_vivo() {
        key_velocity(release_fall_ms(&gold_feat.env_10ms), 2.5, 100.0, 1.0, 127.0)
    } else {
        72
    };

    let step_10ms = (sample_rate as usize / 100).max(1);
    let sub_steps = 5usize; // 2 ms control interpolation resolution (96 samples at 48 kHz)
    let step_2ms = step_10ms / sub_steps;

    let max_sim_steps = if is_stacc { 32 } else { 75 }; // 0.32s for stacc, 0.75s for sustain
    let onset_step = (gold_feat.onset_frame / step_10ms).min(4);
    let total_steps = (onset_step + max_sim_steps).min(gold_feat.env_10ms.len().max(20));

    // Normalize contour against the peak RMS within the active simulation window
    // so that 10% -> 90% attack rise contour inversion is not skewed by late tail peaks.
    let base_env_idx = (gold_feat.onset_frame / step_10ms).saturating_sub(onset_step);
    let mut win_peak_env = 1e-6f32;
    for step in 0..total_steps {
        let e = gold_feat
            .env_10ms
            .get(base_env_idx + step)
            .copied()
            .unwrap_or(0.0);
        if e > win_peak_env {
            win_peak_env = e;
        }
    }

    let mut rendered = vec![0.0f32; total_steps * step_10ms];
    let mut note_triggered = false;
    let mut note_released = false;

    let inv_gamma = 1.0 / preset.bellows_curve_gamma.clamp(0.6, 2.5);

    for step in 0..total_steps {
        let env_idx = base_env_idx + step;
        let env0 = gold_feat.env_10ms.get(env_idx).copied().unwrap_or(0.0);
        let env1 = gold_feat
            .env_10ms
            .get(env_idx + 1)
            .copied()
            .unwrap_or(env0);

        for sub in 0..sub_steps {
            let frac = (sub as f32) / (sub_steps as f32);
            let raw_env = env0 + frac * (env1 - env0);
            let norm_shape = (raw_env / win_peak_env).clamp(0.0, 1.0);

            // Invert the static bellows + reed threshold curve so radiated RMS tracks `norm_shape`
            let p_floor = 0.022;
            let linear_p = p_floor + (base_dyn_norm - p_floor).max(0.05) * norm_shape.powf(1.05);
            let expr = linear_p.powf(inv_gamma).clamp(0.0, 1.0);

            if step >= onset_step && !note_triggered {
                engine.handle_midi_event(MidiEvent::ExpressionNormalized(expr));
                engine.handle_midi_event(MidiEvent::NoteOn {
                    note: entry.midi_note,
                    velocity: vel,
                    hand,
                });
                note_triggered = true;
            } else if note_triggered && !note_released {
                engine.handle_midi_event(MidiEvent::ExpressionNormalized(expr));
                if is_stacc && step >= onset_step + 14 && norm_shape < 0.25 {
                    engine.handle_midi_event(MidiEvent::NoteOff {
                        note: entry.midi_note,
                        release_velocity: release_vel,
                        hand,
                    });
                    note_released = true;
                }
            }

            let start = step * step_10ms + sub * step_2ms;
            let end = if sub + 1 == sub_steps {
                (step + 1) * step_10ms
            } else {
                start + step_2ms
            };
            engine.render_mono(&mut rendered[start..end]);
        }
    }

    let synth_feat = extract_features(&rendered, sample_rate, entry.fundamental_hz, is_stacc);
    let audio_opt = if return_audio { Some(rendered) } else { None };
    (synth_feat, audio_opt)
}
