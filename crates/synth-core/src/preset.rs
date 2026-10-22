use serde::{Deserialize, Serialize};

/// Represents which manual (hand panel) of the bandoneon is playing a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hand {
    /// Left-hand panel (Bajos / Bass side, acoustic C2..B4, MIDI 36..71).
    LeftBajos,
    /// Right-hand panel (Cantabile / Treble side, acoustic C4..C7, MIDI 60..96).
    RightCantabile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ManualScalingParams {
    /// Base activation threshold pressure for the 8' primary reed (normalized 0..1).
    pub p_th_8ft_base: f32,
    /// Slope of 8' threshold pressure with normalized manual pitch `u`.
    pub p_th_8ft_slope: f32,
    /// Base activation threshold pressure for the 4' octave reed (normalized 0..1).
    pub p_th_4ft_base: f32,
    /// Slope of 4' threshold pressure with normalized manual pitch `u`.
    pub p_th_4ft_slope: f32,

    /// Base 4'-to-8' coupling gain (amplitude ratio of octave reed to primary reed at u=0).
    pub octave_4ft_gain_base: f32,
    /// Linear slope of 4' octave gain with normalized manual pitch `u`.
    pub octave_4ft_gain_slope: f32,
    /// Quadratic curvature of 4' octave gain with normalized manual pitch `u`.
    pub octave_4ft_gain_curve: f32,
    /// Dynamic growth of 4'-to-8' ratio with blowing pressure (8' slot saturation vs 4' growth).
    pub octave_4ft_pressure_boost: f32,

    /// Two-sided slot constriction / resting gap asymmetry (controls even-to-odd harmonic drive).
    pub reed_asymmetry_base: f32,
    /// Slope of slot constriction asymmetry with manual pitch `u`.
    pub reed_asymmetry_slope: f32,

    /// Orifice jet sharpness / slot edge nonlinearity (controls high harmonics H3..H16).
    pub slot_sharpness_base: f32,
    /// Slope of orifice jet sharpness with manual pitch `u`.
    pub slot_sharpness_slope: f32,
    /// Dynamic brightness boost from soft (p) to loud (f) pressure.
    pub pressure_brightness_mod: f32,

    /// Finite wooden box baffle high-pass radiation corner frequency (Hz).
    pub baffle_cutoff_hz: f32,
    /// Viscous boundary-layer / jet-transit 2-pole low-pass corner frequency at u=0 (Hz).
    pub jet_corner_base_hz: f32,
    /// Slope of jet-transit low-pass corner frequency with manual pitch `u` (Hz).
    pub jet_corner_slope_hz: f32,

    /// Mechanical reed damping factor `g = omega_0 / Q` base multiplier (controls attack/decay speed).
    pub reed_damping_base: f32,
    /// Slope of reed damping with manual pitch `u`.
    pub reed_damping_slope: f32,

    /// Piston flow coupling `S_piston` (controls self-oscillation startup speed).
    pub piston_coupling_base: f32,
    /// Aeroacoustic chamber beating rate in Hz between coupled 8' and 4' reeds.
    pub octave_beat_hz_base: f32,
    /// Slope of octave beating in Hz with manual pitch `u`.
    pub octave_beat_hz_slope: f32,

    /// Aerodynamic pressure-induced pitch bend in cents at u=0.
    pub pitch_bend_cents_per_pressure: f32,
    /// Slope of aerodynamic pitch bend in cents with manual pitch `u`.
    pub pitch_bend_slope: f32,

    /// Manual-specific pallet click transmission scale.
    pub pallet_click_scale: f32,
    /// Manual-specific peripheral slot air leakage turbulence base scale.
    pub air_noise_base: f32,
    /// Slope of peripheral slot air turbulence with manual pitch `u`.
    pub air_noise_slope: f32,
}

impl Default for ManualScalingParams {
    fn default() -> Self {
        BandoneonPreset::default_bandoneon().right_cantabile
    }
}

