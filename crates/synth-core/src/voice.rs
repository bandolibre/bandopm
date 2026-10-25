use crate::preset::{BandoneonPreset, Hand, ResolvedNoteParams};
use crate::reed::FreeReed;

/// `v5`: the fraction of the bellows pressure that reaches the celda for a pallet `lift` in
/// `[0, 1]` and a reed slot area `slot_ratio` times the open pallet's. The pallet gap and the
/// reed slot are two orifices in series, flow ~ area * sqrt(drop), so the celda gets
/// `L^2 / (L^2 + r^2)`, normalised to 1 at full lift.
#[inline]
pub fn pallet_fill(lift: f32, slot_ratio: f32) -> f32 {
    let r2 = slot_ratio * slot_ratio;
    let l2 = lift * lift;
    l2 * (1.0 + r2) / (l2 + r2)
}

/// Single bandoneon button voice modeling a shared reed chamber (*celda*), pallet valve,
/// coupled 8' (primary) and 4' (octave) free reeds with mode-locked acoustic beating,
/// pitch-scaled *celda* cavity resonance (*Okada 2025*), leather *zapatilla* valve acoustics,
/// and Curle dipole cyclostationary aeroacoustic turbulence.
#[derive(Debug, Clone)]
pub struct BandoneonVoice {
    pub active: bool,
    pub key_down: bool,
    pub midi_note: u8,
    pub hand: Hand,
    pub age: u64,
    pub velocity_norm: f32,

    /// Resolved physical parameters for this note.
    params: ResolvedNoteParams,

    /// Pallet valve aperture `A_valve(t) in [0.0, 1.0]`: the fraction of the bellows pressure
    /// the reeds get. From `v5` it is the celda's filling, `celda_fill`.
    pub valve_aperture: f32,
    prev_valve_aperture: f32,
    valve_open_coeff: f32,
    valve_close_coeff: f32,
    /// `v5`: pallet lift in `[0.0, 1.0]`, moving at constant speed (the key's).
    pub pallet_lift: f32,
    /// `v5`: lift added per sample while opening, and removed per sample while closing.
    lift_open_inc: f32,
    lift_close_inc: f32,
    /// Per-note pressure from the note-on velocity (`VelocityMode::Loudness`), scaled by the
    /// bellows; `< 0.0` when the bellows alone drives the note.
    velocity_pressure: f32,
    /// Note-off received while the sustain pedal was down: released when the pedal lifts.
    pub sustained: bool,
    /// Note-off velocity kept for the release of a sustained note.
    pub sustained_release_velocity: u8,

    /// Mechanical pallet valve click envelope.
    click_env: f32,
    click_filter_z1: f32,
    /// Leather pallet (*zapatilla*) Note-Off seating thump envelope & filter states (`v4_zapatilla` / `v4`).
    zapatilla_env: f32,
    zapatilla_z1: f32,
    zapatilla_z2: f32,
    /// Air turbulence band-pass filter state.
    noise_bp_z1: f32,
    noise_bp_z2: f32,

    /// Individual *celda* cavity 2-pole trapezoidal SVF states (`v4_celda` / `v4`).
    celda_ic1eq: f32,
    celda_ic2eq: f32,

    /// Shared local reed chamber (*celda*) pressure drop state.
    chamber_drop: f32,
    /// Aeroacoustic chamber beating phase `[0, 1)` between coupled 8' and 4' reeds.
    beat_phase: f32,
    /// Staccato / initial attack vortex-shedding transient envelope.
    onset_transient_env: f32,
    /// Latched step-attack scale for staged 8'->4' onset when NoteOn opens into a pressurized bellows.
    step_attack_scale: f32,
    /// Peak smoothed pressure seen by this voice (stabilizes 4'/8' ratio during attack).
    peak_voice_pressure: f32,
    /// Per-note Polyphonic Key Pressure / MPE pressure target (`< 0.0` when driven by global `CC#11`).
    note_pressure_target: f32,
    /// Audio-rate one-pole smoothed per-note pressure in `[0.0, 1.5]`.
    note_pressure_current: f32,
    /// Elapsed audio samples since `NoteOn` (used to gate post-click vortex shedding).
    voice_samples: u32,

    /// Primary 8' reed (`f0`).
    reed_8ft: FreeReed,
    /// Octave 4' reed (`2 * f0 + locked_detune_hz`).
    reed_4ft: FreeReed,

