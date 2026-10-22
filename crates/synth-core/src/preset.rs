use serde::{Deserialize, Serialize};

/// Synthesis engine architecture version for comparative evaluation and live Web UI switching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SynthVersion {
    /// v1: Baseline Lumped Odd-Clipper Model (memoryless odd clipper, 1-pole LP, single shared body EQ).
    V1Baseline = 0,
    /// v2: Rationalized Conjugate 1st-Order ADAA + 1-Pole Baffle + Coupled Celda + Dual-Box Modal EQ.
    V2AdaaBaffle = 1,
    /// v3_aero (Direction 1): v2 + 2-Pole Dipole Baffle, Monopole dU/dt Jet Radiation & Pitch-Scaled Celda Cavity Resonance.
    V3AeroRadiation = 2,
    /// v3_beam (Direction 2): v2 + Asymmetric Blown-Closed Duty-Cycle ADAA & Euler-Bernoulli Clamped-Bar Overtones (4f0, 6f0).
    V3ClampedBar = 3,
    /// v3_transient (Direction 3): v2 + Unclamped Tip-Mass Inertia, Bimorphic 4'/8' Hopf Onset Lag & Staccato Leakage Chiff.
    V3TransientEntrain = 4,
    /// v3 (Unified SOTA): Integrates Directions 1 + 2 + 3 with full multi-parameter physical calibration.
    #[default]
    V3Unified = 5,
}

impl SynthVersion {
    pub const ALL: [SynthVersion; 6] = [
        SynthVersion::V1Baseline,
        SynthVersion::V2AdaaBaffle,
        SynthVersion::V3AeroRadiation,
        SynthVersion::V3ClampedBar,
        SynthVersion::V3TransientEntrain,
        SynthVersion::V3Unified,
    ];

    #[inline]
    pub fn from_u8(id: u8) -> Self {
        match id {
            0 => Self::V1Baseline,
            1 => Self::V2AdaaBaffle,
            2 => Self::V3AeroRadiation,
            3 => Self::V3ClampedBar,
            4 => Self::V3TransientEntrain,
            _ => Self::V3Unified,
        }
    }

    #[inline]
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::V1Baseline => "v1",
            Self::V2AdaaBaffle => "v2",
            Self::V3AeroRadiation => "v3_aero",
            Self::V3ClampedBar => "v3_beam",
            Self::V3TransientEntrain => "v3_transient",
            Self::V3Unified => "v3",
        }
    }

    pub fn from_slug(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "v1" | "v1_baseline" | "baseline" | "0" => Some(Self::V1Baseline),
            "v2" | "v2_adaa" | "v2_adaa_baffle" | "1" => Some(Self::V2AdaaBaffle),
            "v3_aero" | "aero" | "dir1" | "2" => Some(Self::V3AeroRadiation),
            "v3_beam" | "beam" | "clamped_bar" | "dir2" | "3" => Some(Self::V3ClampedBar),
            "v3_transient" | "transient" | "entrain" | "dir3" | "4" => {
                Some(Self::V3TransientEntrain)
            }
            "v3" | "v3_unified" | "unified" | "sota" | "5" => Some(Self::V3Unified),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::V1Baseline => "v1: Baseline Lumped Odd-Clipper",
            Self::V2AdaaBaffle => "v2: ADAA + 1-Pole Baffle + Coupled Celda",
            Self::V3AeroRadiation => "v3-Aero (Dir 1): Monopole dU/dt & 2-Pole Dipole Baffle",
            Self::V3ClampedBar => "v3-Beam (Dir 2): Asymmetric Duty ADAA & Clamped-Bar Modes",
            Self::V3TransientEntrain => "v3-Transient (Dir 3): Hopf Onset Lag & Staccato Chiff",
            Self::V3Unified => "v3: Unified SOTA Physical Model (Aero + Beam + Transient)",
        }
    }
}

/// Represents which manual (hand panel) of the bandoneon is playing a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hand {
    /// Left-hand panel (Bajos / Bass side, MIDI Channel 1, acoustic C2..B4, MIDI 36..=71).
    LeftBajos,
    /// Right-hand panel (Cantabile / Treble side, MIDI Channel 2, acoustic C4..C7, MIDI 60..=96).
    RightCantabile,
}