/// Parameters for a single resonant mode (biquad peaking/resonator stage) of the wooden body.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BodyModeParam {
    pub freq_hz: f32,
    pub q: f32,
    pub gain_db: f32,
}

fn default_left_body_modes() -> [BodyModeParam; 6] {
    [
        BodyModeParam {
            freq_hz: 130.0,
            q: 1.5,
            gain_db: -3.5,
        },
        BodyModeParam {
            freq_hz: 255.0,
            q: 1.7,
            gain_db: 3.2,
        },
        BodyModeParam {
            freq_hz: 495.0,
            q: 1.9,
            gain_db: 5.0,
        },
        BodyModeParam {
            freq_hz: 1020.0,
            q: 1.8,
            gain_db: 2.0,
        },
        BodyModeParam {
            freq_hz: 2050.0,
            q: 1.6,
            gain_db: 3.5,
        },
        BodyModeParam {
            freq_hz: 3850.0,
            q: 1.5,
            gain_db: -0.5,
        },
    ]
}

/// Complete serializable physical modeling preset for the Bandoneon synthesizer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandoneonPreset {
    pub name: String,
    /// Bellows pressure exponent `gamma` mapping MIDI CC#11 (0..127) to physical pressure.
    pub bellows_curve_gamma: f32,
    /// Bellows pressure attack smoothing time constant (ms).
    pub bellows_attack_ms: f32,
    /// Bellows pressure release smoothing time constant (ms).
    pub bellows_release_ms: f32,
    /// Pallet valve opening time constant at mezzo-forte (ms).
    pub valve_open_ms: f32,
    /// Pallet valve closing time constant (ms).
    pub valve_close_ms: f32,
    /// Mechanical pallet valve click level (linear gain).
    pub pallet_click_gain: f32,
    /// Turbulent air leakage / breath noise level (linear gain).
    pub air_turbulence_gain: f32,
    /// Shared reed chamber pressure drop coupling coefficient.
    pub chamber_coupling: f32,
    /// Master output trim gain.
    pub master_gain: f32,

    /// Smooth scaling laws for the Left Hand (Bajos) manual.
    pub left_bajos: ManualScalingParams,
    /// Smooth scaling laws for the Right Hand (Cantabile) manual.
    pub right_cantabile: ManualScalingParams,

    /// 6-band wooden body & bellows modal filter bank for the Right Hand (Cantabile) box.
    pub body_modes: [BodyModeParam; 6],

    /// 6-band wooden body & bellows modal filter bank for the Left Hand (Bajos) bass box.
    #[serde(default = "default_left_body_modes")]
    pub left_body_modes: [BodyModeParam; 6],

    /// Optional bounded per-semitone brightness trim offsets (61 semitones: MIDI 36..=96),
    /// heavily regularized by the second-difference smoothness penalty `L_reg`.
    pub pitch_brightness_trim: Vec<f32>,
}

/// Resolved physical parameters for a single note (computed smoothly from `BandoneonPreset`).
#[derive(Debug, Clone, Copy)]
pub struct ResolvedNoteParams {
    pub u_manual: f32,
    pub p_th_8ft: f32,
    pub p_th_4ft: f32,
    pub octave_4ft_gain: f32,
    pub octave_4ft_pressure_boost: f32,
    pub reed_asymmetry: f32,
    pub slot_sharpness: f32,
    pub pressure_brightness_mod: f32,
    pub baffle_cutoff_hz: f32,
    pub jet_corner_hz: f32,
    pub reed_damping: f32,
    pub piston_coupling: f32,
    pub octave_beat_hz: f32,
    pub pitch_bend_cents: f32,
    pub pallet_click_scale: f32,
    pub air_noise_scale: f32,
}