    /// Deterministic PRNG state for per-voice air turbulence & click texture.
    rng_state: u32,
}

impl BandoneonVoice {
    pub fn new(seed: u32) -> Self {
        Self {
            active: false,
            key_down: false,
            midi_note: 60,
            hand: Hand::RightCantabile,
            age: 0,
            velocity_norm: 0.0,
            params: BandoneonPreset::default_bandoneon().resolve_note(60, Hand::RightCantabile),
            valve_aperture: 0.0,
            prev_valve_aperture: 0.0,
            valve_open_coeff: 0.01,
            valve_close_coeff: 0.005,
            pallet_lift: 0.0,
            lift_open_inc: 0.01,
            lift_close_inc: 0.005,
            velocity_pressure: -1.0,
            sustained: false,
            sustained_release_velocity: 64,
            click_env: 0.0,
            click_filter_z1: 0.0,
            zapatilla_env: 0.0,
            zapatilla_z1: 0.0,
            zapatilla_z2: 0.0,
            noise_bp_z1: 0.0,
            noise_bp_z2: 0.0,
            celda_ic1eq: 0.0,
            celda_ic2eq: 0.0,
            chamber_drop: 0.0,
            beat_phase: 0.0,
            onset_transient_env: 0.0,
            step_attack_scale: 0.0,
            peak_voice_pressure: 0.0,
            note_pressure_target: -1.0,
            note_pressure_current: 0.0,
            voice_samples: 0,
            reed_8ft: FreeReed::new(261.63, 0.0),
            reed_4ft: FreeReed::new(523.25, 0.25),
            rng_state: seed.max(1),
        }
    }

    /// Re-resolves note parameters when the preset or synthesis version is switched live.
    #[inline]
    pub fn refresh_preset(&mut self, preset: &BandoneonPreset, sample_rate: f32) {
        if self.active {
            self.params = preset.resolve_note_at_rate(self.midi_note, self.hand, sample_rate);
        }
    }

    #[inline]
    fn next_noise(&mut self) -> f32 {
        // Xorshift32 fast real-time PRNG -> [-1.0, 1.0]
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng_state = x;
        ((x as i32) as f32) * (1.0 / 2147483648.0)
    }

    /// `velocity_pressure` is the note's own pressure from its velocity
    /// (`VelocityMode::Loudness`), or `< 0.0` to follow the bellows alone.
    #[allow(clippy::too_many_arguments)]
    pub fn trigger_note_on(
        &mut self,
        midi_note: u8,
        velocity: u8,
        hand: Hand,
        sample_rate: f32,
        preset: &BandoneonPreset,
        age: u64,
        reference_hz: f32,
        velocity_pressure: f32,
    ) {
        let same_button = self.active && self.midi_note == midi_note && self.hand == hand;
        let pallet = preset.version.use_v5_vivo();
        self.active = true;
        self.key_down = true;
        self.sustained = false;
        self.velocity_pressure = velocity_pressure;
        self.midi_note = midi_note;
        self.hand = hand;
        self.age = age;
        let vel_norm = (velocity.clamp(1, 127) as f32) / 127.0;
        self.velocity_norm = vel_norm;
        let sr = sample_rate.max(8000.0);
        self.params = preset.resolve_note_at_rate(midi_note, hand, sr);

        let f0 = reference_hz * 2.0f32.powf((midi_note as f32 - 69.0) / 12.0);
        // Alfred Arnold dry (*a seco*) tuning with intra-celda 1:2 entrainment:
        // Small carrier detuning (0.14 Hz) keeps all harmonics H2..H16 aligned within the harmonic bins,
        // while aeroacoustic chamber precession (`beat_phase`) produces the characteristic beating modulation.
        let f_4ft = 2.0 * f0 + 0.14;

        self.reed_8ft.reset(f0, 0.0);
        self.reed_4ft.reset(f_4ft, 0.17);
        self.celda_ic1eq = 0.0;
        self.celda_ic2eq = 0.0;
        self.zapatilla_env = 0.0;
        self.zapatilla_z1 = 0.0;
        self.zapatilla_z2 = 0.0;
        self.chamber_drop = 0.0;
        self.beat_phase = 0.15;
        if !same_button {
            self.valve_aperture = 0.0;
            self.pallet_lift = 0.0;
            self.click_filter_z1 = 0.0;
            self.noise_bp_z1 = 0.0;
            self.noise_bp_z2 = 0.0;
        }
        self.prev_valve_aperture = self.valve_aperture;
        self.step_attack_scale = 0.0;
        self.note_pressure_target = -1.0;
        self.note_pressure_current = 0.0;
        self.voice_samples = 0;

        if pallet {
            // v5: velocity is the key's speed, so it sets only how fast the pallet lifts. The
            // onset transients follow that opening speed (1.0 at the fastest), and the 4' reed
            // weighting starts from the pressure alone.
            let open_ms = preset.pallet_open_ms(velocity);
            let fastest_ms = preset.pallet_open_ms(127);
            self.lift_open_inc = 1.0 / (open_ms * 0.001 * sr).max(1.0);
            self.lift_close_inc = 1.0 / (preset.pallet_close_ms(64) * 0.001 * sr).max(1.0);
            self.onset_transient_env = (fastest_ms / open_ms).clamp(0.0, 1.0);
            self.peak_voice_pressure = 0.25;
        } else {
            self.onset_transient_env = vel_norm * vel_norm;
            // High-velocity staccato attacks strongly excite the lower-mass 4' octave reed
            self.peak_voice_pressure = 0.25 + 1.05 * vel_norm * vel_norm;

            // Higher velocity opens the pallet valve faster
            let open_ms: f32 = (preset.valve_open_ms * (1.35 - 0.75 * vel_norm)).max(0.6);
            let close_ms: f32 = preset.valve_close_ms.max(1.0);
            self.valve_open_coeff = (1.0 - (-1.0 / (open_ms * 0.001 * sr).max(1.0)).exp())
                .clamp(0.0005, 0.5);
            self.valve_close_coeff =
                (1.0 - (-1.0 / (close_ms * 0.001 * sr).max(1.0)).exp())
                    .clamp(0.0002, 0.5);
        }

        // Pallet valve click scales with velocity^1.6 and manual transmission scale
        self.click_env =
            vel_norm.powf(1.6) * preset.pallet_click_gain * self.params.pallet_click_scale;
    }