impl Hand {
    /// Lowest MIDI note on the Left-Hand (*Bajos*) keyboard (`C2 = 36`).
    pub const LEFT_MIN_MIDI: u8 = 36;
    /// Highest MIDI note on the Left-Hand (*Bajos*) keyboard (`B4 = 71`).
    pub const LEFT_MAX_MIDI: u8 = 71;
    /// Lowest MIDI note on the Right-Hand (*Cantabile*) keyboard (`C4 = 60`).
    pub const RIGHT_MIN_MIDI: u8 = 60;
    /// Highest MIDI note on the Right-Hand (*Cantabile*) keyboard (`C7 = 96`).
    pub const RIGHT_MAX_MIDI: u8 = 96;
    /// Lowest MIDI note present on BOTH Bandolibre keyboards (`C4 = 60`).
    pub const OVERLAP_MIN_MIDI: u8 = 60;
    /// Highest MIDI note present on BOTH Bandolibre keyboards (`B4 = 71`).
    pub const OVERLAP_MAX_MIDI: u8 = 71;

    /// Returns true if `midi_note` falls in the 12-semitone overlap octave (`C4..=B4`, MIDI 60..=71)
    /// present on both the Left (*Bajos*) and Right (*Cantabile*) keyboards.
    #[inline]
    pub const fn is_overlap_note(midi_note: u8) -> bool {
        midi_note >= Self::OVERLAP_MIN_MIDI && midi_note <= Self::OVERLAP_MAX_MIDI
    }
}

/// Controls how incoming MIDI channels and notes are mapped to the two bandoneon keyboards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardMode {
    /// Default Bandolibre mode:
    /// - MIDI Channel 1 (`0x90`, `L_MIDI_CH = 0`) is simulated as the Left Keyboard (`Hand::LeftBajos`:
    ///   `C2..B4`, deep wooden bass box resonator, warm *Bajos* reed scaling, Left stereo pan).
    /// - MIDI Channel 2 (`0x91`, `R_MIDI_CH = 1`) is simulated as the Right Keyboard (`Hand::RightCantabile`:
    ///   `C4..C7`, shallow treble grill resonator, singing *Cantabile* reed scaling, Right stereo pan).
    /// - Overlapping notes (`C4..B4`, MIDI `60..=71`) can be played on either channel or both simultaneously.
    #[default]
    DualChannelBandolibre = 0,
    /// Pianist Split mode (for single-channel MIDI piano keyboards or unified single-keyboard UI):
    /// - Routes notes `< split_note` (default `C4 = 60`) to `Hand::LeftBajos` (Left tone & pan)
    ///   and notes `>= split_note` to `Hand::RightCantabile` (Right tone & pan), regardless of MIDI channel.
    PianistSplit = 1,
}

impl KeyboardMode {
    #[inline]
    pub const fn from_u8(id: u8) -> Self {
        match id {
            1 => Self::PianistSplit,
            _ => Self::DualChannelBandolibre,
        }
    }

    #[inline]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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

    // --- Direction 1 (Aero Radiation & Celda Cavity) ---
    /// High-frequency monopole dU/dt & celda cavity resonance boost at u=0.
    pub monopole_rad_boost: f32,
    /// Slope of monopole dU/dt radiation boost with manual pitch `u`.
    pub monopole_rad_slope: f32,
    /// 2nd-order sub-baffle dipole diffraction roll-off mix (`0.0 .. 1.0`).
    pub dipole_baffle_mix: f32,

    // --- Direction 2 (Asymmetric Blown-Closed Duty & Clamped-Bar Modes) ---
    /// Dimensionless resting gap bias `y0 / a` creating the ~2/3 blown-closed duty cycle.
    pub duty_bias_base: f32,
    /// Euler-Bernoulli clamped-bar 2nd flexural mode (~6f0 & 4f0) excitation gain.
    pub clamped_bar_h6_gain: f32,
    /// Blown-closed duty-cycle H3 notch cancellation factor.
    pub h3_notch_inject: f32,