impl BandoneonPreset {
    /// Creates a physically grounded default preset calibrated for an Alfred Arnold (AA) bandoneon.
    pub fn default_bandoneon() -> Self {
        Self {
            name: "Alfred Arnold (AA) Physical Model v2 (ADAA + Baffle + Coupled Celda)".to_string(),
            bellows_curve_gamma: 1.15,
            bellows_attack_ms: 2.5,
            bellows_release_ms: 7.0,
            valve_open_ms: 3.3,
            valve_close_ms: 10.0,
            pallet_click_gain: 0.0022,
            air_turbulence_gain: 0.0225,
            chamber_coupling: 0.16,
            master_gain: 0.48,
            left_bajos: ManualScalingParams {
                p_th_8ft_base: 0.022,
                p_th_8ft_slope: -0.005,
                p_th_4ft_base: 0.026,
                p_th_4ft_slope: 0.006,
                octave_4ft_gain_base: 1.25,
                octave_4ft_gain_slope: -0.308,
                octave_4ft_gain_curve: -0.78,
                octave_4ft_pressure_boost: 0.443,
                reed_asymmetry_base: 0.42,
                reed_asymmetry_slope: -0.158,
                slot_sharpness_base: 6.81,
                slot_sharpness_slope: -1.69,
                pressure_brightness_mod: 3.73,
                baffle_cutoff_hz: 412.0,
                jet_corner_base_hz: 2670.0,
                jet_corner_slope_hz: -80.0,
                reed_damping_base: 0.055,
                reed_damping_slope: 0.040,
                piston_coupling_base: 0.50,
                octave_beat_hz_base: 3.60,
                octave_beat_hz_slope: -1.10,
                pitch_bend_cents_per_pressure: -13.0,
                pitch_bend_slope: 13.5,
                pallet_click_scale: 0.18,
                air_noise_base: 2.09,
                air_noise_slope: -2.05,
            },
            right_cantabile: ManualScalingParams {
                p_th_8ft_base: 0.025,
                p_th_8ft_slope: -0.005,
                p_th_4ft_base: 0.017,
                p_th_4ft_slope: 0.005,
                octave_4ft_gain_base: 0.88,
                octave_4ft_gain_slope: -0.38,
                octave_4ft_gain_curve: -0.32,
                octave_4ft_pressure_boost: 0.65,
                reed_asymmetry_base: 0.28,
                reed_asymmetry_slope: -0.16,
                slot_sharpness_base: 5.40,
                slot_sharpness_slope: -0.89,
                pressure_brightness_mod: 3.32,
                baffle_cutoff_hz: 455.0,
                jet_corner_base_hz: 3600.0,
                jet_corner_slope_hz: 1644.5,
                reed_damping_base: 0.065,
                reed_damping_slope: 0.045,
                piston_coupling_base: 0.58,
                octave_beat_hz_base: 2.35,
                octave_beat_hz_slope: 0.25,
                pitch_bend_cents_per_pressure: -0.5,
                pitch_bend_slope: 1.2,
                pallet_click_scale: 0.58,
                air_noise_base: 0.87,
                air_noise_slope: -0.25,
            },
            body_modes: [
                BodyModeParam {
                    freq_hz: 135.0,
                    q: 1.4,
                    gain_db: -2.0,
                },
                BodyModeParam {
                    freq_hz: 265.0,
                    q: 1.8,
                    gain_db: 2.0,
                },
                BodyModeParam {
                    freq_hz: 520.0,
                    q: 2.0,
                    gain_db: 3.8,
                },
                BodyModeParam {
                    freq_hz: 1050.0,
                    q: 1.8,
                    gain_db: 5.2,
                },
                BodyModeParam {
                    freq_hz: 2150.0,
                    q: 1.6,
                    gain_db: 6.7,
                },
                BodyModeParam {
                    freq_hz: 3950.0,
                    q: 1.4,
                    gain_db: 8.5,
                },
            ],
            left_body_modes: default_left_body_modes(),
            pitch_brightness_trim: vec![0.0; 61],
        }
    }