    /// Sets per-note Polyphonic Key Pressure / MPE expression (`0..=127`) for independent multi-touch/MPE voices.
    #[inline]
    pub fn set_note_pressure(&mut self, pressure: u8, curve_gamma: f32) {
        let norm = (pressure.min(127) as f32) / 127.0;
        let p = if norm <= 0.0 {
            0.0
        } else {
            norm.powf(curve_gamma)
        };
        if self.note_pressure_target < 0.0 {
            self.note_pressure_current = p;
        }
        self.note_pressure_target = p;
    }

    /// Clears per-note pressure override so the voice returns to following global `CC#11`.
    #[inline]
    pub fn clear_note_pressure(&mut self) {
        self.note_pressure_target = -1.0;
    }

    pub fn trigger_note_off(&mut self, release_velocity: u8, preset: &BandoneonPreset, sample_rate: f32) {
        if self.key_down {
            self.key_down = false;
            self.sustained = false;
            if preset.version.use_v5_vivo() {
                // v5: the release velocity is the key's speed back up, so it sets how fast the
                // pallet closes.
                let close_ms = preset.pallet_close_ms(release_velocity);
                self.lift_close_inc = 1.0 / (close_ms * 0.001 * sample_rate.max(8000.0)).max(1.0);
            }
            let rel_norm = (release_velocity.min(127) as f32) / 127.0;
            self.click_env += 0.35
                * rel_norm
                * preset.pallet_click_gain
                * self.params.pallet_click_scale;
            if preset.version.use_v4_zapatilla() {
                self.zapatilla_env += (0.28 + 0.72 * rel_norm)
                    * preset.pallet_click_gain
                    * self.params.zapatilla_thump_gain;
            }
        }
    }