    // --- Direction 3 (Bimorphic 8'/4' Hopf Onset & Staccato Chiff) ---
    /// Soft Hopf bifurcation startup knee on the heavier 8' reed (`0.0 .. 0.85`).
    pub hopf_8ft_knee: f32,
    /// Transient 4' octave energy boost during rapid staccato attacks.
    pub staccato_4ft_boost: f32,
    /// Unpitched orifice leakage (`delta_leak`) turbulence chiff during staccato valve transients.
    pub staccato_chiff_gain: f32,
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
    /// Active synthesis engine version for this preset.
    #[serde(default)]
    pub version: SynthVersion,
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

/// Pre-resolved physical parameters for an individual reed (either 8' primary or 4' octave).
/// Computed once at `NoteOn` / preset refresh so the audio-rate inner loop is branch-free.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedReedParams {
    pub p_th: f32,
    pub p_ext: f32,
    pub inv_one_minus_p_ext: f32,
    pub reed_asymmetry: f32,
    pub slot_sharpness: f32,
    pub pressure_brightness_mod: f32,
    pub baffle_cutoff_hz: f32,
    pub jet_corner_hz: f32,
    pub reed_damping: f32,
    pub piston_coupling: f32,
    pub pitch_bend_cents: f32,
    pub inertia_mult: f32,
    pub monopole_rad_boost: f32,
    pub dipole_baffle_mix: f32,
    pub c_lin: f32,
    pub clamped_bar_h6_gain: f32,
    pub h3_notch_inject: f32,
    pub hopf_knee: f32,
}

/// Resolved physical parameters for a single note (computed smoothly from `BandoneonPreset`).
#[derive(Debug, Clone, Copy)]
pub struct ResolvedNoteParams {
    pub u_manual: f32,
    pub reed_8ft: ResolvedReedParams,
    pub reed_4ft: ResolvedReedParams,
    pub octave_4ft_gain: f32,
    pub octave_4ft_pressure_boost: f32,
    pub octave_beat_hz: f32,
    pub pallet_click_scale: f32,
    pub air_noise_scale: f32,
    pub staccato_4ft_boost: f32,
    pub staccato_chiff_gain: f32,
}

impl BandoneonPreset {
    /// Returns the calibrated preset for the requested `SynthVersion`.
    pub fn for_version(version: SynthVersion) -> Self {
        match version {
            SynthVersion::V1Baseline => Self::v1_baseline(),
            SynthVersion::V2AdaaBaffle => Self::v2_adaa_baffle(),
            SynthVersion::V3AeroRadiation => Self::v3_aero_radiation(),
            SynthVersion::V3ClampedBar => Self::v3_clamped_bar(),
            SynthVersion::V3TransientEntrain => Self::v3_transient_entrain(),
            SynthVersion::V3Unified => Self::v3_unified(),
        }
    }

    /// Creates the Baseline v1 Lumped Odd-Clipper preset (no ADAA, no baffle shelf, shared body EQ).
    pub fn v1_baseline() -> Self {
        let mut p = Self::v2_adaa_baffle();
        p.name = "Alfred Arnold (AA) Baseline v1 (Lumped Odd-Clipper)".to_string();
        p.version = SynthVersion::V1Baseline;
        p.left_bajos.slot_sharpness_base = 3.6;
        p.left_bajos.slot_sharpness_slope = -0.8;
        p.left_bajos.reed_asymmetry_base = 0.20;
        p.left_bajos.reed_asymmetry_slope = -0.05;
        p.right_cantabile.slot_sharpness_base = 3.2;
        p.right_cantabile.slot_sharpness_slope = -0.5;
        p.right_cantabile.reed_asymmetry_base = 0.18;
        p.right_cantabile.reed_asymmetry_slope = -0.05;
        // Single shared body modes in v1
        p.left_body_modes = p.body_modes;
        p
    }

