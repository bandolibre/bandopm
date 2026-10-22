use serde::{Deserialize, Serialize};

/// Synthesis engine architecture version for comparative evaluation and live Web UI switching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SynthVersion {
    /// v1: Baseline Lumped Odd-Clipper Model (memoryless odd clipper, 1-pole LP, single shared body EQ).
    V1Baseline = 0,
    /// v2: Rationalized Conjugate 1st-Order ADAA + 1-Pole Baffle + Coupled Celda + Dual-Box Modal EQ.
    V2AdaaBaffle = 1,
    /// v3_aero (v3 Dir 1): v2 + 2-Pole Dipole Baffle & Quadrature Monopole dU/dt Jet Radiation.
    V3AeroRadiation = 2,
    /// v3_beam (v3 Dir 2): v2 + Asymmetric Blown-Closed Duty-Cycle ADAA & Euler-Bernoulli Clamped-Bar Overtones (4f0, 6f0).
    V3ClampedBar = 3,
    /// v3_transient (v3 Dir 3): v2 + Unclamped Tip-Mass Inertia, Bimorphic 4'/8' Hopf Onset Lag & Staccato Leakage Chiff.
    V3TransientEntrain = 4,
    /// v3: Unified v3 Physical Model (Integrates v3 Directions 1 + 2 + 3).
    V3Unified = 5,
    /// v4_cabinet (v4 Dir 1): v3 + Deconvolved Wooden Cabinet 10-Mode Resonator Bank & Early-Reflection FIR.
    V4CabinetIr = 6,
    /// v4_slot (v4 Dir 2): v3 + 3-Regime Plate-Channel Slot Transit Geometry (Moving Formant / Vowel Swell ADAA).
    V4SlotTransit = 7,
    /// v4_coupled (v4 Dir 3): v3 + True 2nd-Order Coupled Mechanical Reed Integrator, Startup Chirp & Shared Celda Feedback.
    V4CoupledOde = 8,
    /// v4_celda (v4 Dir 4): v3 + Pitch-Scaled Individual Celda Cavity Resonator (Okada 2025).
    V4CeldaCavity = 9,
    /// v4_zapatilla (v4 Dir 5): v3 + Leather Pallet (Zapatilla) Release Thump & Closed-Valve Muffled Reed Ring-Down.
    V4ZapatillaValve = 10,
    /// v4 (Unified SOTA): Integrates all v4 physical modeling advances (Cabinet + Slot + Coupled ODE + Celda + Zapatilla + Fast-Math).
    #[default]
    V4Unified = 11,
}

impl SynthVersion {
    pub const COUNT: usize = 12;

    pub const ALL: [SynthVersion; Self::COUNT] = [
        SynthVersion::V1Baseline,
        SynthVersion::V2AdaaBaffle,
        SynthVersion::V3AeroRadiation,
        SynthVersion::V3ClampedBar,
        SynthVersion::V3TransientEntrain,
        SynthVersion::V3Unified,
        SynthVersion::V4CabinetIr,
        SynthVersion::V4SlotTransit,
        SynthVersion::V4CoupledOde,
        SynthVersion::V4CeldaCavity,
        SynthVersion::V4ZapatillaValve,
        SynthVersion::V4Unified,
    ];

    #[inline]
    pub fn from_u8(id: u8) -> Self {
        match id {
            0 => Self::V1Baseline,
            1 => Self::V2AdaaBaffle,
            2 => Self::V3AeroRadiation,
            3 => Self::V3ClampedBar,
            4 => Self::V3TransientEntrain,
            5 => Self::V3Unified,
            6 => Self::V4CabinetIr,
            7 => Self::V4SlotTransit,
            8 => Self::V4CoupledOde,
            9 => Self::V4CeldaCavity,
            10 => Self::V4ZapatillaValve,
            _ => Self::V4Unified,
        }
    }

    #[inline]
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    #[inline]
    pub fn is_v4_family(self) -> bool {
        (self as u8) >= (Self::V4CabinetIr as u8)
    }