    /// Renders a single sample for this voice. Returns `(audio_sample, total_orifice_flow)`.
    #[inline(always)]
    pub fn tick(
        &mut self,
        sample_rate: f32,
        dt: f32,
        inv_sr_38: f32,
        inv_sr_42: f32,
        bellows_pressure: f32,
        preset: &BandoneonPreset,
    ) -> (f32, f32) {
        if !self.active {
            return (0.0, 0.0);
        }

        let version = preset.version;
        self.voice_samples = self.voice_samples.saturating_add(1);
        let bellows_pressure = if self.note_pressure_target >= 0.0 {
            self.note_pressure_current +=
                0.008 * (self.note_pressure_target - self.note_pressure_current);
            self.note_pressure_current
        } else if self.velocity_pressure >= 0.0 {
            self.velocity_pressure * bellows_pressure
        } else {
            bellows_pressure
        };

        // 1. Update pallet valve aperture A_valve(t)
        self.prev_valve_aperture = self.valve_aperture;
        let mut pallet_jet = 0.0f32;
        if version.use_v5_vivo() {
            // v5: the pallet lifts and closes at constant speed. The celda gets `pallet_fill`
            // of the bellows pressure: almost nothing while the gap is a slit, then a steep rise
            // that saturates long before full lift. It fills through a short lag for its air
            // volume.
            if self.key_down {
                self.pallet_lift = (self.pallet_lift + self.lift_open_inc).min(1.0);
            } else {
                self.pallet_lift = (self.pallet_lift - self.lift_close_inc).max(0.0);
            }
            if self.pallet_lift >= 1.0 && self.valve_aperture >= 0.99999 {
                // Fully open and filled: the common held-note case costs nothing more.
                self.valve_aperture = 1.0;
            } else {
                let r = self.params.pallet_slot_ratio;
                let fill_target = pallet_fill(self.pallet_lift, r);
                self.valve_aperture +=
                    self.params.celda_fill_coeff * (fill_target - self.valve_aperture);
                if self.pallet_lift == 0.0 && self.valve_aperture < 1e-5 {
                    self.valve_aperture = 0.0;
                }
                // Air jet through the half-open pallet: the gap's flow times the pressure drop
                // across it. It peaks while the lift is about r and vanishes once fully open.
                pallet_jet = self.pallet_lift * (1.0 - fill_target).max(0.0) * (2.0 / r)
                    * bellows_pressure;
            }
        } else if self.key_down {
            if self.valve_aperture < 0.9999 {
                self.valve_aperture += self.valve_open_coeff * (1.0 - self.valve_aperture);
            } else {
                self.valve_aperture = 1.0;
            }
        } else {
            self.valve_aperture *= 1.0 - self.valve_close_coeff;
            if self.valve_aperture < 1e-5 {
                self.valve_aperture = 0.0;
            }
        }
        let d_valve = (self.valve_aperture - self.prev_valve_aperture).abs() * sample_rate * 0.01;
        // Decay initial vortex-shedding transient (~45 ms time constant: active in stacc 15..100ms, zero in sustain >270ms)
        if self.onset_transient_env > 0.0 {
            self.onset_transient_env *= (1.0 - 22.0805 * dt).max(0.0);
            if self.onset_transient_env < 2.5e-3 {
                self.onset_transient_env = 0.0;
            }
        }

        // 2. Local reed chamber (*celda*) pressure drop coupling between 8' and 4' reeds
        let local_delta_p = (bellows_pressure - self.chamber_drop).max(0.0);
        if local_delta_p > self.peak_voice_pressure {
            self.peak_voice_pressure += 0.01 * (local_delta_p - self.peak_voice_pressure);
        } else {
            self.peak_voice_pressure += 0.0005 * (local_delta_p - self.peak_voice_pressure);
        }

        // Fast branchless parabolic sine approximation on beat_phase in [0, 1)
        let (beat_rate, beat_depth) = if version.use_v5_vivo() && self.voice_samples > 17040 {
            (self.params.octave_beat_hz * 0.56, 0.12)
        } else {
            (self.params.octave_beat_hz, 0.045)
        };
        self.beat_phase += beat_rate * dt;
        if self.beat_phase >= 1.0 {
            self.beat_phase -= 1.0;
        }
        let bp_centered = 2.0 * self.beat_phase - 1.0; // [-1, 1)
        let beat_sin = 4.0 * bp_centered * (1.0 - bp_centered.abs());
        let beat_mod = 1.0 + beat_depth * beat_sin;

        // 3. Step 8' primary reed
        let (flow_8ft, rad_8ft) = self.reed_8ft.tick(
            version,
            sample_rate,
            dt,
            inv_sr_38,
            inv_sr_42,
            local_delta_p,
            self.valve_aperture,
            &self.params.reed_8ft,
        );

        // 4. Step 4' octave reed (if active on this register)
        let p_4ft = local_delta_p * beat_mod;
        let (flow_4ft, mut rad_4ft) = if self.params.octave_4ft_gain > 1e-3 {
            self.reed_4ft.tick(
                version,
                sample_rate,
                dt,
                inv_sr_38,
                inv_sr_42,
                p_4ft,
                self.valve_aperture,
                &self.params.reed_4ft,
            )
        } else {
            (0.0, 0.0)
        };
        if version.use_v4_coupled() {
            rad_4ft *= 1.0 + self.params.celda_cross_mod * flow_8ft;
        }

        // Dynamic 4'-to-8' coupling gain uses smoothed `peak_voice_pressure` so it doesn't
        // square/warp the within-note attack rise time contour.
        let vel_excess = (self.velocity_norm - 0.34).max(0.0);
        // v5 drops this: key speed shapes only the pallet, and the 4' brightness of a hard
        // attack comes from the faster pressure rise.
        let vel_4ft_boost = if version.use_v3_tran() && !version.use_v5_vivo() {
            1.0 + self.params.staccato_4ft_boost
                * (0.45 * self.velocity_norm * self.velocity_norm + 1.35 * vel_excess)
        } else {
            1.0
        };
        let dyn_4ft_gain = self.params.octave_4ft_gain
            * (0.62 + self.params.octave_4ft_pressure_boost * self.peak_voice_pressure)
            * vel_4ft_boost;

        let total_flow = flow_8ft + dyn_4ft_gain * flow_4ft;
        let target_drop =
            (preset.chamber_coupling * 0.12 * total_flow.abs()).min(bellows_pressure * 0.35);
        self.chamber_drop += 0.15 * (target_drop - self.chamber_drop);
        if self.chamber_drop.abs() < 1e-12 {
            self.chamber_drop = 0.0;
        }

        // 5. Mechanical pallet valve click, leather *zapatilla* release thump & Curle dipole turbulence
        let white = self.next_noise();
        let mut mech_noise = 0.0f32;
        if self.click_env > 1e-6 {
            let click_cut = match self.hand {
                Hand::LeftBajos => 0.05,
                Hand::RightCantabile => 0.14,
            };
            self.click_filter_z1 += click_cut * (white - self.click_filter_z1);
            let p_scale = (0.12 + 0.88 * local_delta_p).clamp(0.12, 1.2);
            mech_noise += self.click_env * p_scale * self.click_filter_z1;
            self.click_env *= 0.985;
        } else {
            self.click_env = 0.0;
        }

        if self.zapatilla_env > 1e-6 {
            let thump_cut = match self.hand {
                Hand::LeftBajos => 0.022,
                Hand::RightCantabile => 0.035,
            };
            self.zapatilla_z1 += thump_cut * (white - self.zapatilla_z1);
            let thump_bp = self.zapatilla_z1 - self.zapatilla_z2;
            self.zapatilla_z2 += 0.007 * thump_bp;
            mech_noise += self.zapatilla_env * thump_bp;
            self.zapatilla_env *= 0.992;
        } else {
            self.zapatilla_env = 0.0;
        }

        // Cyclostationary jet turbulence proportional to instantaneous reed flow (Curle dipole law)
        // plus transient vortex shedding and post-click orifice leakage chiff during staccato (18..100ms).
        let flow_env = self.reed_8ft.osc_amp() + 0.5 * self.reed_4ft.osc_amp();
        let vortex_burst = (d_valve + 1.85 * self.onset_transient_env) * flow_env;
        let (post_click_gate, leak_chiff, gain_8ft, gain_4ft) = if self.onset_transient_env > 0.0 {
            let elapsed_s = self.voice_samples as f32 * dt;
            let pc_gate = ((elapsed_s - 0.0175) * 120.0).clamp(0.0, 1.0);
            let chiff = if version.use_v3_tran() {
                let base_chiff = self.params.staccato_chiff_gain
                    * pc_gate
                    * self.valve_aperture
                    * flow_env
                    * self.onset_transient_env;
                let zapatilla_chiff = if version.use_v4_zapatilla() {
                    let gap_jet = 0.55 * local_delta_p.sqrt() + 0.45 * flow_env;
                    self.params.zapatilla_thump_gain
                        * pc_gate
                        * self.valve_aperture
                        * gap_jet
                        * self.onset_transient_env
                } else {
                    0.0
                };
                base_chiff + zapatilla_chiff
            } else {
                0.0
            };
            let (g8, g4) = if version.use_v3_tran() && elapsed_s < 0.15 {
                let hopf_knee = self.params.reed_8ft.hopf_knee;
                let time_entrain = hopf_knee * (1.0 - elapsed_s * 62.5).clamp(0.0, 1.0);
                let bloom_8ft = self.reed_8ft.harmonic_bloom();
                let amp_entrain = if version.use_v4_coupled() {
                    let sustain_gate = (1.0 - (elapsed_s - 0.025).max(0.0) * 10.0).clamp(0.0, 1.0);
                    (1.0 - bloom_8ft) * sustain_gate * self.params.hopf_onset_lead
                } else {
                    0.0
                };
                let w_hopf = (time_entrain + 1.8 * amp_entrain).clamp(-0.85, 1.65);
                let g8_base = (1.0 - 0.14 * w_hopf).clamp(0.40, 1.25);
                let g4_base = dyn_4ft_gain * (1.0 + 0.18 * w_hopf).max(0.25);
                if version.use_v5_vivo() && elapsed_s < 0.032 {
                    if self.voice_samples == 1 {
                        // v5 has no velocity bump on the bellows: its pressure is already steady.
                        self.step_attack_scale = if bellows_pressure >= 0.135 {
                            self.onset_transient_env
                        } else {
                            0.0
                        };
                    }
                    let env_scale = self.step_attack_scale;
                    if elapsed_s < 0.014 {
                        // Stage 1 (0..14ms): Grave 8' blade starts first while 4' high blade is held back
                        // (total broadband RMS stays <= 1.0 while 8' dominates by +5.2 dB)
                        let s1 = (1.0 - elapsed_s * 71.42857) * env_scale;
                        (g8_base * (1.0 + 0.08 * s1), g4_base * (1.0 - 0.45 * s1))
                    } else {
                        // Stage 2 (14..32ms): High 4' blade fires distinct secondary attack (+9.8 dB 4'/8' shift from Stage 1),
                        // while 8' dips so broadband RMS never overshoots 1.0, then both reeds blend at >32ms
                        let u2 = (elapsed_s - 0.014) * 55.555557;
                        let bell2 = 4.0 * u2 * (1.0 - u2) * env_scale;
                        (g8_base * (1.0 - 0.22 * bell2), g4_base * (1.0 + 0.24 * bell2))
                    }
                } else {
                    (g8_base, g4_base)
                }
            } else {
                (1.0, dyn_4ft_gain)
            };
            (pc_gate, chiff, g8, g4)
        } else {
            (1.0, 0.0, 1.0, dyn_4ft_gain)
        };
        // The pallet jet blows while the celda is still nearly empty, so it is not scaled by it.
        let aero_drive =
            (flow_env * self.params.air_noise_scale + 7.5 * vortex_burst + leak_chiff)
                * self.valve_aperture
                + preset.pallet_jet_gain * pallet_jet;
        if aero_drive > 1e-5 && preset.air_turbulence_gain > 0.0 {
            self.noise_bp_z1 += 0.24 * (white - self.noise_bp_z1);
            let hp = self.noise_bp_z1 - self.noise_bp_z2;
            self.noise_bp_z2 += 0.025 * hp;
            mech_noise += preset.air_turbulence_gain * aero_drive * hp;
        }

        // Check if voice has completely decayed after Note Off
        if !self.key_down
            && self.valve_aperture == 0.0
            && self.click_env == 0.0
            && self.zapatilla_env == 0.0
            && self.reed_8ft.is_silent()
            && self.reed_4ft.is_silent()
        {
            self.active = false;
            return (0.0, 0.0);
        }

        let reed_tonal = gain_8ft * rad_8ft + gain_4ft * rad_4ft;

        // 6. Pitch-scaled individual *celda* cavity resonator (Okada 2025, `v4_celda` / `v4`)
        let reed_radiated = if self.params.celda_gain.abs() > 1e-6 {
            let v3 = reed_tonal - self.celda_ic2eq;
            let v1 = self.params.celda_a1 * self.celda_ic1eq + self.params.celda_a2 * v3;
            let v2 = self.celda_ic2eq
                + self.params.celda_a2 * self.celda_ic1eq
                + self.params.celda_a3 * v3;
            self.celda_ic1eq = 2.0 * v1 - self.celda_ic1eq + 1e-20;
            self.celda_ic2eq = 2.0 * v2 - self.celda_ic2eq;
            reed_tonal + (self.params.celda_gain * post_click_gate) * v1
        } else {
            reed_tonal
        };

        let reed_mix = reed_radiated + mech_noise;
        (reed_mix, total_flow)
    }
}