    /// Creates the exact v2 ADAA + Baffle + Coupled Celda preset (`65.87` Train / `64.25` Val).
    pub fn v2_adaa_baffle() -> Self {
        Self {
            name: "Alfred Arnold (AA) Physical Model v2 (ADAA + Baffle + Coupled Celda)".to_string(),
            version: SynthVersion::V2AdaaBaffle,
            bellows_curve_gamma: 1.15,
            bellows_attack_ms: 2.5,
            bellows_release_ms: 7.0,
            valve_open_ms: 3.3,
            valve_close_ms: 10.0,
            pallet_click_gain: 0.0022,
            air_turbulence_gain: 0.022507008,
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
                octave_4ft_pressure_boost: 0.44267517,
                reed_asymmetry_base: 0.42,
                reed_asymmetry_slope: -0.15760513,
                slot_sharpness_base: 6.8073597,
                slot_sharpness_slope: -1.688,
                pressure_brightness_mod: 3.7274985,
                baffle_cutoff_hz: 411.856,
                jet_corner_base_hz: 2670.0,
                jet_corner_slope_hz: -80.0,
                reed_damping_base: 0.055,
                reed_damping_slope: 0.040,
                piston_coupling_base: 0.50,
                octave_beat_hz_base: 3.598373,
                octave_beat_hz_slope: -1.10,
                pitch_bend_cents_per_pressure: -13.0,
                pitch_bend_slope: 13.5,
                pallet_click_scale: 0.18,
                air_noise_base: 2.0904,
                air_noise_slope: -2.05,
                monopole_rad_boost: 0.0,
                monopole_rad_slope: 0.0,
                dipole_baffle_mix: 0.0,
                duty_bias_base: 0.0,
                clamped_bar_h6_gain: 0.0,
                h3_notch_inject: 0.0,
                hopf_8ft_knee: 0.0,
                staccato_4ft_boost: 0.0,
                staccato_chiff_gain: 0.0,
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
                slot_sharpness_slope: -0.888,
                pressure_brightness_mod: 3.3193634,
                baffle_cutoff_hz: 455.0,
                jet_corner_base_hz: 3600.0,
                jet_corner_slope_hz: 1644.48,
                reed_damping_base: 0.065,
                reed_damping_slope: 0.045,
                piston_coupling_base: 0.58000004,
                octave_beat_hz_base: 2.35,
                octave_beat_hz_slope: 0.25,
                pitch_bend_cents_per_pressure: -0.5,
                pitch_bend_slope: 1.2,
                pallet_click_scale: 0.58,
                air_noise_base: 0.87,
                air_noise_slope: -0.25,
                monopole_rad_boost: 0.0,
                monopole_rad_slope: 0.0,
                dipole_baffle_mix: 0.0,
                duty_bias_base: 0.0,
                clamped_bar_h6_gain: 0.0,
                h3_notch_inject: 0.0,
                hopf_8ft_knee: 0.0,
                staccato_4ft_boost: 0.0,
                staccato_chiff_gain: 0.0,
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

    /// Direction 1 (`v3_aero`): Monopole dU/dt Jet Radiation & 2-Pole Dipole Baffle Diffraction.
    pub fn v3_aero_radiation() -> Self {
        let mut p = Self::v2_adaa_baffle();
        p.name = "Alfred Arnold (AA) v3-Aero (Monopole dU/dt Radiation & 2-Pole Dipole Baffle)".to_string();
        p.version = SynthVersion::V3AeroRadiation;
        p.left_bajos.baffle_cutoff_hz = 391.768;
        p.left_bajos.monopole_rad_slope = 0.087864;
        p.left_bajos.dipole_baffle_mix = 0.011532;
        p.right_cantabile.monopole_rad_slope = -0.17456368;
        p.body_modes[5].gain_db = 8.872;
        p.left_body_modes[2].gain_db = 5.142997;
        p.left_body_modes[3].gain_db = 1.880836;
        p.left_body_modes[4].gain_db = 3.929164;
        p
    }

    /// Direction 2 (`v3_beam`): Asymmetric Blown-Closed Duty-Cycle ADAA & Euler-Bernoulli Clamped-Bar Modes.
    pub fn v3_clamped_bar() -> Self {
        let mut p = Self::v2_adaa_baffle();
        p.name = "Alfred Arnold (AA) v3-Beam (Asymmetric Duty ADAA & Clamped-Bar Flexural Modes)".to_string();
        p.version = SynthVersion::V3ClampedBar;
        p.left_bajos.slot_sharpness_base = 6.72736;
        p.left_bajos.pressure_brightness_mod = 3.6471467;
        p.left_bajos.duty_bias_base = -0.0182;
        p.left_bajos.h3_notch_inject = 0.008;
        p.right_cantabile.slot_sharpness_base = 5.418848;
        p.right_cantabile.pressure_brightness_mod = 3.2390115;
        p.right_cantabile.clamped_bar_h6_gain = 0.0037200002;
        p
    }

    /// Direction 3 (`v3_transient`): Unclamped Tip-Mass Inertia, Bimorphic 4'/8' Hopf Onset Lag & Staccato Chiff.
    pub fn v3_transient_entrain() -> Self {
        let mut p = Self::v2_adaa_baffle();
        p.name = "Alfred Arnold (AA) v3-Transient (Bimorphic Hopf Onset Lag & Staccato Leakage Chiff)".to_string();
        p.version = SynthVersion::V3TransientEntrain;
        p.left_bajos.reed_damping_base = 0.04641672;
        p.left_bajos.hopf_8ft_knee = 0.05011;
        p.left_bajos.staccato_4ft_boost = 0.104832;
        p.left_bajos.staccato_chiff_gain = 0.65239197;
        p.right_cantabile.air_noise_base = 0.8938328;
        p
    }

    /// Unified SOTA v3 Model (`v3`): Combines Directions 1 + 2 + 3 with calibrated physical scaling laws.
    pub fn v3_unified() -> Self {
        let mut p = Self::v2_adaa_baffle();
        p.name = "Alfred Arnold (AA) Physical Model v3 (Aero dU/dt + Clamped-Bar + Hopf Entrainment)".to_string();
        p.version = SynthVersion::V3Unified;
        // Direction 1 (Aero: Quadrature Monopole dU/dt & Sub-Baffle Diffraction)
        p.left_bajos.baffle_cutoff_hz = 391.768;
        p.left_bajos.monopole_rad_boost = 0.024799999;
        p.left_bajos.monopole_rad_slope = 0.152664;
        p.left_bajos.dipole_baffle_mix = 0.011532;
        p.right_cantabile.monopole_rad_slope = -0.18993968;
        p.body_modes[5].gain_db = 8.872;
        p.left_body_modes[2].gain_db = 5.142997;
        p.left_body_modes[3].gain_db = 1.880836;
        p.left_body_modes[4].gain_db = 3.929164;
        p.left_body_modes[5].gain_db = -1.0;
        // Direction 2 (Beam: Poiseuille-Bernoulli Duty Softening & Clamped-Bar Modes)
        p.left_bajos.reed_asymmetry_base = 0.4143;
        p.left_bajos.slot_sharpness_base = 6.72736;
        p.left_bajos.pressure_brightness_mod = 3.517547;
        p.left_bajos.duty_bias_base = -0.0182;
        p.left_bajos.clamped_bar_h6_gain = 0.0074563683;
        p.left_bajos.h3_notch_inject = 0.008;
        p.right_cantabile.reed_asymmetry_slope = -0.175;
        p.right_cantabile.slot_sharpness_base = 5.418848;
        p.right_cantabile.pressure_brightness_mod = 3.1094117;
        p.right_cantabile.clamped_bar_h6_gain = 0.008869968;
        p.right_cantabile.h3_notch_inject = 0.0030752001;
        // Direction 3 (Transient: Unclamped Lastre Inertia, Hopf Onset & Velocity 4' Bite)
        p.left_bajos.reed_damping_base = 0.044033438;
        p.left_bajos.hopf_8ft_knee = 0.059720002;
        p.left_bajos.staccato_4ft_boost = 0.15436512;
        p.left_bajos.staccato_chiff_gain = 0.76399195;
        p.right_cantabile.air_noise_base = 0.8938328;
        p.right_cantabile.staccato_chiff_gain = 0.24919201;
        p
    }

    /// Creates a physically grounded default preset calibrated for an Alfred Arnold (AA) bandoneon.
    pub fn default_bandoneon() -> Self {
        Self::v3_unified()
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

        let monopole_rad_boost =
            (m.monopole_rad_boost + m.monopole_rad_slope * u).clamp(-0.6, 3.5);
        let dipole_baffle_mix = m.dipole_baffle_mix.clamp(-0.6, 1.0);
        let duty_bias = m.duty_bias_base.clamp(-0.20, 0.85);
        let clamped_bar_h6_gain = m.clamped_bar_h6_gain.clamp(-0.25, 0.40);
        let h3_notch_inject = m.h3_notch_inject.clamp(-0.25, 0.40);
        let hopf_8ft_knee = m.hopf_8ft_knee.clamp(0.0, 0.85);
        let staccato_4ft_boost = m.staccato_4ft_boost.clamp(-0.40, 2.0);
        let staccato_chiff_gain = m.staccato_chiff_gain.clamp(0.0, 5.0);
        let pressure_brightness_mod = m.pressure_brightness_mod.clamp(0.0, 8.0);

        let use_tran = matches!(
            self.version,
            SynthVersion::V3TransientEntrain | SynthVersion::V3Unified
        );

        let p_ext_8ft = 0.72 * p_th_8ft;
        let reed_8ft = ResolvedReedParams {
            p_th: p_th_8ft,
            p_ext: p_ext_8ft,
            inv_one_minus_p_ext: 1.0 / (1.0 - p_ext_8ft).max(0.1),
            reed_asymmetry,
            slot_sharpness,
            pressure_brightness_mod,
            baffle_cutoff_hz,
            jet_corner_hz,
            reed_damping,
            piston_coupling,
            pitch_bend_cents,
            inertia_mult: if use_tran {
                0.95 * (1.0 - 0.18 * hopf_8ft_knee)
            } else {
                0.95
            },
            monopole_rad_boost,
            dipole_baffle_mix,
            c_lin: duty_bias,
            clamped_bar_h6_gain,
            h3_notch_inject,
            hopf_knee: if use_tran { hopf_8ft_knee } else { 0.0 },
        };

        let p_ext_4ft = 0.72 * p_th_4ft;
        let dipole_4ft = dipole_baffle_mix * 0.85;
        let reed_4ft = ResolvedReedParams {
            p_th: p_th_4ft,
            p_ext: p_ext_4ft,
            inv_one_minus_p_ext: 1.0 / (1.0 - p_ext_4ft).max(0.1),
            reed_asymmetry: reed_asymmetry * 0.95,
            slot_sharpness: slot_sharpness * 0.90,
            pressure_brightness_mod: pressure_brightness_mod * 0.95,
            baffle_cutoff_hz: baffle_cutoff_hz * 0.85,
            jet_corner_hz: jet_corner_hz * 1.15,
            reed_damping: reed_damping * 1.10,
            piston_coupling: piston_coupling * 1.25,
            pitch_bend_cents: pitch_bend_cents * 0.50,
            inertia_mult: if use_tran {
                2.05 * (1.0 + 0.15 * hopf_8ft_knee)
            } else {
                2.05
            },
            monopole_rad_boost: monopole_rad_boost * 1.15,
            dipole_baffle_mix: dipole_4ft,
            c_lin: duty_bias * 0.50,
            clamped_bar_h6_gain: clamped_bar_h6_gain * 0.65,
            h3_notch_inject: h3_notch_inject * 0.35,
            hopf_knee: 0.0,
        };

        ResolvedNoteParams {
            u_manual: u,
            reed_8ft,
            reed_4ft,
            octave_4ft_gain,
            octave_4ft_pressure_boost: m.octave_4ft_pressure_boost.clamp(0.0, 2.0),
            octave_beat_hz,
            pallet_click_scale,
            air_noise_scale,
            staccato_4ft_boost,
            staccato_chiff_gain,
        }
    }

    /// Copies all numerical parameters from `src` into `self` in place without heap allocations.
    #[inline]
    pub fn copy_params_from(&mut self, src: &Self) {
        self.version = src.version;
        self.bellows_curve_gamma = src.bellows_curve_gamma;
        self.bellows_attack_ms = src.bellows_attack_ms;
        self.bellows_release_ms = src.bellows_release_ms;
        self.valve_open_ms = src.valve_open_ms;
        self.valve_close_ms = src.valve_close_ms;
        self.pallet_click_gain = src.pallet_click_gain;
        self.air_turbulence_gain = src.air_turbulence_gain;
        self.chamber_coupling = src.chamber_coupling;
        self.master_gain = src.master_gain;
        self.left_bajos = src.left_bajos;
        self.right_cantabile = src.right_cantabile;
        self.body_modes = src.body_modes;
        self.left_body_modes = src.left_body_modes;
        if self.pitch_brightness_trim.len() == src.pitch_brightness_trim.len() {
            self.pitch_brightness_trim
                .copy_from_slice(&src.pitch_brightness_trim);
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