    #[inline]
    pub fn use_v3_aero(self) -> bool {
        matches!(self, Self::V3AeroRadiation | Self::V3Unified) || self.is_v4_family()
    }

    #[inline]
    pub fn use_v3_beam(self) -> bool {
        matches!(self, Self::V3ClampedBar | Self::V3Unified) || self.is_v4_family()
    }

    #[inline]
    pub fn use_v3_tran(self) -> bool {
        matches!(self, Self::V3TransientEntrain | Self::V3Unified) || self.is_v4_family()
    }

    #[inline]
    pub fn use_v4_cabinet(self) -> bool {
        matches!(self, Self::V4CabinetIr | Self::V4Unified)
    }

    #[inline]
    pub fn use_v4_slot(self) -> bool {
        matches!(self, Self::V4SlotTransit | Self::V4Unified)
    }

    #[inline]
    pub fn use_v4_coupled(self) -> bool {
        matches!(self, Self::V4CoupledOde | Self::V4Unified)
    }

    #[inline]
    pub fn use_v4_celda(self) -> bool {
        matches!(self, Self::V4CeldaCavity | Self::V4Unified)
    }

    #[inline]
    pub fn use_v4_zapatilla(self) -> bool {
        matches!(self, Self::V4ZapatillaValve | Self::V4Unified)
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::V1Baseline => "v1",
            Self::V2AdaaBaffle => "v2",
            Self::V3AeroRadiation => "v3_aero",
            Self::V3ClampedBar => "v3_beam",
            Self::V3TransientEntrain => "v3_transient",
            Self::V3Unified => "v3",
            Self::V4CabinetIr => "v4_cabinet",
            Self::V4SlotTransit => "v4_slot",
            Self::V4CoupledOde => "v4_coupled",
            Self::V4CeldaCavity => "v4_celda",
            Self::V4ZapatillaValve => "v4_zapatilla",
            Self::V4Unified => "v4",
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
            "v3" | "v3_unified" | "5" => Some(Self::V3Unified),
            "v4_cabinet" | "v4_cabinet_ir" | "cabinet" | "6" => Some(Self::V4CabinetIr),
            "v4_slot" | "v4_slot_transit" | "slot" | "7" => Some(Self::V4SlotTransit),
            "v4_coupled" | "v4_coupled_ode" | "coupled" | "8" => Some(Self::V4CoupledOde),
            "v4_celda" | "v4_celda_cavity" | "celda" | "9" => Some(Self::V4CeldaCavity),
            "v4_zapatilla" | "v4_zapatilla_valve" | "zapatilla" | "10" => {
                Some(Self::V4ZapatillaValve)
            }
            "v4" | "v4_unified" | "unified" | "sota" | "11" => Some(Self::V4Unified),
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
            Self::V3Unified => "v3: Unified Physical Model (Aero + Beam + Transient)",
            Self::V4CabinetIr => "v4-Cabinet (Dir 1): Deconvolved 10-Mode Box & Early-Reflection FIR",
            Self::V4SlotTransit => "v4-Slot (Dir 2): 3-Regime Plate-Channel Slot Transit (Vowel Swell ADAA)",
            Self::V4CoupledOde => "v4-Coupled (Dir 3): 2nd-Order Coupled Reed ODE, Chirp & Celda Feedback",
            Self::V4CeldaCavity => "v4-Celda (Dir 4): Pitch-Scaled Individual Celda Cavity Resonator (Okada 2025)",
            Self::V4ZapatillaValve => "v4-Zapatilla (Dir 5): Leather Valve Thump & Closed-Chamber Muffled Ring-Down",
            Self::V4Unified => "v4: Unified SOTA Physical Model (Cabinet + Slot + Coupled ODE + Celda + Zapatilla)",
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

    // --- v4 Direction 2 (3-Regime Plate-Channel Slot Transit & Vowel Swell ADAA) ---
    /// 3-regime plate-channel transit pinch ADAA weight (`u / (1 + u^2)^(3/2)`).
    pub slot_pinch_adaa: f32,
    /// 3-regime channel transit duty-cycle notch depth on H3 and H5.
    pub slot_channel_notch: f32,
    /// Downstream plate emergence H4/H6 vowel swell gain as pressure rises.
    pub slot_emergence_swell: f32,
    /// Sub-baffle diffraction attenuation of 4' fundamental (H2) on low-compass reeds.
    pub low_bass_h2_tame: f32,

    // --- v4 Direction 3 (2nd-Order Coupled Reed Integrator, Startup Chirp & Celda Feedback) ---
    /// 2nd-order mechanical amplitude envelope momentum / underdamped carry-through.
    pub coupled_ode_momentum: f32,
    /// Audio-rate 8' -> 4' shared celda acoustic pressure cross-modulation.
    pub celda_cross_mod: f32,
    /// Amplitude-governed 4' vs 8' Hopf onset lead during soft/gradual attacks.
    pub hopf_onset_lead: f32,
    /// Transient hydrodynamic added-mass startup frequency chirp in cents.
    pub startup_chirp_cents: f32,

    // --- v4 Direction 4 (Okada 2025 Pitch-Scaled Individual Celda Cavity Resonator) ---
    /// Base individual celda cavity resonance frequency at manual bottom u=0 (Hz).
    pub celda_cavity_hz_base: f32,
    /// Octave scaling exponent of celda cavity frequency across u in [0, 1].
    pub celda_cavity_oct_slope: f32,
    /// Individual celda cavity resonator peak gain at u=0 (dB).
    pub celda_cavity_gain_db: f32,
    /// Slope of individual celda cavity resonator gain across u in [0, 1] (dB).
    pub celda_cavity_gain_slope_db: f32,
    /// Individual celda cavity resonator quality factor Q.
    pub celda_cavity_q: f32,

    // --- v4 Direction 5 (Leather Zapatilla Release Thump & Closed-Valve Muffled Ring-Down) ---
    /// Low-pass occlusion muffling when the leather pallet valve is partially closed.
    pub valve_occlusion_muffle: f32,
    /// Smooth pitch slope of mechanical pallet click transmission across manual compass `u`.
    pub pallet_click_u_slope: f32,
    /// Leather pallet (zapatilla) closing thump & staccato valve-detachment vortex puff gain.
    pub zapatilla_thump_gain: f32,
}

impl Default for ManualScalingParams {
    fn default() -> Self {
        BandoneonPreset::v2_adaa_baffle().right_cantabile
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

fn default_left_cabinet_modes() -> [BodyModeParam; 4] {
    [
        BodyModeParam {
            freq_hz: 185.0,
            q: 2.2,
            gain_db: 0.0,
        },
        BodyModeParam {
            freq_hz: 980.0,
            q: 1.8,
            gain_db: 0.0,
        },
        BodyModeParam {
            freq_hz: 2050.0,
            q: 1.8,
            gain_db: 0.0,
        },
        BodyModeParam {
            freq_hz: 6400.0,
            q: 1.2,
            gain_db: 0.0,
        },
    ]
}

fn default_right_cabinet_modes() -> [BodyModeParam; 4] {
    [
        BodyModeParam {
            freq_hz: 370.0,
            q: 2.0,
            gain_db: 0.0,
        },
        BodyModeParam {
            freq_hz: 1120.0,
            q: 1.8,
            gain_db: 0.0,
        },
        BodyModeParam {
            freq_hz: 2020.0,
            q: 1.6,
            gain_db: 0.0,
        },
        BodyModeParam {
            freq_hz: 3450.0,
            q: 1.5,
            gain_db: 0.0,
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

    /// 4-band auxiliary deconvolved cabinet modal bank for the Left Hand (Bajos) bass box (`v4_cabinet` & `v4`).
    #[serde(default = "default_left_cabinet_modes")]
    pub left_cabinet_modes: [BodyModeParam; 4],

    /// 4-band auxiliary deconvolved cabinet modal bank for the Right Hand (Cantabile) grill (`v4_cabinet` & `v4`).
    #[serde(default = "default_right_cabinet_modes")]
    pub right_cabinet_modes: [BodyModeParam; 4],

    /// Deconvolved wooden box early-reflection comb mix for Left Hand (Bajos) (`v4_cabinet` & `v4`).
    #[serde(default)]
    pub cabinet_ir_mix_left: f32,

    /// Deconvolved wooden box early-reflection comb mix for Right Hand (Cantabile) (`v4_cabinet` & `v4`).
    #[serde(default)]
    pub cabinet_ir_mix_right: f32,

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
    // --- v4 per-reed resolved parameters ---
    pub slot_pinch_adaa: f32,
    pub slot_channel_notch: f32,
    pub slot_emergence_swell: f32,
    pub coupled_ode_momentum: f32,
    pub startup_chirp_cents: f32,
    pub valve_occlusion_muffle: f32,
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
    // --- v4 per-note resolved parameters ---
    pub celda_cross_mod: f32,
    pub hopf_onset_lead: f32,
    pub celda_a1: f32,
    pub celda_a2: f32,
    pub celda_a3: f32,
    pub celda_gain: f32,
    pub zapatilla_thump_gain: f32,
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
            SynthVersion::V4CabinetIr => Self::v4_cabinet_ir(),
            SynthVersion::V4SlotTransit => Self::v4_slot_transit(),
            SynthVersion::V4CoupledOde => Self::v4_coupled_ode(),
            SynthVersion::V4CeldaCavity => Self::v4_celda_cavity(),
            SynthVersion::V4ZapatillaValve => Self::v4_zapatilla_valve(),
            SynthVersion::V4Unified => Self::v4_unified(),
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
                slot_pinch_adaa: 0.0,
                slot_channel_notch: 0.0,
                slot_emergence_swell: 0.0,
                low_bass_h2_tame: 0.0,
                coupled_ode_momentum: 0.0,
                celda_cross_mod: 0.0,
                hopf_onset_lead: 0.0,
                startup_chirp_cents: 0.0,
                celda_cavity_hz_base: 1150.0,
                celda_cavity_oct_slope: 2.40,
                celda_cavity_gain_db: 0.0,
                celda_cavity_gain_slope_db: 0.0,
                celda_cavity_q: 1.45,
                valve_occlusion_muffle: 0.0,
                pallet_click_u_slope: 0.0,
                zapatilla_thump_gain: 0.0,
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
                slot_pinch_adaa: 0.0,
                slot_channel_notch: 0.0,
                slot_emergence_swell: 0.0,
                low_bass_h2_tame: 0.0,
                coupled_ode_momentum: 0.0,
                celda_cross_mod: 0.0,
                hopf_onset_lead: 0.0,
                startup_chirp_cents: 0.0,
                celda_cavity_hz_base: 2400.0,
                celda_cavity_oct_slope: 2.50,
                celda_cavity_gain_db: 0.0,
                celda_cavity_gain_slope_db: 0.0,
                celda_cavity_q: 1.40,
                valve_occlusion_muffle: 0.0,
                pallet_click_u_slope: 0.0,
                zapatilla_thump_gain: 0.0,
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
            left_cabinet_modes: default_left_cabinet_modes(),
            right_cabinet_modes: default_right_cabinet_modes(),
            cabinet_ir_mix_left: 0.0,
            cabinet_ir_mix_right: 0.0,
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

    /// Unified v3 Model (`v3`): Combines v3 Directions 1 + 2 + 3 with calibrated physical scaling laws.
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

    /// v4 Direction 1 (`v4_cabinet`): Deconvolved Wooden Cabinet 10-Mode Resonator Bank & Early-Reflection FIR.
    pub fn v4_cabinet_ir() -> Self {
        let mut p = Self::v3_unified();
        p.name = "Alfred Arnold (AA) v4-Cabinet (Deconvolved 10-Mode Box & Early-Reflection FIR)".to_string();
        p.version = SynthVersion::V4CabinetIr;
        p.left_bajos.monopole_rad_slope = 0.15968865;
        p.right_cantabile.monopole_rad_slope = -0.22488368;
        p.left_cabinet_modes[1].gain_db = -0.037632;
        p.left_cabinet_modes[2].gain_db = 0.142464;
        p.right_cabinet_modes[2].gain_db = 0.037632;
        p
    }

    /// v4 Direction 2 (`v4_slot`): 3-Regime Plate-Channel Slot Transit Geometry (Moving Formant / Vowel Swell ADAA).
    pub fn v4_slot_transit() -> Self {
        let mut p = Self::v3_unified();
        p.name = "Alfred Arnold (AA) v4-Slot (3-Regime Plate-Channel Slot Transit & Vowel Swell ADAA)".to_string();
        p.version = SynthVersion::V4SlotTransit;
        p.left_bajos.slot_sharpness_slope = -1.8576001;
        p.left_bajos.slot_pinch_adaa = 0.01;
        p.left_bajos.slot_channel_notch = -0.0025088002;
        p.left_bajos.slot_emergence_swell = 0.0056;
        p.left_bajos.low_bass_h2_tame = 0.015;
        p
    }

    /// v4 Direction 3 (`v4_coupled`): True 2nd-Order Coupled Mechanical Reed Integrator, Startup Chirp & Shared Celda Feedback.
    pub fn v4_coupled_ode() -> Self {
        let mut p = Self::v3_unified();
        p.name = "Alfred Arnold (AA) v4-Coupled (2nd-Order Coupled Reed ODE, Startup Chirp & Celda Feedback)".to_string();
        p.version = SynthVersion::V4CoupledOde;
        p.pallet_click_gain = 0.00226272;
        p.left_bajos.reed_damping_base = 0.03843344;
        p.left_bajos.pallet_click_scale = 0.16600001;
        p.left_bajos.staccato_4ft_boost = 0.16841441;
        p.left_bajos.staccato_chiff_gain = 1.1960725;
        p.left_bajos.coupled_ode_momentum = 0.00672;
        p.left_bajos.hopf_onset_lead = -0.0056447997;
        p.left_bajos.startup_chirp_cents = 0.1568;
        p.right_cantabile.pallet_click_scale = 0.62702465;
        p
    }

    /// v4 Direction 4 (`v4_celda`): Pitch-Scaled Individual Celda Cavity Resonator (Okada 2025).
    pub fn v4_celda_cavity() -> Self {
        let mut p = Self::v3_unified();
        p.name = "Alfred Arnold (AA) v4-Celda (Pitch-Scaled Individual Celda Cavity Resonator)".to_string();
        p.version = SynthVersion::V4CeldaCavity;
        p.left_bajos.celda_cavity_gain_db = 0.13042176;
        p.left_bajos.celda_cavity_gain_slope_db = 0.042147838;
        p.left_bajos.celda_cavity_q = 1.3492;
        p
    }

    /// v4 Direction 5 (`v4_zapatilla`): Leather Pallet (Zapatilla) Release Thump & Closed-Valve Muffled Reed Ring-Down.
    pub fn v4_zapatilla_valve() -> Self {
        let mut p = Self::v3_unified();
        p.name = "Alfred Arnold (AA) v4-Zapatilla (Leather Valve Thump & Closed-Chamber Muffled Ring-Down)".to_string();
        p.version = SynthVersion::V4ZapatillaValve;
        p.left_bajos.staccato_chiff_gain = 1.0639919;
        p.left_bajos.zapatilla_thump_gain = 0.08400001;
        p.right_cantabile.pallet_click_scale = 0.62702465;
        p.right_cantabile.pallet_click_u_slope = 0.035;
        p
    }

    /// Unified SOTA v4 Model (`v4`): Integrates all 5 v4 physical modeling advances with co-calibrated scaling laws.
    pub fn v4_unified() -> Self {
        let mut p = Self::v3_unified();
        p.name = "Alfred Arnold (AA) Physical Model v4 (Deconvolved Cabinet + 3-Regime Slot + Coupled ODE + Celda + Zapatilla)".to_string();
        p.version = SynthVersion::V4Unified;
        // Direction 1 (Cabinet)
        p.left_bajos.monopole_rad_slope = 0.15968865;
        p.right_cantabile.monopole_rad_slope = -0.22488368;
        p.left_cabinet_modes[1].q = 1.688;
        p.left_cabinet_modes[1].gain_db = -0.157632;
        p.left_cabinet_modes[2].q = 2.112;
        p.left_cabinet_modes[2].gain_db = 0.142464;
        p.right_cabinet_modes[2].gain_db = 0.037632;
        // Direction 2 (Slot)
        p.left_bajos.slot_sharpness_slope = -1.8716494;
        p.left_bajos.slot_pinch_adaa = 0.01;
        p.left_bajos.slot_channel_notch = -0.0025088002;
        p.left_bajos.slot_emergence_swell = 0.0056;
        p.left_bajos.low_bass_h2_tame = 0.015;
        p.right_cantabile.pressure_brightness_mod = 3.0953624;
        // Direction 3 (Coupled ODE)
        p.pallet_click_gain = 0.00226272;
        p.left_bajos.reed_damping_base = 0.03843344;
        p.left_bajos.pallet_click_scale = 0.16600001;
        p.left_bajos.staccato_4ft_boost = 0.16841441;
        p.left_bajos.staccato_chiff_gain = 1.1960725;
        p.left_bajos.coupled_ode_momentum = 0.018720001;
        p.left_bajos.hopf_onset_lead = -0.0056447997;
        p.left_bajos.startup_chirp_cents = 0.1568;
        p.right_cantabile.pallet_click_scale = 0.62702465;
        p.right_cantabile.staccato_chiff_gain = 0.31241378;
        // Direction 4 (Celda Cavity)
        p.left_bajos.celda_cavity_hz_base = 1105.0;
        p.left_bajos.celda_cavity_gain_db = 0.13042176;
        p.left_bajos.celda_cavity_gain_slope_db = 0.06322175;
        p.left_bajos.celda_cavity_q = 1.3492;
        // Direction 5 (Zapatilla Valve)
        p.left_bajos.zapatilla_thump_gain = 0.08400001;
        p.right_cantabile.pallet_click_u_slope = 0.04114656;
        p
    }

    /// Creates a physically grounded default preset calibrated for an Alfred Arnold (AA) bandoneon.
    pub fn default_bandoneon() -> Self {
        Self::v4_unified()
    }

    /// Resolves smooth physical parameters for a given MIDI note and hand manual at 48 kHz.
    #[inline]
    pub fn resolve_note(&self, midi_note: u8, hand: Hand) -> ResolvedNoteParams {
        self.resolve_note_at_rate(midi_note, hand, 48000.0)
    }

    /// Resolves smooth physical parameters for a given MIDI note, hand manual, and audio sample rate.
    #[inline]
    pub fn resolve_note_at_rate(
        &self,
        midi_note: u8,
        hand: Hand,
        sample_rate: f32,
    ) -> ResolvedNoteParams {
        let sr = sample_rate.max(8000.0);
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

        let use_aero = self.version.use_v3_aero();
        let use_beam = self.version.use_v3_beam();
        let use_tran = self.version.use_v3_tran();
        let use_slot = self.version.use_v4_slot();
        let use_coupled = self.version.use_v4_coupled();
        let use_celda = self.version.use_v4_celda();
        let use_zapatilla = self.version.use_v4_zapatilla();

        let click_u_mod = if use_zapatilla {
            (1.0 + m.pallet_click_u_slope.clamp(-1.5, 1.5) * (u - 0.42)).clamp(0.25, 2.5)
        } else {
            1.0
        };
        let pallet_click_scale = (m.pallet_click_scale * click_u_mod).clamp(0.02, 4.0);
        let air_noise_scale = (m.air_noise_base + m.air_noise_slope * u).clamp(0.05, 4.0);

        let monopole_rad_boost = if use_aero {
            (m.monopole_rad_boost + m.monopole_rad_slope * u).clamp(-0.6, 3.5)
        } else {
            0.0
        };
        let dipole_baffle_mix = if use_aero {
            m.dipole_baffle_mix.clamp(-0.6, 1.0)
        } else {
            0.0
        };
        let duty_bias = if use_beam {
            m.duty_bias_base.clamp(-0.20, 0.85)
        } else {
            0.0
        };
        let clamped_bar_h6_gain = if use_beam {
            m.clamped_bar_h6_gain.clamp(-0.25, 0.40)
        } else {
            0.0
        };
        let h3_notch_inject = if use_beam {
            m.h3_notch_inject.clamp(-0.25, 0.40)
        } else {
            0.0
        };
        let hopf_8ft_knee = if use_tran {
            m.hopf_8ft_knee.clamp(0.0, 0.85)
        } else {
            0.0
        };
        let staccato_4ft_boost = if use_tran {
            m.staccato_4ft_boost.clamp(-0.40, 2.0)
        } else {
            0.0
        };
        let staccato_chiff_gain = if use_tran {
            m.staccato_chiff_gain.clamp(0.0, 5.0)
        } else {
            0.0
        };
        let pressure_brightness_mod = m.pressure_brightness_mod.clamp(0.0, 8.0);

        let slot_pinch_adaa = if use_slot {
            m.slot_pinch_adaa.clamp(-0.4, 0.8)
        } else {
            0.0
        };
        let slot_channel_notch = if use_slot {
            m.slot_channel_notch.clamp(-0.3, 0.4) * (1.0 - 0.55 * u)
        } else {
            0.0
        };
        let slot_emergence_swell = if use_slot {
            m.slot_emergence_swell.clamp(-0.3, 0.5) * (0.35 + 0.65 * u)
        } else {
            0.0
        };
        let low_bass_h2_tame = if use_slot {
            m.low_bass_h2_tame.clamp(-0.4, 0.8) * (1.0 - u).powi(2)
        } else {
            0.0
        };

        let coupled_ode_momentum = if use_coupled {
            m.coupled_ode_momentum.clamp(-0.4, 0.6)
        } else {
            0.0
        };
        let startup_chirp_cents = if use_coupled {
            m.startup_chirp_cents.clamp(-15.0, 15.0)
        } else {
            0.0
        };
        let valve_occlusion_muffle = if use_zapatilla {
            m.valve_occlusion_muffle.clamp(0.0, 0.88) * (1.0 - 0.35 * u)
        } else {
            0.0
        };

        let hopf_lead_eff = if use_coupled {
            m.hopf_onset_lead.clamp(-0.5, 0.85)
        } else {
            0.0
        };
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
                0.95 * (1.0 - 0.18 * hopf_8ft_knee) * (1.0 - 0.25 * hopf_lead_eff).clamp(0.55, 1.35)
            } else {
                0.95
            },
            monopole_rad_boost,
            dipole_baffle_mix,
            c_lin: duty_bias,
            clamped_bar_h6_gain,
            h3_notch_inject,
            hopf_knee: if use_tran { hopf_8ft_knee } else { 0.0 },
            slot_pinch_adaa,
            slot_channel_notch,
            slot_emergence_swell,
            coupled_ode_momentum,
            startup_chirp_cents,
            valve_occlusion_muffle,
        };

        let p_ext_4ft = 0.72 * p_th_4ft;
        let dipole_4ft = dipole_baffle_mix * 0.85;
        // On low-compass notes, `low_bass_h2_tame` raises the 4' sub-baffle diffraction cutoff
        // (attenuating excess 2f0 H2 boom) while enriching 4' even constriction (H4, H6).
        let baffle_4ft = baffle_cutoff_hz * (0.85 + 0.45 * low_bass_h2_tame);
        let asym_4ft = (reed_asymmetry * (0.95 + 0.75 * low_bass_h2_tame)).clamp(0.02, 1.35);
        let reed_4ft = ResolvedReedParams {
            p_th: p_th_4ft,
            p_ext: p_ext_4ft,
            inv_one_minus_p_ext: 1.0 / (1.0 - p_ext_4ft).max(0.1),
            reed_asymmetry: asym_4ft,
            slot_sharpness: slot_sharpness * (0.90 + 0.18 * low_bass_h2_tame),
            pressure_brightness_mod: pressure_brightness_mod * 0.95,
            baffle_cutoff_hz: baffle_4ft,
            jet_corner_hz: jet_corner_hz * 1.15,
            reed_damping: reed_damping * 1.10,
            piston_coupling: piston_coupling * 1.25,
            pitch_bend_cents: pitch_bend_cents * 0.50,
            inertia_mult: if use_tran {
                2.05 * (1.0 + 0.15 * hopf_8ft_knee) * (1.0 + 0.40 * hopf_lead_eff).clamp(0.65, 1.75)
            } else {
                2.05
            },
            monopole_rad_boost: monopole_rad_boost * 1.15,
            dipole_baffle_mix: dipole_4ft,
            c_lin: duty_bias * 0.50,
            clamped_bar_h6_gain: clamped_bar_h6_gain * (0.65 + 0.65 * low_bass_h2_tame),
            h3_notch_inject: h3_notch_inject * 0.35,
            hopf_knee: 0.0,
            slot_pinch_adaa: slot_pinch_adaa * 0.60,
            slot_channel_notch: slot_channel_notch * 0.30,
            slot_emergence_swell: slot_emergence_swell * 1.15,
            coupled_ode_momentum: coupled_ode_momentum * 1.15,
            startup_chirp_cents: startup_chirp_cents * 0.50,
            valve_occlusion_muffle: valve_occlusion_muffle * 0.85,
        };

        // Precompute Okada (2025) pitch-scaled individual celda cavity SVF peaking coefficients
        let (celda_a1, celda_a2, celda_a3, celda_gain) = if use_celda {
            let f_celda = (m.celda_cavity_hz_base
                * 2.0f32.powf(m.celda_cavity_oct_slope.clamp(-1.0, 3.5) * u))
            .clamp(180.0, (sr * 0.42).min(14000.0));
            let q_celda = m.celda_cavity_q.clamp(0.6, 8.0);
            let gain_db = (m.celda_cavity_gain_db + m.celda_cavity_gain_slope_db * u).clamp(-12.0, 12.0);
            let g = (core::f32::consts::PI * f_celda / sr).tan().clamp(0.005, 1.2);
            let k = 1.0 / q_celda;
            let a1 = 1.0 / (1.0 + g * (g + k));
            let a2 = g * a1;
            let a3 = g * a2;
            let lin_gain = (10.0f32.powf(gain_db / 20.0) - 1.0) * k;
            (a1, a2, a3, lin_gain)
        } else {
            (0.0, 0.0, 0.0, 0.0)
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
            celda_cross_mod: if use_coupled {
                m.celda_cross_mod.clamp(-0.4, 0.6)
            } else {
                0.0
            },
            hopf_onset_lead: hopf_lead_eff,
            celda_a1,
            celda_a2,
            celda_a3,
            celda_gain,
            zapatilla_thump_gain: if use_zapatilla {
                m.zapatilla_thump_gain.clamp(0.0, 3.0)
            } else {
                0.0
            },
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
        self.left_cabinet_modes = src.left_cabinet_modes;
        self.right_cabinet_modes = src.right_cabinet_modes;
        self.cabinet_ir_mix_left = src.cabinet_ir_mix_left;
        self.cabinet_ir_mix_right = src.cabinet_ir_mix_right;
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
