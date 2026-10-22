use crate::scorecard::{evaluate_preset, Scorecard};
use crate::sim::CachedGoldenSample;
use synth_core::BandoneonPreset;

/// Automated coordinate pattern-search / physical delta-guided tuner.
///
/// Optimizes the smooth physical scaling laws of `BandoneonPreset` against the `train` split
/// and verifies generalization on the held-out `val` split.
pub fn tune_preset(
    initial_preset: &BandoneonPreset,
    golden_samples: &[CachedGoldenSample],
    sample_rate: u32,
    rounds: usize,
) -> (BandoneonPreset, Scorecard) {
    let mut best_preset = initial_preset.clone();
    let mut best_card = evaluate_preset(&best_preset, golden_samples, sample_rate, false);

    println!(
        "[Tuner] Initial Score -> Train: {:.2} (loss {:.4}), Val: {:.2} (loss {:.4})",
        best_card.scores.composite_train_score,
        best_card.scores.composite_train_loss,
        best_card.scores.composite_val_score,
        best_card.scores.composite_val_loss
    );

    // Coordinate mutation closures operating on smooth physical scaling parameters
    type ParamMutator = Box<dyn Fn(&mut BandoneonPreset, f32)>;
    let mut mutators: Vec<(String, f32, ParamMutator)> = vec![
        (
            "left.slot_sharpness_base".into(),
            0.40,
            Box::new(|p, d| p.left_bajos.slot_sharpness_base = (p.left_bajos.slot_sharpness_base + d).clamp(0.5, 9.5)),
        ),
        (
            "left.slot_sharpness_slope".into(),
            0.40,
            Box::new(|p, d| p.left_bajos.slot_sharpness_slope = (p.left_bajos.slot_sharpness_slope + d).clamp(-5.0, 5.0)),
        ),
        (
            "right.slot_sharpness_base".into(),
            0.40,
            Box::new(|p, d| p.right_cantabile.slot_sharpness_base = (p.right_cantabile.slot_sharpness_base + d).clamp(0.5, 9.5)),
        ),
        (
            "right.slot_sharpness_slope".into(),
            0.40,
            Box::new(|p, d| p.right_cantabile.slot_sharpness_slope = (p.right_cantabile.slot_sharpness_slope + d).clamp(-5.0, 5.0)),
        ),
        (
            "left.octave_4ft_gain_base".into(),
            0.15,
            Box::new(|p, d| p.left_bajos.octave_4ft_gain_base = (p.left_bajos.octave_4ft_gain_base + d).clamp(0.05, 2.5)),
        ),
        (
            "left.octave_4ft_gain_slope".into(),
            0.15,
            Box::new(|p, d| p.left_bajos.octave_4ft_gain_slope = (p.left_bajos.octave_4ft_gain_slope + d).clamp(-2.5, 2.0)),
        ),
        (
            "left.octave_4ft_gain_curve".into(),
            0.15,
            Box::new(|p, d| p.left_bajos.octave_4ft_gain_curve = (p.left_bajos.octave_4ft_gain_curve + d).clamp(-2.5, 2.0)),
        ),
        (
            "right.octave_4ft_gain_base".into(),
            0.15,
            Box::new(|p, d| p.right_cantabile.octave_4ft_gain_base = (p.right_cantabile.octave_4ft_gain_base + d).clamp(0.05, 2.5)),
        ),
        (
            "right.octave_4ft_gain_slope".into(),
            0.15,
            Box::new(|p, d| p.right_cantabile.octave_4ft_gain_slope = (p.right_cantabile.octave_4ft_gain_slope + d).clamp(-2.5, 2.0)),
        ),
        (
            "right.octave_4ft_gain_curve".into(),
            0.15,
            Box::new(|p, d| p.right_cantabile.octave_4ft_gain_curve = (p.right_cantabile.octave_4ft_gain_curve + d).clamp(-2.5, 2.0)),
        ),
        (
            "left.octave_4ft_pressure_boost".into(),
            0.10,
            Box::new(|p, d| p.left_bajos.octave_4ft_pressure_boost = (p.left_bajos.octave_4ft_pressure_boost + d).clamp(0.0, 1.8)),
        ),
        (
            "right.octave_4ft_pressure_boost".into(),
            0.10,
            Box::new(|p, d| p.right_cantabile.octave_4ft_pressure_boost = (p.right_cantabile.octave_4ft_pressure_boost + d).clamp(0.0, 1.8)),
        ),
        (
            "left.reed_asymmetry_base".into(),
            0.06,
            Box::new(|p, d| p.left_bajos.reed_asymmetry_base = (p.left_bajos.reed_asymmetry_base + d).clamp(0.02, 0.95)),
        ),
        (
            "left.reed_asymmetry_slope".into(),
            0.06,
            Box::new(|p, d| p.left_bajos.reed_asymmetry_slope = (p.left_bajos.reed_asymmetry_slope + d).clamp(-0.8, 0.8)),
        ),
        (
            "right.reed_asymmetry_base".into(),
            0.06,
            Box::new(|p, d| p.right_cantabile.reed_asymmetry_base = (p.right_cantabile.reed_asymmetry_base + d).clamp(0.02, 0.95)),
        ),
        (
            "right.reed_asymmetry_slope".into(),
            0.06,
            Box::new(|p, d| p.right_cantabile.reed_asymmetry_slope = (p.right_cantabile.reed_asymmetry_slope + d).clamp(-0.8, 0.8)),
        ),
        (
            "left.baffle_cutoff_hz".into(),
            35.0,
            Box::new(|p, d| p.left_bajos.baffle_cutoff_hz = (p.left_bajos.baffle_cutoff_hz + d).clamp(120.0, 900.0)),
        ),
        (
            "right.baffle_cutoff_hz".into(),
            35.0,
            Box::new(|p, d| p.right_cantabile.baffle_cutoff_hz = (p.right_cantabile.baffle_cutoff_hz + d).clamp(120.0, 900.0)),
        ),
        (
            "left.jet_corner_base_hz".into(),
            220.0,
            Box::new(|p, d| p.left_bajos.jet_corner_base_hz = (p.left_bajos.jet_corner_base_hz + d).clamp(900.0, 8000.0)),
        ),
        (
            "left.jet_corner_slope_hz".into(),
            220.0,
            Box::new(|p, d| p.left_bajos.jet_corner_slope_hz = (p.left_bajos.jet_corner_slope_hz + d).clamp(-2500.0, 4000.0)),
        ),
        (
            "right.jet_corner_base_hz".into(),
            300.0,
            Box::new(|p, d| p.right_cantabile.jet_corner_base_hz = (p.right_cantabile.jet_corner_base_hz + d).clamp(1200.0, 9000.0)),
        ),
        (
            "right.jet_corner_slope_hz".into(),
            300.0,
            Box::new(|p, d| p.right_cantabile.jet_corner_slope_hz = (p.right_cantabile.jet_corner_slope_hz + d).clamp(-2000.0, 6000.0)),
        ),
        (
            "valve_open_ms".into(),
            1.2,
            Box::new(|p, d| p.valve_open_ms = (p.valve_open_ms + d).clamp(1.0, 35.0)),
        ),
        (
            "bellows_attack_ms".into(),
            1.0,
            Box::new(|p, d| p.bellows_attack_ms = (p.bellows_attack_ms + d).clamp(0.5, 30.0)),
        ),
        (
            "piston_coupling".into(),
            0.08,
            Box::new(|p, d| {
                p.left_bajos.piston_coupling_base = (p.left_bajos.piston_coupling_base + d).clamp(0.05, 0.95);
                p.right_cantabile.piston_coupling_base = (p.right_cantabile.piston_coupling_base + d).clamp(0.05, 0.95);
            }),
        ),
        (
            "pallet_click_gain".into(),
            0.0005,
            Box::new(|p, d| p.pallet_click_gain = (p.pallet_click_gain + d).clamp(0.0004, 0.15)),
        ),
        (
            "left.pallet_click_scale".into(),
            0.05,
            Box::new(|p, d| p.left_bajos.pallet_click_scale = (p.left_bajos.pallet_click_scale + d).clamp(0.05, 2.5)),
        ),
        (
            "right.pallet_click_scale".into(),
            0.08,
            Box::new(|p, d| p.right_cantabile.pallet_click_scale = (p.right_cantabile.pallet_click_scale + d).clamp(0.05, 2.5)),
        ),
        (
            "air_turbulence_gain".into(),
            0.004,
            Box::new(|p, d| p.air_turbulence_gain = (p.air_turbulence_gain + d).clamp(0.001, 0.10)),
        ),
        (
            "left.air_noise_base".into(),
            0.25,
            Box::new(|p, d| p.left_bajos.air_noise_base = (p.left_bajos.air_noise_base + d).clamp(0.1, 4.0)),
        ),
        (
            "right.air_noise_base".into(),
            0.15,
            Box::new(|p, d| p.right_cantabile.air_noise_base = (p.right_cantabile.air_noise_base + d).clamp(0.1, 4.0)),
        ),
        (
            "left.pressure_brightness_mod".into(),
            0.35,
            Box::new(|p, d| p.left_bajos.pressure_brightness_mod = (p.left_bajos.pressure_brightness_mod + d).clamp(0.0, 6.5)),
        ),
        (
            "right.pressure_brightness_mod".into(),
            0.35,
            Box::new(|p, d| p.right_cantabile.pressure_brightness_mod = (p.right_cantabile.pressure_brightness_mod + d).clamp(0.0, 6.5)),
        ),
        (
            "left.octave_beat_hz_base".into(),
            0.25,
            Box::new(|p, d| p.left_bajos.octave_beat_hz_base = (p.left_bajos.octave_beat_hz_base + d).clamp(0.2, 5.0)),
        ),
        (
            "right.octave_beat_hz_base".into(),
            0.25,
            Box::new(|p, d| p.right_cantabile.octave_beat_hz_base = (p.right_cantabile.octave_beat_hz_base + d).clamp(0.2, 5.0)),
        ),
        (
            "left.pitch_bend_cents".into(),
            1.5,
            Box::new(|p, d| p.left_bajos.pitch_bend_cents_per_pressure = (p.left_bajos.pitch_bend_cents_per_pressure + d).clamp(-28.0, 15.0)),
        ),
        (
            "left.pitch_bend_slope".into(),
            1.5,
            Box::new(|p, d| p.left_bajos.pitch_bend_slope = (p.left_bajos.pitch_bend_slope + d).clamp(-20.0, 28.0)),
        ),
        (
            "right.pitch_bend_cents".into(),
            1.0,
            Box::new(|p, d| p.right_cantabile.pitch_bend_cents_per_pressure = (p.right_cantabile.pitch_bend_cents_per_pressure + d).clamp(-20.0, 15.0)),
        ),
    ];

    for i in 0..6 {
        mutators.push((
            format!("body_mode_{i}"),
            1.5,
            Box::new(move |p, d| p.body_modes[i].gain_db = (p.body_modes[i].gain_db + d).clamp(-15.0, 15.0)),
        ));
        mutators.push((
            format!("left_body_{i}"),
            1.5,
            Box::new(move |p, d| p.left_body_modes[i].gain_db = (p.left_body_modes[i].gain_db + d).clamp(-15.0, 15.0)),
        ));
    }

    for round in 0..rounds {
        let mut improved_in_round = false;
        let step_scale = 0.72f32.powi(round as i32);

        for (name, base_step, mutate) in &mut mutators {
            let step = *base_step * step_scale;
            for dir in [1.0f32, -1.0f32] {
                let mut cand = best_preset.clone();
                mutate(&mut cand, dir * step);
                let card = evaluate_preset(&cand, golden_samples, sample_rate, false);

                // Accept only if train loss improves AND validation loss does not regress beyond tolerance
                let train_better =
                    card.scores.composite_train_loss + 1e-4 < best_card.scores.composite_train_loss;
                let val_ok =
                    card.scores.composite_val_loss <= best_card.scores.composite_val_loss + 0.002;

                if train_better && val_ok {
                    best_preset = cand;
                    best_card = card;
                    improved_in_round = true;
                    println!(
                        "  [Round {}] {:26} ({:+.3}) -> Train: {:.2}, Val: {:.2}",
                        round + 1,
                        name,
                        dir * step,
                        best_card.scores.composite_train_score,
                        best_card.scores.composite_val_score
                    );
                    break;
                }
            }
        }
        if !improved_in_round {
            println!("  [Round {}] Pattern search converged.", round + 1);
            break;
        }
    }

    let final_card = evaluate_preset(&best_preset, golden_samples, sample_rate, true);
    (best_preset, final_card)
}