    /// Resolves smooth physical parameters for a given MIDI note and hand manual.
    #[inline]
    pub fn resolve_note(&self, midi_note: u8, hand: Hand) -> ResolvedNoteParams {
        let clamped_note = midi_note.clamp(36, 96);
        // Per-manual normalized compass coordinate u in [0.0, 1.0]
        let (u, m) = match hand {
            Hand::LeftBajos => (
                ((clamped_note as f32 - 36.0) / 35.0).clamp(0.0, 1.0),
                &self.left_bajos,
            ),
            Hand::RightCantabile => (
                ((clamped_note as f32 - 60.0) / 36.0).clamp(0.0, 1.0),
                &self.right_cantabile,
            ),
        };

        let trim_idx = (clamped_note - 36) as usize;
        let trim = self
            .pitch_brightness_trim
            .get(trim_idx)
            .copied()
            .unwrap_or(0.0)
            .clamp(-1.5, 1.5);

        let p_th_8ft = (m.p_th_8ft_base + m.p_th_8ft_slope * u).clamp(0.005, 0.25);
        let p_th_4ft = (m.p_th_4ft_base + m.p_th_4ft_slope * u).clamp(0.005, 0.30);

        let octave_4ft_gain = (m.octave_4ft_gain_base
            + m.octave_4ft_gain_slope * u
            + m.octave_4ft_gain_curve * u * u)
            .clamp(0.02, 3.0);

        let reed_asymmetry = (m.reed_asymmetry_base + m.reed_asymmetry_slope * u).clamp(0.02, 1.20);
        let slot_sharpness =
            (m.slot_sharpness_base + m.slot_sharpness_slope * u + trim).clamp(0.5, 10.0);

        let baffle_cutoff_hz = m.baffle_cutoff_hz.clamp(80.0, 1200.0);
        let jet_corner_hz =
            (m.jet_corner_base_hz + m.jet_corner_slope_hz * u).clamp(600.0, 12000.0);

        let reed_damping = (m.reed_damping_base + m.reed_damping_slope * u).clamp(0.008, 0.30);
        let piston_coupling = m.piston_coupling_base.clamp(0.05, 0.95);
        let octave_beat_hz =
            (m.octave_beat_hz_base + m.octave_beat_hz_slope * u).clamp(0.15, 6.0);
        let pitch_bend_cents =
            (m.pitch_bend_cents_per_pressure + m.pitch_bend_slope * u).clamp(-35.0, 35.0);
        let pallet_click_scale = m.pallet_click_scale.clamp(0.05, 4.0);
        let air_noise_scale = (m.air_noise_base + m.air_noise_slope * u).clamp(0.05, 4.0);

        ResolvedNoteParams {
            u_manual: u,
            p_th_8ft,
            p_th_4ft,
            octave_4ft_gain,
            octave_4ft_pressure_boost: m.octave_4ft_pressure_boost.clamp(0.0, 2.0),
            reed_asymmetry,
            slot_sharpness,
            pressure_brightness_mod: m.pressure_brightness_mod.clamp(0.0, 8.0),
            baffle_cutoff_hz,
            jet_corner_hz,
            reed_damping,
            piston_coupling,
            octave_beat_hz,
            pitch_bend_cents,
            pallet_click_scale,
            air_noise_scale,
        }
    }

    /// Computes the second-difference smoothness regularization penalty `L_reg`
    /// over `pitch_brightness_trim` to prevent per-note lookup overfitting.
    pub fn roughness_penalty(&self) -> f32 {
        if self.pitch_brightness_trim.len() < 3 {
            return 0.0;
        }
        let mut sum_sq = 0.0f32;
        for i in 1..(self.pitch_brightness_trim.len() - 1) {
            let d2 = self.pitch_brightness_trim[i + 1] - 2.0 * self.pitch_brightness_trim[i]
                + self.pitch_brightness_trim[i - 1];
            sum_sq += d2 * d2;
        }
        let l2: f32 = self.pitch_brightness_trim.iter().map(|x| x * x).sum();
        0.5 * sum_sq + 0.05 * l2
    }
}
