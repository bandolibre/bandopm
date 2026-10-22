#![allow(clippy::needless_range_loop, clippy::for_kv_map)]
use crate::features::{AcousticFeatures, BODY_BAND_CENTERS_HZ, NUM_BODY_BANDS, NUM_HARMONICS};
use crate::sim::{simulate_and_extract, CachedGoldenSample};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;
use synth_core::{BandoneonPreset, Hand, MidiEvent, SynthEngine};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatesReport {
    pub passed_all: bool,
    pub stability_no_nan_or_inf: bool,
    pub silence_convergence: bool,
    pub cpu_budget_passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerfReport {
    pub sample_rate_hz: u32,
    pub quantum_samples: usize,
    pub realtime_quantum_us: f32,
    pub voice_1_us_per_quantum: f32,
    pub voice_16_us_per_quantum: f32,
    pub voice_16_budget_us: f32,
    pub core_utilization_16v_pct: f32,
    pub realtime_headroom_factor: f32,
    pub performance_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryScore {
    pub score: f32,
    pub loss: f32,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitCategories {
    pub harmonic_profile: CategoryScore,
    pub transients: CategoryScore,
    pub dual_reed: CategoryScore,
    pub pitch_bend: CategoryScore,
    pub body_resonance: CategoryScore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoresSummary {
    pub composite_train_score: f32,
    pub composite_val_score: f32,
    pub performance_score: f32,
    pub composite_train_loss: f32,
    pub composite_val_loss: f32,
    pub regularization_penalty: f32,
    pub train_categories: SplitCategories,
    pub val_categories: SplitCategories,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicBreakdown {
    pub p: f32,
    pub mf: f32,
    pub f: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedDeltasReport {
    /// Signed dB error (synth - gold) for H1..H8 averaged over train sustain samples.
    pub h1_to_h8_err_db: [f32; 8],
    /// Signed spectral centroid error in Hz (synth - gold) by dynamic.
    pub spectral_centroid_err_hz: DynamicBreakdown,
    /// Signed 4'-to-8' octave balance error in dB (synth - gold) by dynamic and hand.
    pub octave_4ft_to_8ft_err_db_left: DynamicBreakdown,
    pub octave_4ft_to_8ft_err_db_right: DynamicBreakdown,
    /// Signed attack rise time error in ms (synth - gold).
    pub attack_rise_err_ms_sustain: f32,
    pub attack_rise_err_ms_stacc: f32,
    /// Signed pallet click energy error in dB (synth - gold).
    pub pallet_click_err_db: f32,
    /// Signed octave beat frequency error in Hz (synth - gold).
    pub beat_freq_err_hz: f32,
    /// Signed dual-reed onset lag error in ms (synth - gold) at soft dynamic `p`.
    pub dual_reed_onset_lag_err_ms: f32,
    /// Signed pitch bend drift from `p` to `f` in cents (synth - gold).
    pub drift_p_to_f_err_cents: f32,
    /// Signed dB error across the 6 wooden body formant octave bands (125Hz..4kHz).
    pub body_band_err_db: [f32; NUM_BODY_BANDS],
    /// Signed air turbulence noise floor error in dB (synth - gold).
    pub air_noise_floor_err_db: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scorecard {
    pub preset_name: String,
    pub total_evaluated_samples: usize,
    pub gates: GatesReport,
    pub performance: PerfReport,
    pub scores: ScoresSummary,
    pub diagnostics_signed_deltas: SignedDeltasReport,
    pub top_actionable_recommendations: Vec<String>,
}

struct SampleEvalResult {
    is_val: bool,
    is_left: bool,
    is_stacc: bool,
    dynamic: String,
    note_key: String,
    gold: AcousticFeatures,
    synth: AcousticFeatures,
    loss_harmonic: f32,
    loss_transient: f32,
    loss_dual_reed: f32,
    loss_body: f32,
}

#[inline]
fn loss_to_score(loss: f32) -> f32 {
    (100.0 * (-loss.max(0.0)).exp()).clamp(0.0, 100.0)
}

/// Evaluates `preset` across `golden_samples` in parallel and computes the full `Scorecard`.
pub fn evaluate_preset(
    preset: &BandoneonPreset,
    golden_samples: &[CachedGoldenSample],
    sample_rate: u32,
    run_perf_bench: bool,
) -> Scorecard {
    let results: Vec<SampleEvalResult> = golden_samples
        .par_iter()
        .map(|item| {
            let (synth, _) =
                simulate_and_extract(&item.meta, &item.features, preset, sample_rate, false);
            let g = &item.features;

            // 1. Harmonic loss: weighted average dB error across H1..H16 + centroid relative error
            let mut h_err_sum = 0.0f32;
            let mut w_sum = 0.0f32;
            for k in 0..NUM_HARMONICS {
                let w = 1.0 / ((k as f32 + 1.0).powf(0.45));
                let diff = (synth.harmonics_rel_db[k] - g.harmonics_rel_db[k]).abs();
                h_err_sum += w * diff;
                w_sum += w;
            }
            let mean_h_db_err = h_err_sum / w_sum;
            let centroid_rel_err = ((synth.spectral_centroid_hz - g.spectral_centroid_hz).abs())
                / g.spectral_centroid_hz.max(100.0);
            let loss_harmonic = (mean_h_db_err / 18.0) + 0.45 * centroid_rel_err;

            // 2. Transient loss: attack rise time error + click energy error
            let rise_err_ms = (synth.attack_rise_ms - g.attack_rise_ms).abs();
            let click_err_db = (synth.click_energy_db - g.click_energy_db).abs();
            let loss_transient = (rise_err_ms / 45.0) + (click_err_db / 24.0);

            // 3. Dual-reed interaction loss: 4'/8' ratio + beating rate + onset lag
            let oct_err_db = (synth.octave_4ft_to_8ft_db - g.octave_4ft_to_8ft_db).abs();
            let beat_err_hz = (synth.beat_freq_hz - g.beat_freq_hz).abs();
            let lag_err_ms = (synth.dual_reed_onset_lag_ms - g.dual_reed_onset_lag_ms).abs();
            let loss_dual_reed =
                (oct_err_db / 14.0) + 0.15 * (beat_err_hz / 2.5) + 0.15 * (lag_err_ms / 35.0);

            // 5. Body resonance & air noise loss
            let mut band_err_sum = 0.0f32;
            for b in 0..NUM_BODY_BANDS {
                band_err_sum += (synth.octave_band_rel_db[b] - g.octave_band_rel_db[b]).abs();
            }
            let mean_band_err = band_err_sum / (NUM_BODY_BANDS as f32);
            let noise_err = (synth.air_noise_floor_db - g.air_noise_floor_db).abs();
            let loss_body = (mean_band_err / 14.0) + 0.35 * (noise_err / 25.0);

            SampleEvalResult {
                is_val: item.meta.split == "val",
                is_left: item.meta.hand == "left_bajos",
                is_stacc: item.meta.is_staccato(),
                dynamic: item.meta.dynamic.clone(),
                note_key: format!("{}_{}", item.meta.sample_label, item.meta.articulation),
                gold: g.clone(),
                synth,
                loss_harmonic,
                loss_transient,
                loss_dual_reed,
                loss_body,
            }
        })
        .collect();

    // Compute Priority 4: Pressure-induced pitch bend (comparing `p` vs `f` for identical notes)
    let mut by_note: HashMap<String, (Option<&SampleEvalResult>, Option<&SampleEvalResult>)> =
        HashMap::new();
    for r in &results {
        if !r.is_stacc {
            let entry = by_note.entry(r.note_key.clone()).or_insert((None, None));
            if r.dynamic == "p" {
                entry.0 = Some(r);
            } else if r.dynamic == "f" {
                entry.1 = Some(r);
            }
        }
    }

    let mut train_pitch_err_sum = 0.0f32;
    let mut train_pitch_cnt = 0usize;
    let mut val_pitch_err_sum = 0.0f32;
    let mut val_pitch_cnt = 0usize;
    let mut signed_drift_err_sum = 0.0f32;

    for (_k, (opt_p, opt_f)) in &by_note {
        if let (Some(rp), Some(rf)) = (opt_p, opt_f) {
            let gold_drift = rf.gold.measured_cents_offset - rp.gold.measured_cents_offset;
            let synth_drift = rf.synth.measured_cents_offset - rp.synth.measured_cents_offset;
            let err = (synth_drift - gold_drift).abs();
            if rp.is_val {
                val_pitch_err_sum += err / 15.0;
                val_pitch_cnt += 1;
            } else {
                train_pitch_err_sum += err / 15.0;
                train_pitch_cnt += 1;
                signed_drift_err_sum += synth_drift - gold_drift;
            }
        }
    }

    let train_pitch_loss = train_pitch_err_sum / (train_pitch_cnt.max(1) as f32);
    let val_pitch_loss = val_pitch_err_sum / (val_pitch_cnt.max(1) as f32);
    let drift_p_to_f_err_cents = signed_drift_err_sum / (train_pitch_cnt.max(1) as f32);

    let aggregate_split = |is_val: bool, pitch_loss: f32| -> (SplitCategories, f32, f32) {
        let subset: Vec<&SampleEvalResult> =
            results.iter().filter(|r| r.is_val == is_val).collect();
        let n = subset.len().max(1) as f32;
        let lh = subset.iter().map(|r| r.loss_harmonic).sum::<f32>() / n;
        let lt = subset.iter().map(|r| r.loss_transient).sum::<f32>() / n;
        let ld = subset.iter().map(|r| r.loss_dual_reed).sum::<f32>() / n;
        let lb = subset.iter().map(|r| r.loss_body).sum::<f32>() / n;

        let total_loss = 0.35 * lh + 0.25 * lt + 0.20 * ld + 0.10 * pitch_loss + 0.10 * lb;
        let cats = SplitCategories {
            harmonic_profile: CategoryScore {
                score: loss_to_score(lh),
                loss: lh,
                weight: 0.35,
            },
            transients: CategoryScore {
                score: loss_to_score(lt),
                loss: lt,
                weight: 0.25,
            },
            dual_reed: CategoryScore {
                score: loss_to_score(ld),
                loss: ld,
                weight: 0.20,
            },
            pitch_bend: CategoryScore {
                score: loss_to_score(pitch_loss),
                loss: pitch_loss,
                weight: 0.10,
            },
            body_resonance: CategoryScore {
                score: loss_to_score(lb),
                loss: lb,
                weight: 0.10,
            },
        };
        (cats, total_loss, loss_to_score(total_loss))
    };

    let reg_penalty = preset.roughness_penalty();
    let (train_cats, raw_train_loss, _) = aggregate_split(false, train_pitch_loss);
    let (val_cats, raw_val_loss, _) = aggregate_split(true, val_pitch_loss);

    let composite_train_loss = raw_train_loss + reg_penalty;
    let composite_val_loss = raw_val_loss + reg_penalty;
    let composite_train_score = loss_to_score(composite_train_loss);
    let composite_val_score = loss_to_score(composite_val_loss);

    // Compute signed physical deltas on the Train split for AI diagnostics
    let train_sub: Vec<&SampleEvalResult> = results.iter().filter(|r| !r.is_val).collect();
    let n_train = train_sub.len().max(1) as f32;

    let mut h1_to_h8_err_db = [0.0f32; 8];
    for r in &train_sub {
        for k in 0..8 {
            h1_to_h8_err_db[k] += (r.synth.harmonics_rel_db[k] - r.gold.harmonics_rel_db[k]) / n_train;
        }
    }

    let dyn_avg = |pred: &dyn Fn(&SampleEvalResult) -> bool,
                   metric: &dyn Fn(&SampleEvalResult) -> f32|
     -> f32 {
        let mut s = 0.0f32;
        let mut c = 0usize;
        for r in &train_sub {
            if pred(r) {
                s += metric(r);
                c += 1;
            }
        }
        s / (c.max(1) as f32)
    };

    let spectral_centroid_err_hz = DynamicBreakdown {
        p: dyn_avg(
            &|r| r.dynamic == "p",
            &|r| r.synth.spectral_centroid_hz - r.gold.spectral_centroid_hz,
        ),
        mf: dyn_avg(
            &|r| r.dynamic == "mf",
            &|r| r.synth.spectral_centroid_hz - r.gold.spectral_centroid_hz,
        ),
        f: dyn_avg(
            &|r| r.dynamic == "f",
            &|r| r.synth.spectral_centroid_hz - r.gold.spectral_centroid_hz,
        ),
    };

    let octave_4ft_to_8ft_err_db_left = DynamicBreakdown {
        p: dyn_avg(
            &|r| r.is_left && r.dynamic == "p",
            &|r| r.synth.octave_4ft_to_8ft_db - r.gold.octave_4ft_to_8ft_db,
        ),
        mf: dyn_avg(
            &|r| r.is_left && r.dynamic == "mf",
            &|r| r.synth.octave_4ft_to_8ft_db - r.gold.octave_4ft_to_8ft_db,
        ),
        f: dyn_avg(
            &|r| r.is_left && r.dynamic == "f",
            &|r| r.synth.octave_4ft_to_8ft_db - r.gold.octave_4ft_to_8ft_db,
        ),
    };

    let octave_4ft_to_8ft_err_db_right = DynamicBreakdown {
        p: dyn_avg(
            &|r| !r.is_left && r.dynamic == "p",
            &|r| r.synth.octave_4ft_to_8ft_db - r.gold.octave_4ft_to_8ft_db,
        ),
        mf: dyn_avg(
            &|r| !r.is_left && r.dynamic == "mf",
            &|r| r.synth.octave_4ft_to_8ft_db - r.gold.octave_4ft_to_8ft_db,
        ),
        f: dyn_avg(
            &|r| !r.is_left && r.dynamic == "f",
            &|r| r.synth.octave_4ft_to_8ft_db - r.gold.octave_4ft_to_8ft_db,
        ),
    };

    let attack_rise_err_ms_sustain = dyn_avg(
        &|r| !r.is_stacc,
        &|r| r.synth.attack_rise_ms - r.gold.attack_rise_ms,
    );
    let attack_rise_err_ms_stacc = dyn_avg(
        &|r| r.is_stacc,
        &|r| r.synth.attack_rise_ms - r.gold.attack_rise_ms,
    );
    let pallet_click_err_db =
        dyn_avg(&|_| true, &|r| r.synth.click_energy_db - r.gold.click_energy_db);
    let beat_freq_err_hz = dyn_avg(
        &|r| !r.is_stacc,
        &|r| r.synth.beat_freq_hz - r.gold.beat_freq_hz,
    );
    let dual_reed_onset_lag_err_ms = dyn_avg(
        &|r| r.dynamic == "p",
        &|r| r.synth.dual_reed_onset_lag_ms - r.gold.dual_reed_onset_lag_ms,
    );

    let mut body_band_err_db = [0.0f32; NUM_BODY_BANDS];
    for b in 0..NUM_BODY_BANDS {
        body_band_err_db[b] = dyn_avg(
            &|_| true,
            &|r| r.synth.octave_band_rel_db[b] - r.gold.octave_band_rel_db[b],
        );
    }
    let air_noise_floor_err_db = dyn_avg(
        &|r| !r.is_stacc,
        &|r| r.synth.air_noise_floor_db - r.gold.air_noise_floor_db,
    );

    let deltas = SignedDeltasReport {
        h1_to_h8_err_db,
        spectral_centroid_err_hz,
        octave_4ft_to_8ft_err_db_left,
        octave_4ft_to_8ft_err_db_right,
        attack_rise_err_ms_sustain,
        attack_rise_err_ms_stacc,
        pallet_click_err_db,
        beat_freq_err_hz,
        dual_reed_onset_lag_err_ms,
        drift_p_to_f_err_cents,
        body_band_err_db,
        air_noise_floor_err_db,
    };

    let (gates, performance) = if run_perf_bench {
        run_benchmarks_and_gates(preset, sample_rate)
    } else {
        (
            GatesReport {
                passed_all: true,
                stability_no_nan_or_inf: true,
                silence_convergence: true,
                cpu_budget_passed: true,
            },
            PerfReport {
                sample_rate_hz: sample_rate,
                quantum_samples: 128,
                realtime_quantum_us: 2666.7,
                voice_1_us_per_quantum: 0.0,
                voice_16_us_per_quantum: 0.0,
                voice_16_budget_us: 350.0,
                core_utilization_16v_pct: 0.0,
                realtime_headroom_factor: 0.0,
                performance_score: 100.0,
            },
        )
    };

    let recommendations = build_recommendations(&deltas, &train_cats);
    let perf_score = performance.performance_score;

    Scorecard {
        preset_name: preset.name.clone(),
        total_evaluated_samples: results.len(),
        gates,
        performance,
        scores: ScoresSummary {
            composite_train_score,
            composite_val_score,
            performance_score: perf_score,
            composite_train_loss,
            composite_val_loss,
            regularization_penalty: reg_penalty,
            train_categories: train_cats,
            val_categories: val_cats,
        },
        diagnostics_signed_deltas: deltas,
        top_actionable_recommendations: recommendations,
    }
}

/// Computes the normalized 0..100 Performance Score from the 16-voice quantum render time (`us`)
/// relative to the `350 us` real-time budget (and `2666.7 us` quantum deadline).
#[inline]
pub fn compute_performance_score(voice_16_us: f32, budget_us: f32, gates_passed: bool) -> f32 {
    if !gates_passed {
        return (40.0 * (-((voice_16_us - budget_us).max(0.0) / budget_us)).exp()).clamp(0.0, 49.9);
    }
    let ratio = (voice_16_us / budget_us.max(1.0)).max(0.0);
    (100.0 * (-0.35 * ratio).exp()).clamp(0.0, 100.0)
}

/// Runs native CPU benchmarks (1-voice and 16-voice per 128-sample quantum) and stability checks.
pub fn run_benchmarks_and_gates(
    preset: &BandoneonPreset,
    sample_rate: u32,
) -> (GatesReport, PerfReport) {
    let sr = sample_rate as f32;
    let mut left = [0.0f32; 128];
    let mut right = [0.0f32; 128];

    // 1. 1-Voice CPU Benchmark (warmup + best of 4 batches of 400 quanta to eliminate OS jitter)
    let mut synth1 = SynthEngine::new(sr, preset.clone());
    synth1.handle_midi_event(MidiEvent::ControlChange11(96));
    synth1.handle_midi_event(MidiEvent::NoteOn {
        note: 60,
        velocity: 96,
        hand: Hand::RightCantabile,
    });
    for _ in 0..150 {
        synth1.render(&mut left, &mut right);
    }
    let batch_quanta = 400usize;
    let mut voice_1_us = f32::MAX;
    for _ in 0..4 {
        let t0 = Instant::now();
        for _ in 0..batch_quanta {
            synth1.render(&mut left, &mut right);
            std::hint::black_box((left[0], right[0]));
        }
        let us = (t0.elapsed().as_secs_f64() * 1e6 / (batch_quanta as f64)) as f32;
        if us < voice_1_us {
            voice_1_us = us;
        }
    }

    // 2. 16-Voice CPU Benchmark & Stability Check
    let mut synth16 = SynthEngine::new(sr, preset.clone());
    for i in 0..16u8 {
        let hand = if i < 8 {
            Hand::LeftBajos
        } else {
            Hand::RightCantabile
        };
        synth16.handle_midi_event(MidiEvent::NoteOn {
            note: 38 + i * 3,
            velocity: 120,
            hand,
        });
    }
    synth16.handle_midi_event(MidiEvent::ControlChange11(127));

    // Warmup 150 quanta while checking for NaN/Inf under CC#11 jump
    let mut no_nan = true;
    for q in 0..150 {
        if q == 75 {
            synth16.handle_midi_event(MidiEvent::ControlChange11(30));
        } else if q == 110 {
            synth16.handle_midi_event(MidiEvent::ControlChange11(127));
        }
        synth16.render(&mut left, &mut right);
        if left.iter().any(|x| !x.is_finite()) || right.iter().any(|x| !x.is_finite()) {
            no_nan = false;
        }
    }

    let mut voice_16_us = f32::MAX;
    for _ in 0..4 {
        let t1 = Instant::now();
        for _ in 0..batch_quanta {
            synth16.render(&mut left, &mut right);
            std::hint::black_box((left[0], right[0]));
        }
        if left.iter().any(|x| !x.is_finite()) || right.iter().any(|x| !x.is_finite()) {
            no_nan = false;
        }
        let us = (t1.elapsed().as_secs_f64() * 1e6 / (batch_quanta as f64)) as f32;
        if us < voice_16_us {
            voice_16_us = us;
        }
    }

    // 3. Silence Convergence Check (< -90 dBFS within 0.5s after CC#11=0 and NoteOff)
    synth16.handle_midi_event(MidiEvent::ControlChange11(0));
    synth16.handle_midi_event(MidiEvent::AllNotesOff);
    for _ in 0..180 {
        synth16.render(&mut left, &mut right);
    }
    let tail_rms = (left.iter().map(|x| x * x).sum::<f32>() / 128.0).sqrt();
    let silence_ok = tail_rms < 3.16e-5;
    let budget_us = 350.0f32;
    let cpu_ok = voice_16_us < budget_us;
    let passed_all = no_nan && silence_ok && cpu_ok;
    let realtime_quantum_us = 128.0 * 1e6 / sr;
    let core_utilization_16v_pct = 100.0 * voice_16_us / realtime_quantum_us;
    let realtime_headroom_factor = realtime_quantum_us / voice_16_us.max(0.1);
    let performance_score = compute_performance_score(voice_16_us, budget_us, passed_all);

    (
        GatesReport {
            passed_all,
            stability_no_nan_or_inf: no_nan,
            silence_convergence: silence_ok,
            cpu_budget_passed: cpu_ok,
        },
        PerfReport {
            sample_rate_hz: sample_rate,
            quantum_samples: 128,
            realtime_quantum_us,
            voice_1_us_per_quantum: voice_1_us,
            voice_16_us_per_quantum: voice_16_us,
            voice_16_budget_us: budget_us,
            core_utilization_16v_pct,
            realtime_headroom_factor,
            performance_score,
        },
    )
}

fn build_recommendations(deltas: &SignedDeltasReport, cats: &SplitCategories) -> Vec<String> {
    let mut recs = Vec::new();

    if deltas.spectral_centroid_err_hz.mf.abs() > 45.0 {
        if deltas.spectral_centroid_err_hz.mf < 0.0 {
            recs.push(format!(
                "Harmonic Brightness (mf): Synth spectral centroid is {:.0} Hz too dark; increase `slot_sharpness_base` or `pressure_brightness_mod`.",
                deltas.spectral_centroid_err_hz.mf
            ));
        } else {
            recs.push(format!(
                "Harmonic Brightness (mf): Synth spectral centroid is +{:.0} Hz too bright; decrease `slot_sharpness_base`.",
                deltas.spectral_centroid_err_hz.mf
            ));
        }
    }

    if deltas.octave_4ft_to_8ft_err_db_right.mf.abs() > 1.5 {
        recs.push(format!(
            "Right-Hand Dual-Reed Balance (mf): 4'-to-8' octave ratio error is {:+.1} dB; adjust `right_cantabile.octave_4ft_gain_base` and `reed_asymmetry_base`.",
            deltas.octave_4ft_to_8ft_err_db_right.mf
        ));
    }

    if deltas.octave_4ft_to_8ft_err_db_left.mf.abs() > 1.5 {
        recs.push(format!(
            "Left-Hand Dual-Reed Balance (mf): 4'-to-8' octave ratio error is {:+.1} dB; adjust `left_bajos.octave_4ft_gain_base` and `reed_asymmetry_base`.",
            deltas.octave_4ft_to_8ft_err_db_left.mf
        ));
    }

    if deltas.attack_rise_err_ms_sustain.abs() > 4.0 || deltas.attack_rise_err_ms_stacc.abs() > 4.0
    {
        recs.push(format!(
            "Attack Transients: Rise time delta is {:+.1} ms (sustain) / {:+.1} ms (stacc); adjust `valve_open_ms`, `bellows_attack_ms`, and `piston_coupling_base`.",
            deltas.attack_rise_err_ms_sustain, deltas.attack_rise_err_ms_stacc
        ));
    }

    if deltas.pallet_click_err_db.abs() > 2.0 {
        recs.push(format!(
            "Mechanical Pallet Click: Onset click energy error is {:+.1} dB; adjust `pallet_click_gain`.",
            deltas.pallet_click_err_db
        ));
    }

    // Check worst body band
    if let Some((idx, &err)) = deltas
        .body_band_err_db
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
    {
        if err.abs() > 1.5 {
            recs.push(format!(
                "Wooden Body Formant ({:.0} Hz band): Error is {:+.1} dB; adjust `body_modes[{}].gain_db`.",
                BODY_BAND_CENTERS_HZ[idx], err, idx
            ));
        }
    }

    if recs.is_empty() {
        recs.push(format!(
            "All primary physical deltas are well-balanced (Harmonic Score {:.1}, Transient Score {:.1}). Fine-tune higher harmonics H5..H12.",
            cats.harmonic_profile.score, cats.transients.score
        ));
    }
    recs
}

/// Writes both `scorecard.json` and `scorecard.md` to `out_dir`.
pub fn write_scorecard_reports(scorecard: &Scorecard, out_dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(out_dir)?;
    let json_path = out_dir.join("scorecard.json");
    let md_path = out_dir.join("scorecard.md");

    let json_str = serde_json::to_string_pretty(scorecard)?;
    std::fs::write(&json_path, json_str)?;

    let s = &scorecard.scores;
    let d = &scorecard.diagnostics_signed_deltas;
    let p = &scorecard.performance;
    let g = &scorecard.gates;

    let mut md = format!(
        "# Bandolibre Physical Modeling Evaluation Scorecard

- **Preset**: `{preset_name}`
- **Evaluated Golden Samples**: `{total_samples}` (`train` + `val` splits)
- **Hard Gates**: `{gate_str}` (No NaN/Inf: `{no_nan}`, Silence `< -90 dBFS`: `{silence}`, CPU `< {budget:.0} us`: `{cpu_ok}`)

## 1. Composite, Category & Performance Scores (`0.0` to `100.0`, Higher is Better)

| Metric / Category | Weight | Train Score | Validation Score | Train Loss | Val Loss |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Composite Acoustic Total** | **100%** | **{tr_score:.2}** | **{val_score:.2}** | `{tr_loss:.4}` | `{val_loss:.4}` |
| **Real-Time Performance Score** | **CPU** | **{perf_score:.2}** | **{perf_score:.2}** | `{v16_us:.1} us` | `{headroom:.1}x RT` |
| 1. Harmonic Profile & Balance | 35% | {tr_h_s:.2} | {val_h_s:.2} | `{tr_h_l:.4}` | `{val_h_l:.4}` |
| 2. Attack & Release Transients | 25% | {tr_t_s:.2} | {val_t_s:.2} | `{tr_t_l:.4}` | `{val_t_l:.4}` |
| 3. Dual-Reed (8' + 4') Coupling | 20% | {tr_d_s:.2} | {val_d_s:.2} | `{tr_d_l:.4}` | `{val_d_l:.4}` |
| 4. Pressure-Induced Pitch Bend | 10% | {tr_p_s:.2} | {val_p_s:.2} | `{tr_p_l:.4}` | `{val_p_l:.4}` |
| 5. Wooden Body & Bellows Formants | 10% | {tr_b_s:.2} | {val_b_s:.2} | `{tr_b_l:.4}` | `{val_b_l:.4}` |

## 2. Native Real-Time CPU Performance (`48 kHz`, 128-Sample Quantum = `2666.7 us`)

- **Performance Score**: **{perf_score:.2} / 100.0** (`{headroom:.1}x` real-time polyphonic headroom)
- **1 Voice (2 Coupled Reeds + Body Filter)**: `{v1_us:.2} us / quantum` (`{v1_pct:.2}%` of real-time core)
- **16 Voices (32 Coupled Reeds + Body Filter)**: `{v16_us:.2} us / quantum` (`{v16_pct:.2}%` of real-time core, Budget: `< {budget:.0} us`)

## 3. Signed Physical Error Deltas (`Synth - Golden` on Train Set)

- **Harmonic Error H1..H8 (dB)**: `[{h_err}]`
- **Spectral Centroid Error (Hz)**: `p: {c_p:+.0} Hz`, `mf: {c_mf:+.0} Hz`, `f: {c_f:+.0} Hz`
- **Left-Hand (Bajos) 4'/8' Octave Ratio Error (dB)**: `p: {l_p:+.1}`, `mf: {l_mf:+.1}`, `f: {l_f:+.1}`
- **Right-Hand (Cantabile) 4'/8' Octave Ratio Error (dB)**: `p: {r_p:+.1}`, `mf: {r_mf:+.1}`, `f: {r_f:+.1}`
- **Attack Rise Time Error (ms)**: `Sustain: {rise_sus:+.1} ms`, `Staccato: {rise_stacc:+.1} ms`
- **Mechanical Pallet Click Error**: `{click:+.1} dB`
- **Octave Beating Frequency Error**: `{beat:+.2} Hz`, **Dual-Reed Onset Lag Error (`p`)**: `{lag:+.1} ms`
- **Pressure Pitch Bend (`p -> f`) Error**: `{drift:+.2} cents`
- **Body Formant Band Error (125Hz..4kHz, dB)**: `[{band_err}]`
- **Inter-Harmonic Air Turbulence Floor Error**: `{noise:+.1} dB`

## 4. Top Actionable Recommendations for Next Iteration

",
        preset_name = scorecard.preset_name,
        total_samples = scorecard.total_evaluated_samples,
        gate_str = if g.passed_all { "PASSED" } else { "FAILED" },
        no_nan = g.stability_no_nan_or_inf,
        silence = g.silence_convergence,
        budget = p.voice_16_budget_us,
        cpu_ok = g.cpu_budget_passed,
        tr_score = s.composite_train_score,
        val_score = s.composite_val_score,
        perf_score = p.performance_score,
        headroom = p.realtime_headroom_factor,
        tr_loss = s.composite_train_loss,
        val_loss = s.composite_val_loss,
        tr_h_s = s.train_categories.harmonic_profile.score,
        val_h_s = s.val_categories.harmonic_profile.score,
        tr_h_l = s.train_categories.harmonic_profile.loss,
        val_h_l = s.val_categories.harmonic_profile.loss,
        tr_t_s = s.train_categories.transients.score,
        val_t_s = s.val_categories.transients.score,
        tr_t_l = s.train_categories.transients.loss,
        val_t_l = s.val_categories.transients.loss,
        tr_d_s = s.train_categories.dual_reed.score,
        val_d_s = s.val_categories.dual_reed.score,
        tr_d_l = s.train_categories.dual_reed.loss,
        val_d_l = s.val_categories.dual_reed.loss,
        tr_p_s = s.train_categories.pitch_bend.score,
        val_p_s = s.val_categories.pitch_bend.score,
        tr_p_l = s.train_categories.pitch_bend.loss,
        val_p_l = s.val_categories.pitch_bend.loss,
        tr_b_s = s.train_categories.body_resonance.score,
        val_b_s = s.val_categories.body_resonance.score,
        tr_b_l = s.train_categories.body_resonance.loss,
        val_b_l = s.val_categories.body_resonance.loss,
        v1_us = p.voice_1_us_per_quantum,
        v1_pct = 100.0 * p.voice_1_us_per_quantum / p.realtime_quantum_us,
        v16_us = p.voice_16_us_per_quantum,
        v16_pct = p.core_utilization_16v_pct,
        h_err = d.h1_to_h8_err_db.iter().map(|v| format!("{v:+.1}")).collect::<Vec<_>>().join(", "),
        c_p = d.spectral_centroid_err_hz.p,
        c_mf = d.spectral_centroid_err_hz.mf,
        c_f = d.spectral_centroid_err_hz.f,
        l_p = d.octave_4ft_to_8ft_err_db_left.p,
        l_mf = d.octave_4ft_to_8ft_err_db_left.mf,
        l_f = d.octave_4ft_to_8ft_err_db_left.f,
        r_p = d.octave_4ft_to_8ft_err_db_right.p,
        r_mf = d.octave_4ft_to_8ft_err_db_right.mf,
        r_f = d.octave_4ft_to_8ft_err_db_right.f,
        rise_sus = d.attack_rise_err_ms_sustain,
        rise_stacc = d.attack_rise_err_ms_stacc,
        click = d.pallet_click_err_db,
        beat = d.beat_freq_err_hz,
        lag = d.dual_reed_onset_lag_err_ms,
        drift = d.drift_p_to_f_err_cents,
        band_err = d.body_band_err_db.iter().map(|v| format!("{v:+.1}")).collect::<Vec<_>>().join(", "),
        noise = d.air_noise_floor_err_db,
    );
    for (i, rec) in scorecard.top_actionable_recommendations.iter().enumerate() {
        md.push_str(&format!("{}. {}\n", i + 1, rec));
    }

    std::fs::write(&md_path, md)?;
    Ok(())
}

/// Prints detailed physical breakdown of Synth vs Golden across hand, dynamic, articulation, and pitch.
pub fn print_detailed_diagnostics(
    preset: &BandoneonPreset,
    golden_samples: &[CachedGoldenSample],
    sample_rate: u32,
) {
    let pairs: Vec<(&CachedGoldenSample, AcousticFeatures)> = golden_samples
        .par_iter()
        .map(|item| {
            let (synth, _) =
                simulate_and_extract(&item.meta, &item.features, preset, sample_rate, false);
            (item, synth)
        })
        .collect();

    let train_pairs: Vec<&(&CachedGoldenSample, AcousticFeatures)> =
        pairs.iter().filter(|(g, _)| g.meta.split == "train").collect();

    let n_tr = train_pairs.len().max(1) as f32;
    let mut sum_h_db_term = 0.0f32;
    let mut sum_cent_term = 0.0f32;
    let mut sum_rise_term = 0.0f32;
    let mut sum_click_term = 0.0f32;
    let mut sum_oct_term = 0.0f32;
    let mut sum_beat_term = 0.0f32;
    let mut sum_lag_term = 0.0f32;
    let mut sum_band_term = 0.0f32;
    let mut sum_noise_term = 0.0f32;
    let mut per_h_abs_err = [0.0f32; NUM_HARMONICS];
    let mut per_b_abs_err = [0.0f32; NUM_BODY_BANDS];

    for (item, synth) in &train_pairs {
        let g = &item.features;
        let mut h_err_sum = 0.0f32;
        let mut w_sum = 0.0f32;
        for k in 0..NUM_HARMONICS {
            let w = 1.0 / ((k as f32 + 1.0).powf(0.45));
            let diff = (synth.harmonics_rel_db[k] - g.harmonics_rel_db[k]).abs();
            h_err_sum += w * diff;
            w_sum += w;
            per_h_abs_err[k] += diff / n_tr;
        }
        sum_h_db_term += (h_err_sum / w_sum) / 18.0 / n_tr;
        sum_cent_term += 0.45
            * ((synth.spectral_centroid_hz - g.spectral_centroid_hz).abs()
                / g.spectral_centroid_hz.max(100.0))
            / n_tr;

        sum_rise_term += ((synth.attack_rise_ms - g.attack_rise_ms).abs() / 45.0) / n_tr;
        sum_click_term += ((synth.click_energy_db - g.click_energy_db).abs() / 24.0) / n_tr;

        sum_oct_term += ((synth.octave_4ft_to_8ft_db - g.octave_4ft_to_8ft_db).abs() / 14.0) / n_tr;
        sum_beat_term += 0.15 * ((synth.beat_freq_hz - g.beat_freq_hz).abs() / 2.5) / n_tr;
        sum_lag_term +=
            0.15 * ((synth.dual_reed_onset_lag_ms - g.dual_reed_onset_lag_ms).abs() / 35.0) / n_tr;

        let mut b_sum = 0.0f32;
        for b in 0..NUM_BODY_BANDS {
            let bd = (synth.octave_band_rel_db[b] - g.octave_band_rel_db[b]).abs();
            b_sum += bd;
            per_b_abs_err[b] += bd / n_tr;
        }
        sum_band_term += (b_sum / (NUM_BODY_BANDS as f32) / 14.0) / n_tr;
        sum_noise_term += 0.35 * ((synth.air_noise_floor_db - g.air_noise_floor_db).abs() / 25.0) / n_tr;
    }

    println!("=== DETAILED SYNTH vs GOLDEN DIAGNOSTICS (Train n={}) ===", train_pairs.len());
    println!(
        "Loss Sub-Term Breakdown (Train):\n  1. Harmonic ({:.4}): H_dB_err={:.4} (mean {:.1} dB), Centroid_rel_err={:.4}\n  2. Transient({:.4}): Rise_err={:.4} (mean {:.1} ms), Click_err={:.4} (mean {:.1} dB)\n  3. DualReed ({:.4}): Oct_err={:.4} (mean {:.1} dB), Beat_err={:.4}, Lag_err={:.4}\n  5. Body     ({:.4}): Band_err={:.4} (mean {:.1} dB), Noise_err={:.4} (mean {:.1} dB)",
        sum_h_db_term + sum_cent_term, sum_h_db_term, sum_h_db_term * 18.0, sum_cent_term,
        sum_rise_term + sum_click_term, sum_rise_term, sum_rise_term * 45.0, sum_click_term, sum_click_term * 24.0,
        sum_oct_term + sum_beat_term + sum_lag_term, sum_oct_term, sum_oct_term * 14.0, sum_beat_term, sum_lag_term,
        sum_band_term + sum_noise_term, sum_band_term, sum_band_term * 14.0, sum_noise_term, sum_noise_term * 25.0 / 0.35
    );
    println!(
        "  Mean Abs Error H1..H8  (dB): [{}]",
        per_h_abs_err[..8].iter().map(|v| format!("{:4.1}", v)).collect::<Vec<_>>().join(", ")
    );
    println!(
        "  Mean Abs Error H9..H16 (dB): [{}]",
        per_h_abs_err[8..].iter().map(|v| format!("{:4.1}", v)).collect::<Vec<_>>().join(", ")
    );
    println!(
        "  Mean Abs Error Bands 1..6 (dB): [{}]",
        per_b_abs_err.iter().map(|v| format!("{:4.1}", v)).collect::<Vec<_>>().join(", ")
    );
    for (idx, (item, synth)) in train_pairs.iter().take(12).enumerate() {
        println!(
            "  Sample {:2} {:16}: Rise G {:5.1} vs S {:5.1}ms | Click G {:+5.1} vs S {:+5.1}dB | 4'/8' G {:+5.1} vs S {:+5.1}dB | Cent G {:5.0} vs S {:5.0}Hz",
            idx, item.meta.id,
            item.features.attack_rise_ms, synth.attack_rise_ms,
            item.features.click_energy_db, synth.click_energy_db,
            item.features.octave_4ft_to_8ft_db, synth.octave_4ft_to_8ft_db,
            item.features.spectral_centroid_hz, synth.spectral_centroid_hz
        );
    }

    for hand in ["left_bajos", "right_cantabile"] {
        println!("\n-------------------------------------------------------------------------");
        println!(" MANUAL: {}", hand);
        println!("-------------------------------------------------------------------------");
        for art in ["sustain", "stacc"] {
            for dyn_mark in ["p", "mf", "f"] {
                let sub: Vec<_> = train_pairs
                    .iter()
                    .filter(|(g, _)| {
                        g.meta.hand == hand
                            && g.meta.articulation == art
                            && g.meta.dynamic == dyn_mark
                    })
                    .collect();
                if sub.is_empty() {
                    continue;
                }
                let n = sub.len() as f32;
                let avg = |f: &dyn Fn(&CachedGoldenSample, &AcousticFeatures) -> f32| -> f32 {
                    sub.iter().map(|(g, s)| f(g, s)).sum::<f32>() / n
                };

                let g_cent = avg(&|g, _| g.features.spectral_centroid_hz);
                let s_cent = avg(&|_, s| s.spectral_centroid_hz);
                let g_oct = avg(&|g, _| g.features.octave_4ft_to_8ft_db);
                let s_oct = avg(&|_, s| s.octave_4ft_to_8ft_db);
                let g_rise = avg(&|g, _| g.features.attack_rise_ms);
                let s_rise = avg(&|_, s| s.attack_rise_ms);
                let g_click = avg(&|g, _| g.features.click_energy_db);
                let s_click = avg(&|_, s| s.click_energy_db);
                let g_beat = avg(&|g, _| g.features.beat_freq_hz);
                let s_beat = avg(&|_, s| s.beat_freq_hz);
                let g_lag = avg(&|g, _| g.features.dual_reed_onset_lag_ms);
                let s_lag = avg(&|_, s| s.dual_reed_onset_lag_ms);
                let g_noise = avg(&|g, _| g.features.air_noise_floor_db);
                let s_noise = avg(&|_, s| s.air_noise_floor_db);

                println!(
                    "  [{:7} {:2} n={:2}] Cent: G {:6.0} vs S {:6.0}Hz | 4'/8': G {:+5.1} vs S {:+5.1}dB | Rise: G {:5.1} vs S {:5.1}ms | Click: G {:+5.1} vs S {:+5.1}dB | Beat: G {:.2} vs S {:.2}Hz | Lag: G {:+5.1} vs S {:+5.1}ms | Noise: G {:+5.1} vs S {:+5.1}dB",
                    art, dyn_mark, sub.len(),
                    g_cent, s_cent, g_oct, s_oct, g_rise, s_rise, g_click, s_click, g_beat, s_beat, g_lag, s_lag, g_noise, s_noise
                );

                let mut gh = [0.0f32; 8];
                let mut sh = [0.0f32; 8];
                for k in 0..8 {
                    gh[k] = avg(&|g, _| g.features.harmonics_rel_db[k]);
                    sh[k] = avg(&|_, s| s.harmonics_rel_db[k]);
                }
                println!(
                    "    H1..H8 Gold : [{}]",
                    gh.iter().map(|v| format!("{:5.1}", v)).collect::<Vec<_>>().join(", ")
                );
                println!(
                    "    H1..H8 Synth: [{}]",
                    sh.iter().map(|v| format!("{:5.1}", v)).collect::<Vec<_>>().join(", ")
                );
            }
        }

        // Pitch regions within manual (Low / Mid / High)
        let (n_min, n_max) = if hand == "left_bajos" { (36u8, 71u8) } else { (60u8, 96u8) };
        let span = (n_max - n_min) / 3;
        println!("  --- Pitch Regions (Sustain) ---");
        for (r_idx, r_name) in ["Low", "Mid", "High"].iter().enumerate() {
            let lo = n_min + (r_idx as u8) * span;
            let hi = if r_idx == 2 { n_max + 1 } else { lo + span };
            let sub: Vec<_> = train_pairs
                .iter()
                .filter(|(g, _)| {
                    g.meta.hand == hand
                        && g.meta.articulation == "sustain"
                        && g.meta.midi_note >= lo
                        && g.meta.midi_note < hi
                })
                .collect();
            if sub.is_empty() {
                continue;
            }
            let n = sub.len() as f32;
            let avg = |f: &dyn Fn(&CachedGoldenSample, &AcousticFeatures) -> f32| -> f32 {
                sub.iter().map(|(g, s)| f(g, s)).sum::<f32>() / n
            };
            println!(
                "  Region {:4} (MIDI {:2}..{:2}, n={:2}): Cent G {:5.0} / S {:5.0}Hz | 4'/8' G {:+5.1} / S {:+5.1}dB | Rise G {:5.1} / S {:5.1}ms | Click G {:+5.1} / S {:+5.1}dB | H1 G {:5.1}/S {:5.1}, H2 G {:5.1}/S {:5.1}, H3 G {:5.1}/S {:5.1}",
                r_name, lo, hi - 1, sub.len(),
                avg(&|g, _| g.features.spectral_centroid_hz),
                avg(&|_, s| s.spectral_centroid_hz),
                avg(&|g, _| g.features.octave_4ft_to_8ft_db),
                avg(&|_, s| s.octave_4ft_to_8ft_db),
                avg(&|g, _| g.features.attack_rise_ms),
                avg(&|_, s| s.attack_rise_ms),
                avg(&|g, _| g.features.click_energy_db),
                avg(&|_, s| s.click_energy_db),
                avg(&|g, _| g.features.harmonics_rel_db[0]),
                avg(&|_, s| s.harmonics_rel_db[0]),
                avg(&|g, _| g.features.harmonics_rel_db[1]),
                avg(&|_, s| s.harmonics_rel_db[1]),
                avg(&|g, _| g.features.harmonics_rel_db[2]),
                avg(&|_, s| s.harmonics_rel_db[2]),
            );
        }

        // Body bands by hand
        let sub_hand: Vec<_> = train_pairs.iter().filter(|(g, _)| g.meta.hand == hand).collect();
        let nh = sub_hand.len().max(1) as f32;
        let mut gb = [0.0f32; NUM_BODY_BANDS];
        let mut sb = [0.0f32; NUM_BODY_BANDS];
        for b in 0..NUM_BODY_BANDS {
            gb[b] = sub_hand.iter().map(|(g, _)| g.features.octave_band_rel_db[b]).sum::<f32>() / nh;
            sb[b] = sub_hand.iter().map(|(_, s)| s.octave_band_rel_db[b]).sum::<f32>() / nh;
        }
        println!(
            "  Body Bands (125..4k) Gold : [{}]",
            gb.iter().map(|v| format!("{:6.1}", v)).collect::<Vec<_>>().join(", ")
        );
        println!(
            "  Body Bands (125..4k) Synth: [{}]",
            sb.iter().map(|v| format!("{:6.1}", v)).collect::<Vec<_>>().join(", ")
        );
    }
}
