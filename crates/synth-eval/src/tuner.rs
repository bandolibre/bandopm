use crate::scorecard::{evaluate_preset, Scorecard};
use crate::sim::CachedGoldenSample;
use synth_core::{BandoneonPreset, SynthVersion};

/// Automated coordinate pattern-search / physical delta-guided tuner.
///
/// Optimizes the smooth physical scaling laws of `BandoneonPreset` against the `train` split
/// and verifies generalization on the held-out `val` split.
pub fn tune_preset(
    initial_preset: &BandoneonPreset,
    golden_samples: &[CachedGoldenSample],
    sample_rate: u32,
    rounds: usize,
    only: &[String],
) -> (BandoneonPreset, Scorecard) {
    let mut best_preset = initial_preset.clone();
    let mut best_card = evaluate_preset(&best_preset, golden_samples, sample_rate, false);

    println!(
        "[Tuner] Initial Score ({}) -> Train: {:.2} (loss {:.4}), Val: {:.2} (loss {:.4})",
        best_preset.version.slug(),
        best_card.scores.composite_train_score,
        best_card.scores.composite_train_loss,
        best_card.scores.composite_val_score,
        best_card.scores.composite_val_loss
    );

    let ver = best_preset.version;
    let use_aero = ver.use_v3_aero();
    let use_beam = ver.use_v3_beam();
    let use_tran = ver.use_v3_tran();
    let use_v4_cab = ver.use_v4_cabinet();
    let use_v4_slot = ver.use_v4_slot();
    let use_v4_coupled = ver.use_v4_coupled();
    let use_v4_celda = ver.use_v4_celda();
    let use_v4_zap = ver.use_v4_zapatilla();

    // Coordinate mutation closures operating on smooth physical scaling parameters
    type ParamMutator = Box<dyn Fn(&mut BandoneonPreset, f32)>;
    let mut mutators: Vec<(String, f32, ParamMutator)> = Vec::new();

    let is_legacy = matches!(ver, SynthVersion::V1Baseline | SynthVersion::V2AdaaBaffle);

    if use_v4_cab {
        for i in 0..4 {
            mutators.push((
                format!("left_cab_gain_{i}"),
                0.12,
                Box::new(move |p, d| {
                    p.left_cabinet_modes[i].gain_db =
                        (p.left_cabinet_modes[i].gain_db + d).clamp(-15.0, 15.0)
                }),
            ));
            mutators.push((
                format!("right_cab_gain_{i}"),
                0.12,
                Box::new(move |p, d| {
                    p.right_cabinet_modes[i].gain_db =
                        (p.right_cabinet_modes[i].gain_db + d).clamp(-15.0, 15.0)
                }),
            ));
            mutators.push((
                format!("left_cab_q_{i}"),
                0.20,
                Box::new(move |p, d| {
                    p.left_cabinet_modes[i].q = (p.left_cabinet_modes[i].q + d).clamp(0.7, 12.0)
                }),
            ));
            mutators.push((
                format!("right_cab_q_{i}"),
                0.20,
                Box::new(move |p, d| {
                    p.right_cabinet_modes[i].q = (p.right_cabinet_modes[i].q + d).clamp(0.7, 12.0)
                }),
            ));
        }
        mutators.push((
            "cabinet_ir_mix_left".into(),
            0.006,
            Box::new(|p, d| p.cabinet_ir_mix_left = (p.cabinet_ir_mix_left + d).clamp(-0.30, 0.30)),
        ));
        mutators.push((
            "cabinet_ir_mix_right".into(),
            0.006,
            Box::new(|p, d| {
                p.cabinet_ir_mix_right = (p.cabinet_ir_mix_right + d).clamp(-0.30, 0.30)
            }),
        ));
        for i in 0..6 {
            mutators.push((
                format!("body_mode_{i}"),
                0.16,
                Box::new(move |p, d| {
                    p.body_modes[i].gain_db = (p.body_modes[i].gain_db + d).clamp(-15.0, 15.0)
                }),
            ));
            mutators.push((
                format!("left_body_{i}"),
                0.16,
                Box::new(move |p, d| {
                    p.left_body_modes[i].gain_db =
                        (p.left_body_modes[i].gain_db + d).clamp(-15.0, 15.0)
                }),
            ));
        }
    }

    if use_v4_slot {
        let slot_muts: Vec<(String, f32, ParamMutator)> = vec![
            (
                "left.slot_pinch_adaa".into(),
                0.010,
                Box::new(|p, d| {
                    p.left_bajos.slot_pinch_adaa =
                        (p.left_bajos.slot_pinch_adaa + d).clamp(-0.35, 0.75)
                }),
            ),
            (
                "left.slot_channel_notch".into(),
                0.008,
                Box::new(|p, d| {
                    p.left_bajos.slot_channel_notch =
                        (p.left_bajos.slot_channel_notch + d).clamp(-0.25, 0.38)
                }),
            ),
            (
                "left.slot_emergence_swell".into(),
                0.010,
                Box::new(|p, d| {
                    p.left_bajos.slot_emergence_swell =
                        (p.left_bajos.slot_emergence_swell + d).clamp(-0.25, 0.45)
                }),
            ),
            (
                "left.low_bass_h2_tame".into(),
                0.015,
                Box::new(|p, d| {
                    p.left_bajos.low_bass_h2_tame =
                        (p.left_bajos.low_bass_h2_tame + d).clamp(-0.35, 0.75)
                }),
            ),
            (
                "right.slot_pinch_adaa".into(),
                0.010,
                Box::new(|p, d| {
                    p.right_cantabile.slot_pinch_adaa =
                        (p.right_cantabile.slot_pinch_adaa + d).clamp(-0.35, 0.75)
                }),
            ),
            (
                "right.slot_channel_notch".into(),
                0.008,
                Box::new(|p, d| {
                    p.right_cantabile.slot_channel_notch =
                        (p.right_cantabile.slot_channel_notch + d).clamp(-0.25, 0.38)
                }),
            ),
            (
                "right.slot_emergence_swell".into(),
                0.010,
                Box::new(|p, d| {
                    p.right_cantabile.slot_emergence_swell =
                        (p.right_cantabile.slot_emergence_swell + d).clamp(-0.25, 0.45)
                }),
            ),
            (
                "right.low_bass_h2_tame".into(),
                0.015,
                Box::new(|p, d| {
                    p.right_cantabile.low_bass_h2_tame =
                        (p.right_cantabile.low_bass_h2_tame + d).clamp(-0.35, 0.75)
                }),
            ),
        ];
        mutators.extend(slot_muts);
    }

    if use_v4_coupled {
        let coupled_muts: Vec<(String, f32, ParamMutator)> = vec![
            (
                "left.coupled_ode_momentum".into(),
                0.012,
                Box::new(|p, d| {
                    p.left_bajos.coupled_ode_momentum =
                        (p.left_bajos.coupled_ode_momentum + d).clamp(-0.35, 0.55)
                }),
            ),
            (
                "left.startup_chirp_cents".into(),
                0.28,
                Box::new(|p, d| {
                    p.left_bajos.startup_chirp_cents =
                        (p.left_bajos.startup_chirp_cents + d).clamp(-12.0, 12.0)
                }),
            ),
            (
                "left.celda_cross_mod".into(),
                0.010,
                Box::new(|p, d| {
                    p.left_bajos.celda_cross_mod =
                        (p.left_bajos.celda_cross_mod + d).clamp(-0.30, 0.30)
                }),
            ),
            (
                "left.hopf_onset_lead".into(),
                0.018,
                Box::new(|p, d| {
                    p.left_bajos.hopf_onset_lead =
                        (p.left_bajos.hopf_onset_lead + d).clamp(-0.45, 0.85)
                }),
            ),
            (
                "right.coupled_ode_momentum".into(),
                0.012,
                Box::new(|p, d| {
                    p.right_cantabile.coupled_ode_momentum =
                        (p.right_cantabile.coupled_ode_momentum + d).clamp(-0.35, 0.55)
                }),
            ),
            (
                "right.startup_chirp_cents".into(),
                0.28,
                Box::new(|p, d| {
                    p.right_cantabile.startup_chirp_cents =
                        (p.right_cantabile.startup_chirp_cents + d).clamp(-12.0, 12.0)
                }),
            ),
            (
                "right.celda_cross_mod".into(),
                0.010,
                Box::new(|p, d| {
                    p.right_cantabile.celda_cross_mod =
                        (p.right_cantabile.celda_cross_mod + d).clamp(-0.30, 0.30)
                }),
            ),
            (
                "right.hopf_onset_lead".into(),
                0.018,
                Box::new(|p, d| {
                    p.right_cantabile.hopf_onset_lead =
                        (p.right_cantabile.hopf_onset_lead + d).clamp(-0.45, 0.85)
                }),
            ),
        ];
        mutators.extend(coupled_muts);
    }

    if use_v4_celda {
        let celda_muts: Vec<(String, f32, ParamMutator)> = vec![
            (
                "left.celda_cavity_gain_db".into(),
                0.12,
                Box::new(|p, d| {
                    p.left_bajos.celda_cavity_gain_db =
                        (p.left_bajos.celda_cavity_gain_db + d).clamp(-10.0, 10.0)
                }),
            ),
            (
                "left.celda_cavity_gain_slope_db".into(),
                0.12,
                Box::new(|p, d| {
                    p.left_bajos.celda_cavity_gain_slope_db =
                        (p.left_bajos.celda_cavity_gain_slope_db + d).clamp(-10.0, 10.0)
                }),
            ),
            (
                "right.celda_cavity_gain_db".into(),
                0.12,
                Box::new(|p, d| {
                    p.right_cantabile.celda_cavity_gain_db =
                        (p.right_cantabile.celda_cavity_gain_db + d).clamp(-10.0, 10.0)
                }),
            ),
            (
                "right.celda_cavity_gain_slope_db".into(),
                0.12,
                Box::new(|p, d| {
                    p.right_cantabile.celda_cavity_gain_slope_db =
                        (p.right_cantabile.celda_cavity_gain_slope_db + d).clamp(-10.0, 10.0)
                }),
            ),
            (
                "left.celda_cavity_hz_base".into(),
                45.0,
                Box::new(|p, d| {
                    p.left_bajos.celda_cavity_hz_base =
                        (p.left_bajos.celda_cavity_hz_base + d).clamp(250.0, 4500.0)
                }),
            ),
            (
                "left.celda_cavity_oct_slope".into(),
                0.08,
                Box::new(|p, d| {
                    p.left_bajos.celda_cavity_oct_slope =
                        (p.left_bajos.celda_cavity_oct_slope + d).clamp(-0.8, 2.8)
                }),
            ),
            (
                "left.celda_cavity_q".into(),
                0.18,
                Box::new(|p, d| {
                    p.left_bajos.celda_cavity_q =
                        (p.left_bajos.celda_cavity_q + d).clamp(0.7, 6.0)
                }),
            ),
            (
                "right.celda_cavity_hz_base".into(),
                65.0,
                Box::new(|p, d| {
                    p.right_cantabile.celda_cavity_hz_base =
                        (p.right_cantabile.celda_cavity_hz_base + d).clamp(500.0, 8000.0)
                }),
            ),
            (
                "right.celda_cavity_oct_slope".into(),
                0.08,
                Box::new(|p, d| {
                    p.right_cantabile.celda_cavity_oct_slope =
                        (p.right_cantabile.celda_cavity_oct_slope + d).clamp(-0.8, 2.8)
                }),
            ),
            (
                "right.celda_cavity_q".into(),
                0.18,
                Box::new(|p, d| {
                    p.right_cantabile.celda_cavity_q =
                        (p.right_cantabile.celda_cavity_q + d).clamp(0.7, 6.0)
                }),
            ),
        ];
        mutators.extend(celda_muts);
    }

    if use_v4_zap {
        let zap_muts: Vec<(String, f32, ParamMutator)> = vec![
            (
                "left.valve_occlusion_muffle".into(),
                0.025,
                Box::new(|p, d| {
                    p.left_bajos.valve_occlusion_muffle =
                        (p.left_bajos.valve_occlusion_muffle + d).clamp(0.0, 0.85)
                }),
            ),
            (
                "left.pallet_click_u_slope".into(),
                0.035,
                Box::new(|p, d| {
                    p.left_bajos.pallet_click_u_slope =
                        (p.left_bajos.pallet_click_u_slope + d).clamp(-1.4, 1.4)
                }),
            ),
            (
                "left.zapatilla_thump_gain".into(),
                0.15,
                Box::new(|p, d| {
                    p.left_bajos.zapatilla_thump_gain =
                        (p.left_bajos.zapatilla_thump_gain + d).clamp(0.0, 2.5)
                }),
            ),
            (
                "right.valve_occlusion_muffle".into(),
                0.025,
                Box::new(|p, d| {
                    p.right_cantabile.valve_occlusion_muffle =
                        (p.right_cantabile.valve_occlusion_muffle + d).clamp(0.0, 0.85)
                }),
            ),
            (
                "right.pallet_click_u_slope".into(),
                0.035,
                Box::new(|p, d| {
                    p.right_cantabile.pallet_click_u_slope =
                        (p.right_cantabile.pallet_click_u_slope + d).clamp(-1.4, 1.4)
                }),
            ),
            (
                "right.zapatilla_thump_gain".into(),
                0.15,
                Box::new(|p, d| {
                    p.right_cantabile.zapatilla_thump_gain =
                        (p.right_cantabile.zapatilla_thump_gain + d).clamp(0.0, 2.5)
                }),
            ),
            (
                "pallet_click_gain".into(),
                0.0002,
                Box::new(|p, d| p.pallet_click_gain = (p.pallet_click_gain + d).clamp(0.0004, 0.15)),
            ),
            (
                "left.pallet_click_scale".into(),
                0.025,
                Box::new(|p, d| {
                    p.left_bajos.pallet_click_scale =
                        (p.left_bajos.pallet_click_scale + d).clamp(0.05, 2.5)
                }),
            ),
            (
                "right.pallet_click_scale".into(),
                0.04,
                Box::new(|p, d| {
                    p.right_cantabile.pallet_click_scale =
                        (p.right_cantabile.pallet_click_scale + d).clamp(0.05, 2.5)
                }),
            ),
            (
                "valve_open_ms".into(),
                0.25,
                Box::new(|p, d| p.valve_open_ms = (p.valve_open_ms + d).clamp(1.0, 35.0)),
            ),
            (
                "bellows_attack_ms".into(),
                0.25,
                Box::new(|p, d| p.bellows_attack_ms = (p.bellows_attack_ms + d).clamp(0.5, 30.0)),
            ),
            (
                "left.staccato_chiff_gain".into(),
                0.15,
                Box::new(|p, d| {
                    p.left_bajos.staccato_chiff_gain =
                        (p.left_bajos.staccato_chiff_gain + d).clamp(0.0, 4.5)
                }),
            ),
            (
                "right.staccato_chiff_gain".into(),
                0.15,
                Box::new(|p, d| {
                    p.right_cantabile.staccato_chiff_gain =
                        (p.right_cantabile.staccato_chiff_gain + d).clamp(0.0, 4.5)
                }),
            ),
        ];
        mutators.extend(zap_muts);
    }

    if use_aero || is_legacy || use_v4_cab {
        if use_aero || use_v4_cab {
            let aero_muts: Vec<(String, f32, ParamMutator)> = vec![
                (
                    "left.monopole_rad_boost".into(),
                    0.04,
                    Box::new(|p, d| p.left_bajos.monopole_rad_boost = (p.left_bajos.monopole_rad_boost + d).clamp(-0.4, 2.0)),
                ),
                (
                    "left.monopole_rad_slope".into(),
                    0.04,
                    Box::new(|p, d| p.left_bajos.monopole_rad_slope = (p.left_bajos.monopole_rad_slope + d).clamp(-0.8, 1.5)),
                ),
                (
                    "left.dipole_baffle_mix".into(),
                    0.03,
                    Box::new(|p, d| p.left_bajos.dipole_baffle_mix = (p.left_bajos.dipole_baffle_mix + d).clamp(-0.5, 0.9)),
                ),
                (
                    "right.monopole_rad_boost".into(),
                    0.04,
                    Box::new(|p, d| p.right_cantabile.monopole_rad_boost = (p.right_cantabile.monopole_rad_boost + d).clamp(-0.4, 2.0)),
                ),
                (
                    "right.monopole_rad_slope".into(),
                    0.04,
                    Box::new(|p, d| p.right_cantabile.monopole_rad_slope = (p.right_cantabile.monopole_rad_slope + d).clamp(-0.8, 1.5)),
                ),
                (
                    "right.dipole_baffle_mix".into(),
                    0.03,
                    Box::new(|p, d| p.right_cantabile.dipole_baffle_mix = (p.right_cantabile.dipole_baffle_mix + d).clamp(-0.5, 0.9)),
                ),
            ];
            mutators.extend(aero_muts);
        }

        let aero_coupled: Vec<(String, f32, ParamMutator)> = vec![
            (
                "left.baffle_cutoff_hz".into(),
                20.0,
                Box::new(|p, d| p.left_bajos.baffle_cutoff_hz = (p.left_bajos.baffle_cutoff_hz + d).clamp(120.0, 900.0)),
            ),
            (
                "right.baffle_cutoff_hz".into(),
                20.0,
                Box::new(|p, d| p.right_cantabile.baffle_cutoff_hz = (p.right_cantabile.baffle_cutoff_hz + d).clamp(120.0, 900.0)),
            ),
            (
                "left.jet_corner_base_hz".into(),
                150.0,
                Box::new(|p, d| p.left_bajos.jet_corner_base_hz = (p.left_bajos.jet_corner_base_hz + d).clamp(900.0, 8000.0)),
            ),
            (
                "left.jet_corner_slope_hz".into(),
                150.0,
                Box::new(|p, d| p.left_bajos.jet_corner_slope_hz = (p.left_bajos.jet_corner_slope_hz + d).clamp(-2500.0, 4000.0)),
            ),
            (
                "right.jet_corner_base_hz".into(),
                180.0,
                Box::new(|p, d| p.right_cantabile.jet_corner_base_hz = (p.right_cantabile.jet_corner_base_hz + d).clamp(1200.0, 9000.0)),
            ),
            (
                "right.jet_corner_slope_hz".into(),
                180.0,
                Box::new(|p, d| p.right_cantabile.jet_corner_slope_hz = (p.right_cantabile.jet_corner_slope_hz + d).clamp(-2000.0, 6000.0)),
            ),
        ];
        mutators.extend(aero_coupled);

        if !use_v4_cab {
            for i in 0..6 {
                mutators.push((
                    format!("body_mode_{i}"),
                    0.5,
                    Box::new(move |p, d| p.body_modes[i].gain_db = (p.body_modes[i].gain_db + d).clamp(-15.0, 15.0)),
                ));
                mutators.push((
                    format!("left_body_{i}"),
                    0.5,
                    Box::new(move |p, d| p.left_body_modes[i].gain_db = (p.left_body_modes[i].gain_db + d).clamp(-15.0, 15.0)),
                ));
            }
        }
    }

    if use_beam || is_legacy || use_v4_slot || use_v4_celda {
        if use_beam || use_v4_slot {
            let beam_muts: Vec<(String, f32, ParamMutator)> = vec![
                (
                    "left.duty_bias_base".into(),
                    0.012,
                    Box::new(|p, d| p.left_bajos.duty_bias_base = (p.left_bajos.duty_bias_base + d).clamp(-0.20, 0.45)),
                ),
                (
                    "left.clamped_bar_h6_gain".into(),
                    0.006,
                    Box::new(|p, d| p.left_bajos.clamped_bar_h6_gain = (p.left_bajos.clamped_bar_h6_gain + d).clamp(-0.20, 0.25)),
                ),
                (
                    "left.h3_notch_inject".into(),
                    0.008,
                    Box::new(|p, d| p.left_bajos.h3_notch_inject = (p.left_bajos.h3_notch_inject + d).clamp(-0.20, 0.25)),
                ),
                (
                    "right.duty_bias_base".into(),
                    0.012,
                    Box::new(|p, d| p.right_cantabile.duty_bias_base = (p.right_cantabile.duty_bias_base + d).clamp(-0.20, 0.45)),
                ),
                (
                    "right.clamped_bar_h6_gain".into(),
                    0.006,
                    Box::new(|p, d| p.right_cantabile.clamped_bar_h6_gain = (p.right_cantabile.clamped_bar_h6_gain + d).clamp(-0.20, 0.25)),
                ),
                (
                    "right.h3_notch_inject".into(),
                    0.008,
                    Box::new(|p, d| p.right_cantabile.h3_notch_inject = (p.right_cantabile.h3_notch_inject + d).clamp(-0.20, 0.25)),
                ),
            ];
            mutators.extend(beam_muts);
        }

        let beam_coupled: Vec<(String, f32, ParamMutator)> = vec![
            (
                "left.slot_sharpness_base".into(),
                0.08,
                Box::new(|p, d| p.left_bajos.slot_sharpness_base = (p.left_bajos.slot_sharpness_base + d).clamp(0.5, 9.5)),
            ),
            (
                "left.slot_sharpness_slope".into(),
                0.08,
                Box::new(|p, d| p.left_bajos.slot_sharpness_slope = (p.left_bajos.slot_sharpness_slope + d).clamp(-5.0, 5.0)),
            ),
            (
                "right.slot_sharpness_base".into(),
                0.08,
                Box::new(|p, d| p.right_cantabile.slot_sharpness_base = (p.right_cantabile.slot_sharpness_base + d).clamp(0.5, 9.5)),
            ),
            (
                "right.slot_sharpness_slope".into(),
                0.08,
                Box::new(|p, d| p.right_cantabile.slot_sharpness_slope = (p.right_cantabile.slot_sharpness_slope + d).clamp(-5.0, 5.0)),
            ),
            (
                "left.reed_asymmetry_base".into(),
                0.015,
                Box::new(|p, d| p.left_bajos.reed_asymmetry_base = (p.left_bajos.reed_asymmetry_base + d).clamp(0.02, 0.95)),
            ),
            (
                "left.reed_asymmetry_slope".into(),
                0.015,
                Box::new(|p, d| p.left_bajos.reed_asymmetry_slope = (p.left_bajos.reed_asymmetry_slope + d).clamp(-0.8, 0.8)),
            ),
            (
                "right.reed_asymmetry_base".into(),
                0.015,
                Box::new(|p, d| p.right_cantabile.reed_asymmetry_base = (p.right_cantabile.reed_asymmetry_base + d).clamp(0.02, 0.95)),
            ),
            (
                "right.reed_asymmetry_slope".into(),
                0.015,
                Box::new(|p, d| p.right_cantabile.reed_asymmetry_slope = (p.right_cantabile.reed_asymmetry_slope + d).clamp(-0.8, 0.8)),
            ),
            (
                "left.pressure_brightness_mod".into(),
                0.08,
                Box::new(|p, d| p.left_bajos.pressure_brightness_mod = (p.left_bajos.pressure_brightness_mod + d).clamp(0.0, 6.5)),
            ),
            (
                "right.pressure_brightness_mod".into(),
                0.08,
                Box::new(|p, d| p.right_cantabile.pressure_brightness_mod = (p.right_cantabile.pressure_brightness_mod + d).clamp(0.0, 6.5)),
            ),
        ];
        mutators.extend(beam_coupled);
    }

    if use_tran || is_legacy || use_v4_coupled {
        if use_tran || use_v4_coupled {
            let tran_muts: Vec<(String, f32, ParamMutator)> = vec![
                (
                    "left.hopf_8ft_knee".into(),
                    0.025,
                    Box::new(|p, d| p.left_bajos.hopf_8ft_knee = (p.left_bajos.hopf_8ft_knee + d).clamp(0.0, 0.65)),
                ),
                (
                    "left.staccato_4ft_boost".into(),
                    0.04,
                    Box::new(|p, d| p.left_bajos.staccato_4ft_boost = (p.left_bajos.staccato_4ft_boost + d).clamp(-0.3, 1.2)),
                ),
                (
                    "left.staccato_chiff_gain".into(),
                    0.18,
                    Box::new(|p, d| p.left_bajos.staccato_chiff_gain = (p.left_bajos.staccato_chiff_gain + d).clamp(0.0, 4.5)),
                ),
                (
                    "right.hopf_8ft_knee".into(),
                    0.025,
                    Box::new(|p, d| p.right_cantabile.hopf_8ft_knee = (p.right_cantabile.hopf_8ft_knee + d).clamp(0.0, 0.65)),
                ),
                (
                    "right.staccato_4ft_boost".into(),
                    0.04,
                    Box::new(|p, d| p.right_cantabile.staccato_4ft_boost = (p.right_cantabile.staccato_4ft_boost + d).clamp(-0.3, 1.2)),
                ),
                (
                    "right.staccato_chiff_gain".into(),
                    0.18,
                    Box::new(|p, d| p.right_cantabile.staccato_chiff_gain = (p.right_cantabile.staccato_chiff_gain + d).clamp(0.0, 4.5)),
                ),
                (
                    "left.reed_damping_base".into(),
                    0.010,
                    Box::new(|p, d| p.left_bajos.reed_damping_base = (p.left_bajos.reed_damping_base + d).clamp(0.01, 0.25)),
                ),
                (
                    "right.reed_damping_base".into(),
                    0.010,
                    Box::new(|p, d| p.right_cantabile.reed_damping_base = (p.right_cantabile.reed_damping_base + d).clamp(0.01, 0.25)),
                ),
                (
                    "left.p_th_8ft_base".into(),
                    0.003,
                    Box::new(|p, d| p.left_bajos.p_th_8ft_base = (p.left_bajos.p_th_8ft_base + d).clamp(0.008, 0.08)),
                ),
                (
                    "left.p_th_4ft_base".into(),
                    0.003,
                    Box::new(|p, d| p.left_bajos.p_th_4ft_base = (p.left_bajos.p_th_4ft_base + d).clamp(0.008, 0.08)),
                ),
                (
                    "right.p_th_8ft_base".into(),
                    0.003,
                    Box::new(|p, d| p.right_cantabile.p_th_8ft_base = (p.right_cantabile.p_th_8ft_base + d).clamp(0.008, 0.08)),
                ),
                (
                    "right.p_th_4ft_base".into(),
                    0.003,
                    Box::new(|p, d| p.right_cantabile.p_th_4ft_base = (p.right_cantabile.p_th_4ft_base + d).clamp(0.008, 0.08)),
                ),
            ];
            mutators.extend(tran_muts);
        }

        let tran_coupled: Vec<(String, f32, ParamMutator)> = vec![
            (
                "left.octave_4ft_gain_base".into(),
                0.08,
                Box::new(|p, d| p.left_bajos.octave_4ft_gain_base = (p.left_bajos.octave_4ft_gain_base + d).clamp(0.05, 2.5)),
            ),
            (
                "left.octave_4ft_gain_slope".into(),
                0.08,
                Box::new(|p, d| p.left_bajos.octave_4ft_gain_slope = (p.left_bajos.octave_4ft_gain_slope + d).clamp(-2.5, 2.0)),
            ),
            (
                "left.octave_4ft_gain_curve".into(),
                0.08,
                Box::new(|p, d| p.left_bajos.octave_4ft_gain_curve = (p.left_bajos.octave_4ft_gain_curve + d).clamp(-2.5, 2.0)),
            ),
            (
                "right.octave_4ft_gain_base".into(),
                0.08,
                Box::new(|p, d| p.right_cantabile.octave_4ft_gain_base = (p.right_cantabile.octave_4ft_gain_base + d).clamp(0.05, 2.5)),
            ),
            (
                "right.octave_4ft_gain_slope".into(),
                0.08,
                Box::new(|p, d| p.right_cantabile.octave_4ft_gain_slope = (p.right_cantabile.octave_4ft_gain_slope + d).clamp(-2.5, 2.0)),
            ),
            (
                "right.octave_4ft_gain_curve".into(),
                0.08,
                Box::new(|p, d| p.right_cantabile.octave_4ft_gain_curve = (p.right_cantabile.octave_4ft_gain_curve + d).clamp(-2.5, 2.0)),
            ),
            (
                "left.octave_4ft_pressure_boost".into(),
                0.06,
                Box::new(|p, d| p.left_bajos.octave_4ft_pressure_boost = (p.left_bajos.octave_4ft_pressure_boost + d).clamp(0.0, 1.8)),
            ),
            (
                "right.octave_4ft_pressure_boost".into(),
                0.06,
                Box::new(|p, d| p.right_cantabile.octave_4ft_pressure_boost = (p.right_cantabile.octave_4ft_pressure_boost + d).clamp(0.0, 1.8)),
            ),
            (
                "valve_open_ms".into(),
                0.3,
                Box::new(|p, d| p.valve_open_ms = (p.valve_open_ms + d).clamp(1.0, 35.0)),
            ),
            (
                "bellows_attack_ms".into(),
                0.3,
                Box::new(|p, d| p.bellows_attack_ms = (p.bellows_attack_ms + d).clamp(0.5, 30.0)),
            ),
            (
                "left.piston_coupling".into(),
                0.04,
                Box::new(|p, d| p.left_bajos.piston_coupling_base = (p.left_bajos.piston_coupling_base + d).clamp(0.05, 0.95)),
            ),
            (
                "right.piston_coupling".into(),
                0.04,
                Box::new(|p, d| p.right_cantabile.piston_coupling_base = (p.right_cantabile.piston_coupling_base + d).clamp(0.05, 0.95)),
            ),
            (
                "pallet_click_gain".into(),
                0.0002,
                Box::new(|p, d| p.pallet_click_gain = (p.pallet_click_gain + d).clamp(0.0004, 0.15)),
            ),
            (
                "left.pallet_click_scale".into(),
                0.025,
                Box::new(|p, d| p.left_bajos.pallet_click_scale = (p.left_bajos.pallet_click_scale + d).clamp(0.05, 2.5)),
            ),
            (
                "right.pallet_click_scale".into(),
                0.04,
                Box::new(|p, d| p.right_cantabile.pallet_click_scale = (p.right_cantabile.pallet_click_scale + d).clamp(0.05, 2.5)),
            ),
            (
                "air_turbulence_gain".into(),
                0.0025,
                Box::new(|p, d| p.air_turbulence_gain = (p.air_turbulence_gain + d).clamp(0.001, 0.10)),
            ),
            (
                "left.air_noise_base".into(),
                0.15,
                Box::new(|p, d| p.left_bajos.air_noise_base = (p.left_bajos.air_noise_base + d).clamp(0.1, 4.0)),
            ),
            (
                "right.air_noise_base".into(),
                0.10,
                Box::new(|p, d| p.right_cantabile.air_noise_base = (p.right_cantabile.air_noise_base + d).clamp(0.1, 4.0)),
            ),
            (
                "left.octave_beat_hz_base".into(),
                0.15,
                Box::new(|p, d| p.left_bajos.octave_beat_hz_base = (p.left_bajos.octave_beat_hz_base + d).clamp(0.2, 5.0)),
            ),
            (
                "right.octave_beat_hz_base".into(),
                0.15,
                Box::new(|p, d| p.right_cantabile.octave_beat_hz_base = (p.right_cantabile.octave_beat_hz_base + d).clamp(0.2, 5.0)),
            ),
            (
                "left.pitch_bend_cents".into(),
                1.0,
                Box::new(|p, d| p.left_bajos.pitch_bend_cents_per_pressure = (p.left_bajos.pitch_bend_cents_per_pressure + d).clamp(-28.0, 15.0)),
            ),
            (
                "left.pitch_bend_slope".into(),
                1.0,
                Box::new(|p, d| p.left_bajos.pitch_bend_slope = (p.left_bajos.pitch_bend_slope + d).clamp(-20.0, 28.0)),
            ),
            (
                "right.pitch_bend_cents".into(),
                0.8,
                Box::new(|p, d| p.right_cantabile.pitch_bend_cents_per_pressure = (p.right_cantabile.pitch_bend_cents_per_pressure + d).clamp(-20.0, 15.0)),
            ),
        ];
        mutators.extend(tran_coupled);
    }

    if ver.use_v5_vivo() {
        // v5 pallet: the slot ratio sets the pressure law, plus the celda filling and the jet
        // hiss. The pallet times are not tuned: they are the key's travel times, which the
        // eval cannot see (it replays each recording's attack as bellows pressure).
        let pallet_muts: Vec<(String, f32, ParamMutator)> = vec![
            (
                "pallet_slot_ratio".into(),
                0.03,
                Box::new(|p, d| p.pallet_slot_ratio = (p.pallet_slot_ratio + d).clamp(0.05, 0.8)),
            ),
            (
                "pallet_slot_ratio_oct".into(),
                0.05,
                Box::new(|p, d| {
                    p.pallet_slot_ratio_oct = (p.pallet_slot_ratio_oct + d).clamp(-0.5, 0.8)
                }),
            ),
            (
                "celda_fill_ms".into(),
                0.1,
                Box::new(|p, d| p.celda_fill_ms = (p.celda_fill_ms + d).clamp(0.05, 5.0)),
            ),
            (
                "celda_fill_oct".into(),
                0.1,
                Box::new(|p, d| p.celda_fill_oct = (p.celda_fill_oct + d).clamp(-0.5, 1.5)),
            ),
            (
                "pallet_jet_gain".into(),
                0.2,
                Box::new(|p, d| p.pallet_jet_gain = (p.pallet_jet_gain + d).clamp(0.0, 8.0)),
            ),
        ];
        mutators.extend(pallet_muts);
    }

    if !only.is_empty() {
        mutators.retain(|(name, _, _)| only.iter().any(|part| name.contains(part.as_str())));
        println!(
            "[Tuner] Tuning {} parameters: {}",
            mutators.len(),
            mutators.iter().map(|(n, _, _)| n.as_str()).collect::<Vec<_>>().join(", ")
        );
    }

    for round in 0..rounds {
        let mut improved_in_round = false;
        let step_scale = 0.56f32.powi(round as i32);

        for (name, base_step, mutate) in &mut mutators {
            let step = *base_step * step_scale;
            for dir in [1.0f32, -1.0f32] {
                let mut moved_dir = false;
                for _sub in 0..3 {
                    let mut cand = best_preset.clone();
                    mutate(&mut cand, dir * step);
                    let card = evaluate_preset(&cand, golden_samples, sample_rate, false);

                    // Accept only if neither Train nor Val regresses and at least one strictly improves
                    let train_better =
                        card.scores.composite_train_loss < best_card.scores.composite_train_loss;
                    let val_better =
                        card.scores.composite_val_loss < best_card.scores.composite_val_loss;
                    let train_ok =
                        card.scores.composite_train_loss <= best_card.scores.composite_train_loss;
                    let val_ok =
                        card.scores.composite_val_loss <= best_card.scores.composite_val_loss;

                    if (train_better && val_ok) || (val_better && train_ok) {
                        best_preset = cand;
                        best_card = card;
                        improved_in_round = true;
                        moved_dir = true;
                        println!(
                            "  [Round {}] {:28} ({:+.4}) -> Train: {:.2} ({:.5}), Val: {:.2} ({:.5})",
                            round + 1,
                            name,
                            dir * step,
                            best_card.scores.composite_train_score,
                            best_card.scores.composite_train_loss,
                            best_card.scores.composite_val_score,
                            best_card.scores.composite_val_loss,
                        );
                    } else {
                        break;
                    }
                }
                if moved_dir {
                    break;
                }
            }
        }
        if !improved_in_round {
            println!(
                "  [Round {}] No move at scale {:.2}, refining step size...",
                round + 1,
                step_scale
            );
        }
    }

    let final_card = evaluate_preset(&best_preset, golden_samples, sample_rate, true);
    (best_preset, final_card)
}
