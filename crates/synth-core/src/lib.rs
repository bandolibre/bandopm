//! `synth-core`: Platform-agnostic real-time physical modeling bandoneon DSP engine.
//!
//! Designed for the **Bandolibre** open-source MIDI bandoneon controller:
//! - Zero heap allocations, zero locks, and zero I/O inside the audio render loop.
//! - Coupled 8' + 4' blown-closed free-reed oscillators with unsteady Bernoulli orifice flow.
//! - Separate Left-Hand (*Bajos*) and Right-Hand (*Cantabile*) physical scaling laws.
//! - 6-mode wooden *cabezal* and bellows enclosure resonator bank.

pub mod bellows;
pub mod body;
pub mod post;
pub mod preset;
pub mod reed;
pub mod voice;

pub use bellows::BellowsModel;
pub use body::BodyResonator;
pub use post::PostProcessor;
pub use preset::{
    BandoneonPreset, BodyModeParam, Hand, KeyboardMode, ManualScalingParams, ResolvedNoteParams,
    SynthVersion,
};
pub use voice::BandoneonVoice;

/// Maximum simultaneous polyphonic voices (16 voices = 32 coupled free reeds).
pub const MAX_VOICES: usize = 16;

/// Structured MIDI/control event consumed by `SynthEngine`.
#[derive(Debug, Clone, Copy)]
pub enum MidiEvent {
    /// Trigger a note on with MIDI note (`0..=127`), blade velocity (`1..=127`), and manual hand.
    NoteOn {
        note: u8,
        velocity: u8,
        hand: Hand,
    },
    /// Release a note with release velocity (`0..=127`).
    NoteOff {
        note: u8,
        release_velocity: u8,
        hand: Hand,
    },
    /// Continuous MIDI CC#11 Expression (`0..=127`) from the Bandolibre bellows sensor.
    ControlChange11(u8),
    /// High-resolution continuous expression in `[0.0, 1.0]`.
    ExpressionNormalized(f32),
    /// Enable/disable Table Mode (practice without moving bellows).
    SetTableMode(bool),
    /// Select keyboard/channel routing mode (`DualChannelBandolibre` vs `PianistSplit`).
    SetKeyboardMode(KeyboardMode),
    /// Set split MIDI note (`36..=96`, default `60 = C4`) used in `KeyboardMode::PianistSplit`.
    SetSplitNote(u8),
    /// Set stereo separation width between Left (*Bajos*) and Right (*Cantabile*) manuals in `[0.0, 1.0]`.
    SetStereoWidth(f32),
    /// Immediately silence all voices.
    AllNotesOff,
}

/// Real-time bandoneon physical modeling synthesizer engine.
#[derive(Debug, Clone)]
pub struct SynthEngine {
    sample_rate: f32,
    preset: BandoneonPreset,
    bellows: BellowsModel,
    voices: [BandoneonVoice; MAX_VOICES],
    body: BodyResonator,
    last_total_flow: f32,
    event_counter: u64,
    cc11_msb: u8,
    keyboard_mode: KeyboardMode,
    split_note: u8,
    stereo_width: f32,
    post: PostProcessor,
}

impl SynthEngine {
    /// Creates and pre-allocates a new `SynthEngine` at `sample_rate` Hz.
    pub fn new(sample_rate: f32, preset: BandoneonPreset) -> Self {
        let sr = sample_rate.max(8000.0);
        let mut bellows = BellowsModel::new(sr);
        bellows.set_smoothing(
            preset.bellows_curve_gamma,
            preset.bellows_attack_ms,
            preset.bellows_release_ms,
        );
        let body = BodyResonator::new_from_preset(sr, &preset);
        let voices =
            core::array::from_fn(|i| BandoneonVoice::new((i as u32 + 1).wrapping_mul(0x9E37_79B9)));

        Self {
            sample_rate: sr,
            preset,
            bellows,
            voices,
            body,
            last_total_flow: 0.0,
            event_counter: 0,
            cc11_msb: 0,
            keyboard_mode: KeyboardMode::DualChannelBandolibre,
            split_note: 60,
            stereo_width: 0.84,
            post: PostProcessor::new(sr),
        }
    }

    /// Updates the active preset and refreshes any active voices.
    pub fn set_preset(&mut self, preset: BandoneonPreset) {
        self.apply_preset_ref(&preset);
        self.preset = preset;
    }

    /// Updates the active preset from a reference without allocating or deallocating heap memory.
    /// Real-time audio thread safe.
    pub fn apply_preset_ref(&mut self, preset: &BandoneonPreset) {
        self.bellows.set_smoothing(
            preset.bellows_curve_gamma,
            preset.bellows_attack_ms,
            preset.bellows_release_ms,
        );
        self.body.update_from_preset(preset);
        for v in &mut self.voices {
            v.refresh_preset(preset, self.sample_rate);
        }
        self.preset.copy_params_from(preset);
    }

    pub fn preset(&self) -> &BandoneonPreset {
        &self.preset
    }

    #[inline]
    pub fn version(&self) -> SynthVersion {
        self.preset.version
    }

    /// Switches the active synthesis model version and its calibrated preset.
    pub fn set_version(&mut self, version: SynthVersion) {
        self.set_preset(BandoneonPreset::for_version(version));
    }

    #[inline]
    pub fn keyboard_mode(&self) -> KeyboardMode {
        self.keyboard_mode
    }

    #[inline]
    pub fn set_keyboard_mode(&mut self, mode: KeyboardMode) {
        self.keyboard_mode = mode;
    }

    #[inline]
    pub fn split_note(&self) -> u8 {
        self.split_note
    }

    #[inline]
    pub fn set_split_note(&mut self, note: u8) {
        self.split_note = note.clamp(Hand::LEFT_MIN_MIDI, Hand::RIGHT_MAX_MIDI);
    }

    #[inline]
    pub fn stereo_width(&self) -> f32 {
        self.stereo_width
    }

    #[inline]
    pub fn set_stereo_width(&mut self, width: f32) {
        self.stereo_width = width.clamp(0.0, 1.0);
    }

    #[inline]
    pub fn post_enabled(&self) -> bool {
        self.post.enabled
    }

    #[inline]
    pub fn set_post_enabled(&mut self, enabled: bool) {
        self.post.enabled = enabled;
    }

    #[inline]
    pub fn set_post_params(
        &mut self,
        comp_amount: f32,
        reverb_mix: f32,
        reverb_size: f32,
        spatial_amount: f32,
    ) {
        self.post.comp_amount = comp_amount.clamp(0.0, 1.0);
        self.post.reverb_mix = reverb_mix.clamp(0.0, 1.0);
        self.post.reverb_size = reverb_size.clamp(0.0, 1.0);
        self.post.spatial_amount = spatial_amount.clamp(0.0, 1.0);
    }

    #[inline]
    pub fn post_gain_reduction_db(&self) -> f32 {
        self.post.last_gain_reduction_db()
    }

    pub fn reset(&mut self) {
        self.bellows = BellowsModel::new(self.sample_rate);
        self.bellows.set_smoothing(
            self.preset.bellows_curve_gamma,
            self.preset.bellows_attack_ms,
            self.preset.bellows_release_ms,
        );
        for v in &mut self.voices {
            v.active = false;
            v.key_down = false;
        }
        self.body.reset();
        self.post.reset();
        self.last_total_flow = 0.0;
    }

    /// Infers the default bandoneon manual (`Hand`) from MIDI note if not specified.
    #[inline]
    pub fn default_hand_for_note(midi_note: u8) -> Hand {
        if midi_note < 60 {
            Hand::LeftBajos
        } else {
            Hand::RightCantabile
        }
    }

    /// Resolves which bandoneon keyboard (`Hand`) a raw MIDI message on 0-based channel `ch`
    /// and MIDI pitch `note` maps to under the current `KeyboardMode`.
    #[inline]
    pub fn resolve_hand_for_channel(&self, ch: u8, note: u8) -> Hand {
        match self.keyboard_mode {
            KeyboardMode::DualChannelBandolibre => match ch {
                0 => Hand::LeftBajos,
                1 => Hand::RightCantabile,
                _ => {
                    if note < self.split_note {
                        Hand::LeftBajos
                    } else {
                        Hand::RightCantabile
                    }
                }
            },
            KeyboardMode::PianistSplit => {
                if note < self.split_note {
                    Hand::LeftBajos
                } else {
                    Hand::RightCantabile
                }
            }
        }
    }

    /// Processes a structured MIDI or control event (real-time safe).
    pub fn handle_midi_event(&mut self, event: MidiEvent) {
        self.event_counter = self.event_counter.wrapping_add(1);
        match event {
            MidiEvent::NoteOn {
                note,
                velocity,
                hand,
            } => {
                if velocity == 0 {
                    self.handle_midi_event(MidiEvent::NoteOff {
                        note,
                        release_velocity: 64,
                        hand,
                    });
                    return;
                }
                self.bellows.notify_note_on(velocity);
                let idx = self.allocate_voice(note, hand);
                self.voices[idx].trigger_note_on(
                    note,
                    velocity,
                    hand,
                    self.sample_rate,
                    &self.preset,
                    self.event_counter,
                );
            }
            MidiEvent::NoteOff {
                note,
                release_velocity,
                hand,
            } => {
                for v in &mut self.voices {
                    if v.active && v.key_down && v.midi_note == note && v.hand == hand {
                        v.trigger_note_off(release_velocity, &self.preset);
                    }
                }
                if !self.voices.iter().any(|v| v.active && v.key_down) {
                    self.bellows.notify_all_notes_off();
                }
            }
            MidiEvent::ControlChange11(cc11) => {
                self.bellows.set_cc11(cc11);
            }
            MidiEvent::ExpressionNormalized(expr) => {
                self.bellows.set_expression_norm(expr);
            }
            MidiEvent::SetTableMode(enabled) => {
                self.bellows.table_mode = enabled;
            }
            MidiEvent::SetKeyboardMode(mode) => {
                self.keyboard_mode = mode;
            }
            MidiEvent::SetSplitNote(note) => {
                self.set_split_note(note);
            }
            MidiEvent::SetStereoWidth(width) => {
                self.set_stereo_width(width);
            }
            MidiEvent::AllNotesOff => {
                for v in &mut self.voices {
                    v.active = false;
                    v.key_down = false;
                }
                self.bellows.notify_all_notes_off();
                self.body.reset();
                self.post.reset();
            }
        }
    }

    /// Parses raw 3-byte MIDI messages matching Bandolibre firmware conventions:
    /// - In `KeyboardMode::DualChannelBandolibre` (default):
    ///   - `L_MIDI_CH = 0` (MIDI Ch 1) = Left Wing (`Hand::LeftBajos`: warm bass box + left pan)
    ///   - `R_MIDI_CH = 1` (MIDI Ch 2) = Right Wing (`Hand::RightCantabile`: bright treble grill + right pan)
    ///   - Overlapping notes (`C4..B4`, MIDI 60..71) are routed strictly by channel so both manuals
    ///     can play the same pitch with their distinct tone and stereo position.
    /// - In `KeyboardMode::PianistSplit`:
    ///   - Notes `< split_note` (default `C4 = 60`) route to `Hand::LeftBajos` and `>= split_note`
    ///     route to `Hand::RightCantabile` regardless of MIDI channel.
    /// - Supports both 7-bit `CC#11` and 14-bit `CC#11` (MSB) + `CC#43` (LSB) expression pairs.
    pub fn handle_raw_midi(&mut self, status: u8, data1: u8, data2: u8) {
        let cmd = status & 0xF0;
        let ch = status & 0x0F;
        let hand = self.resolve_hand_for_channel(ch, data1);
        match cmd {
            0x90 => self.handle_midi_event(MidiEvent::NoteOn {
                note: data1,
                velocity: data2,
                hand,
            }),
            0x80 => self.handle_midi_event(MidiEvent::NoteOff {
                note: data1,
                release_velocity: data2,
                hand,
            }),
            0xB0 if data1 == 11 => {
                self.cc11_msb = data2.min(127);
                self.handle_midi_event(MidiEvent::ControlChange11(self.cc11_msb));
            }
            0xB0 if data1 == 43 => {
                let lsb = data2.min(127) as u16;
                let msb = self.cc11_msb as u16;
                let val14 = ((msb << 7) | lsb) as f32 / 16383.0;
                self.handle_midi_event(MidiEvent::ExpressionNormalized(val14));
            }
            0xB0 if data1 == 120 || data1 == 123 => {
                self.handle_midi_event(MidiEvent::AllNotesOff)
            }
            _ => {}
        }
    }

    fn allocate_voice(&self, note: u8, hand: Hand) -> usize {
        // 1. Retrigger existing voice on the same note + hand
        for (i, v) in self.voices.iter().enumerate() {
            if v.active && v.midi_note == note && v.hand == hand {
                return i;
            }
        }
        // 2. Use first inactive voice
        for (i, v) in self.voices.iter().enumerate() {
            if !v.active {
                return i;
            }
        }
        // 3. Steal oldest released voice
        if let Some((i, _)) = self
            .voices
            .iter()
            .enumerate()
            .filter(|(_, v)| !v.key_down)
            .min_by_key(|(_, v)| v.age)
        {
            return i;
        }
        // 4. Steal oldest active voice
        self.voices
            .iter()
            .enumerate()
            .min_by_key(|(_, v)| v.age)
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    #[inline]
    fn step_manuals_and_body(&mut self, dt: f32, inv_sr_38: f32, inv_sr_42: f32) -> (f32, f32) {
        let p_bellows = self
            .bellows
            .tick(self.last_total_flow, self.preset.chamber_coupling);

        let mut sum_l = 0.0f32;
        let mut sum_r = 0.0f32;
        let mut flow_sum = 0.0f32;

        for v in &mut self.voices {
            if v.active {
                let (sig, flow) = v.tick(
                    self.sample_rate,
                    dt,
                    inv_sr_38,
                    inv_sr_42,
                    p_bellows,
                    &self.preset,
                );
                flow_sum += flow;
                match v.hand {
                    Hand::LeftBajos => sum_l += sig,
                    Hand::RightCantabile => sum_r += sig,
                }
            }
        }

        self.last_total_flow = flow_sum;
        self.body.tick_stereo(sum_l, sum_r)
    }

    #[inline]
    fn soft_limit(x: f32) -> f32 {
        x / (1.0 + x * x).sqrt()
    }

    /// Renders a block of stereo audio in real time (zero allocations).
    /// Left-hand (*Bajos*, Channel 1) voices pass through `left_body_modes` and pan to Left;
    /// Right-hand (*Cantabile*, Channel 2) voices pass through `body_modes` and pan to Right.
    pub fn render(&mut self, out_left: &mut [f32], out_right: &mut [f32]) {
        let frames = out_left.len().min(out_right.len());
        let gain = self.preset.master_gain;
        let w = self.stereo_width.clamp(0.0, 1.0);
        let pan_main = 0.5 + 0.5 * w;
        let pan_cross = 0.5 - 0.5 * w;
        let dt = 1.0 / self.sample_rate;
        let inv_sr_38 = dt * (1.0 / 0.38);
        let inv_sr_42 = dt * (6.0 / 0.42);
        for i in 0..frames {
            let (body_l, body_r) = self.step_manuals_and_body(dt, inv_sr_38, inv_sr_42);
            let stereo_l = (pan_main * body_l + pan_cross * body_r) * gain;
            let stereo_r = (pan_cross * body_l + pan_main * body_r) * gain;
            let (post_l, post_r) = self.post.tick_stereo(stereo_l, stereo_r);
            out_left[i] = Self::soft_limit(post_l);
            out_right[i] = Self::soft_limit(post_r);
        }
    }

    /// Renders a mono signal mix (routing LeftBajos through the Left box resonator and
    /// RightCantabile through the Right box resonator).
    pub fn render_mono(&mut self, out_mono: &mut [f32]) {
        let gain = self.preset.master_gain;
        let dt = 1.0 / self.sample_rate;
        let inv_sr_38 = dt * (1.0 / 0.38);
        let inv_sr_42 = dt * (6.0 / 0.42);
        for sample in out_mono.iter_mut() {
            let (body_l, body_r) = self.step_manuals_and_body(dt, inv_sr_38, inv_sr_42);
            *sample = Self::soft_limit((body_l + body_r) * gain);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fast_sin_cos_accuracy_and_seam_continuity() {
        let n = 4096;
        let mut prev_s = 0.0f32;
        let mut prev_c = 1.0f32;
        for i in 0..=n {
            let phase = (i as f32) / (n as f32);
            let (s, c) = crate::reed::fast_sin_cos(phase);
            let (s_ref, c_ref) = (phase * core::f32::consts::TAU).sin_cos();
            assert!(
                (s - s_ref).abs() < 2.0e-6 && (c - c_ref).abs() < 2.0e-6,
                "fast_sin_cos error exceeded 2e-6 at phase={phase}: err_s={}, err_c={}",
                (s - s_ref).abs(),
                (c - c_ref).abs()
            );
            if i > 0 {
                let max_step = core::f32::consts::TAU / (n as f32) + 5e-6;
                assert!(
                    (s - prev_s).abs() <= max_step && (c - prev_c).abs() <= max_step,
                    "Seam discontinuity at phase={phase}"
                );
            }
            prev_s = s;
            prev_c = c;
        }
    }

    #[test]
    fn test_stability_and_silence_convergence() {
        let preset = BandoneonPreset::default_bandoneon();
        let mut synth = SynthEngine::new(48000.0, preset);

        // Trigger 16 voices at max velocity and jump CC#11 0 -> 127 -> 0
        for i in 0..16u8 {
            let hand = if i < 8 {
                Hand::LeftBajos
            } else {
                Hand::RightCantabile
            };
            synth.handle_midi_event(MidiEvent::NoteOn {
                note: 40 + i * 3,
                velocity: 127,
                hand,
            });
        }
        synth.handle_midi_event(MidiEvent::ControlChange11(127));

        let mut buf_l = [0.0f32; 4800];
        let mut buf_r = [0.0f32; 4800];
        synth.render(&mut buf_l, &mut buf_r);

        for (&l, &r) in buf_l.iter().zip(buf_r.iter()) {
            assert!(l.is_finite() && r.is_finite(), "NaN or Inf detected");
            assert!(l.abs() <= 1.0 && r.abs() <= 1.0, "Sample exceeded [-1, 1]");
        }
        let active_rms: f32 =
            (buf_l.iter().map(|x| x * x).sum::<f32>() / buf_l.len() as f32).sqrt();
        assert!(active_rms > 0.01, "Expected audible output when CC11=127");

        // Now drop CC#11 to 0 and release all notes; verify convergence to < -90 dBFS within 0.5s
        synth.handle_midi_event(MidiEvent::ControlChange11(0));
        for i in 0..16u8 {
            let hand = if i < 8 {
                Hand::LeftBajos
            } else {
                Hand::RightCantabile
            };
            synth.handle_midi_event(MidiEvent::NoteOff {
                note: 40 + i * 3,
                release_velocity: 64,
                hand,
            });
        }

        for _ in 0..5 {
            synth.render(&mut buf_l, &mut buf_r);
        }
        let tail_rms: f32 =
            (buf_l.iter().map(|x| x * x).sum::<f32>() / buf_l.len() as f32).sqrt();
        assert!(
            tail_rms < 3.16e-5,
            "Did not converge to <-90 dBFS (tail_rms = {tail_rms})"
        );
    }

    #[test]
    fn test_all_twelve_versions_and_live_version_switching() {
        let presets: [BandoneonPreset; SynthVersion::COUNT] =
            core::array::from_fn(|i| BandoneonPreset::for_version(SynthVersion::from_u8(i as u8)));

        for (idx, ver) in SynthVersion::ALL.iter().copied().enumerate() {
            assert_eq!(ver.as_u8() as usize, idx);
            assert_eq!(SynthVersion::from_u8(ver.as_u8()), ver);
            assert_eq!(SynthVersion::from_slug(ver.slug()), Some(ver));
            assert!(
                presets[idx].roughness_penalty() < 1e-6,
                "Expected zero roughness penalty on {}",
                ver.slug()
            );

            let mut synth = SynthEngine::new(48000.0, presets[idx].clone());
            assert_eq!(synth.version(), ver);

            // Extreme compass boundary notes (MIDI 36 LeftBajos, MIDI 96 RightCantabile)
            synth.handle_midi_event(MidiEvent::NoteOn {
                note: 36,
                velocity: 127,
                hand: Hand::LeftBajos,
            });
            synth.handle_midi_event(MidiEvent::NoteOn {
                note: 96,
                velocity: 127,
                hand: Hand::RightCantabile,
            });
            synth.handle_midi_event(MidiEvent::ControlChange11(127));

            let mut buf_l = [0.0f32; 2400];
            let mut buf_r = [0.0f32; 2400];
            synth.render(&mut buf_l, &mut buf_r);

            for (&l, &r) in buf_l.iter().zip(buf_r.iter()) {
                assert!(
                    l.is_finite() && r.is_finite(),
                    "Non-finite sample on {}",
                    ver.slug()
                );
            }
            let rms = (buf_l.iter().map(|x| x * x).sum::<f32>() / buf_l.len() as f32).sqrt();
            assert!(rms > 0.005, "Silent output on {}", ver.slug());

            // Live mid-stream version switching across all 12 versions while extreme notes are held
            for next_preset in &presets {
                synth.apply_preset_ref(next_preset);
                assert_eq!(synth.version(), next_preset.version);
                let mut q_l = [0.0f32; 128];
                let mut q_r = [0.0f32; 128];
                synth.render(&mut q_l, &mut q_r);
                for (&l, &r) in q_l.iter().zip(q_r.iter()) {
                    assert!(l.is_finite() && r.is_finite());
                    assert!(l.abs() <= 1.0 && r.abs() <= 1.0);
                }
            }

            // Verify sample-to-sample continuity (zero click/step discontinuities) on mid-compass
            // A2 (110 Hz LeftBajos) + A4 (440 Hz RightCantabile) during rapid quantum switching
            synth.reset();
            synth.handle_midi_event(MidiEvent::NoteOn {
                note: 45,
                velocity: 100,
                hand: Hand::LeftBajos,
            });
            synth.handle_midi_event(MidiEvent::NoteOn {
                note: 69,
                velocity: 100,
                hand: Hand::RightCantabile,
            });
            synth.handle_midi_event(MidiEvent::ControlChange11(100));
            synth.render(&mut buf_l, &mut buf_r);

            let mut prev_l = *buf_l.last().unwrap();
            let mut prev_r = *buf_r.last().unwrap();
            for (step_i, &ver_idx) in [0usize, 11, 5, 6, 7, 8, 9, 10, 1, 2, 3, 4, 11]
                .iter()
                .enumerate()
            {
                let next_preset = &presets[ver_idx];
                synth.apply_preset_ref(next_preset);
                assert_eq!(synth.version(), next_preset.version);
                let cc11 = (60 + (step_i * 5) % 60) as u8;
                synth.handle_midi_event(MidiEvent::ControlChange11(cc11));
                let mut q_l = [0.0f32; 128];
                let mut q_r = [0.0f32; 128];
                synth.render(&mut q_l, &mut q_r);
                for (&l, &r) in q_l.iter().zip(q_r.iter()) {
                    assert!(l.is_finite() && r.is_finite());
                    assert!(
                        (l - prev_l).abs() < 0.32 && (r - prev_r).abs() < 0.32,
                        "Discontinuity detected on live version switch to {}: dl={}, dr={}",
                        next_preset.version.slug(),
                        (l - prev_l).abs(),
                        (r - prev_r).abs()
                    );
                    prev_l = l;
                    prev_r = r;
                }
            }
        }
    }

    #[test]
    fn test_dual_keyboard_overlap_channel_tone_and_panning() {
        let preset = BandoneonPreset::default_bandoneon();
        let mut synth = SynthEngine::new(48000.0, preset);
        assert_eq!(synth.keyboard_mode(), KeyboardMode::DualChannelBandolibre);

        // MIDI 69 (A4 = 440 Hz) is in the 12-semitone overlap octave (C4..B4, MIDI 60..71)
        // present on both Bandolibre keyboards.
        assert!(Hand::is_overlap_note(69));

        // 1. Trigger A4 (MIDI 69) on MIDI Channel 1 (0x90 -> LeftBajos)
        synth.handle_raw_midi(0xB0, 11, 100);
        synth.handle_raw_midi(0x90, 69, 100);
        let mut ch1_l = [0.0f32; 4800];
        let mut ch1_r = [0.0f32; 4800];
        synth.render(&mut ch1_l, &mut ch1_r);
        let rms_ch1_l = (ch1_l[2400..].iter().map(|x| x * x).sum::<f32>() / 2400.0).sqrt();
        let rms_ch1_r = (ch1_r[2400..].iter().map(|x| x * x).sum::<f32>() / 2400.0).sqrt();
        assert!(
            rms_ch1_l > 5.0 * rms_ch1_r,
            "Expected Ch 1 (LeftBajos) A4 to pan predominantly Left: L={rms_ch1_l}, R={rms_ch1_r}"
        );

        // 2. Trigger the exact same pitch A4 (MIDI 69) on MIDI Channel 2 (0x91 -> RightCantabile)
        synth.reset();
        synth.handle_raw_midi(0xB0, 11, 100);
        synth.handle_raw_midi(0x91, 69, 100);
        let mut ch2_l = [0.0f32; 4800];
        let mut ch2_r = [0.0f32; 4800];
        synth.render(&mut ch2_l, &mut ch2_r);
        let rms_ch2_l = (ch2_l[2400..].iter().map(|x| x * x).sum::<f32>() / 2400.0).sqrt();
        let rms_ch2_r = (ch2_r[2400..].iter().map(|x| x * x).sum::<f32>() / 2400.0).sqrt();
        assert!(
            rms_ch2_r > 5.0 * rms_ch2_l,
            "Expected Ch 2 (RightCantabile) A4 to pan predominantly Right: L={rms_ch2_l}, R={rms_ch2_r}"
        );

        // Verify distinct tone/spectral brightness (zero-crossing rate / high-frequency derivative energy)
        // between Ch 1 (warm Bajos wooden box) and Ch 2 (bright Cantabile metal grill) on the same note 69
        let hf_ratio_ch1 = (ch1_l[2401..]
            .windows(2)
            .map(|w| (w[1] - w[0]).powi(2))
            .sum::<f32>()
            / 2399.0)
            .sqrt()
            / rms_ch1_l.max(1e-6);
        let hf_ratio_ch2 = (ch2_r[2401..]
            .windows(2)
            .map(|w| (w[1] - w[0]).powi(2))
            .sum::<f32>()
            / 2399.0)
            .sqrt()
            / rms_ch2_r.max(1e-6);
        assert!(
            (hf_ratio_ch2 - hf_ratio_ch1).abs() > 0.01,
            "Expected distinct tone between LeftBajos and RightCantabile on overlap note 69: hf1={hf_ratio_ch1}, hf2={hf_ratio_ch2}"
        );

        // 3. Trigger A4 (MIDI 69) on BOTH Channel 1 and Channel 2 simultaneously (unison across manuals)
        synth.reset();
        synth.handle_raw_midi(0xB0, 11, 100);
        synth.handle_raw_midi(0x90, 69, 100);
        synth.handle_raw_midi(0x91, 69, 100);
        let active_count = synth.voices.iter().filter(|v| v.active && v.key_down).count();
        assert_eq!(
            active_count, 2,
            "Expected 2 simultaneous voices when overlap note 69 is held on both Ch 1 and Ch 2"
        );

        // Releasing Ch 1 (0x80) leaves Ch 2 (RightCantabile) still held
        synth.handle_raw_midi(0x80, 69, 64);
        let held_hands: Vec<Hand> = synth
            .voices
            .iter()
            .filter(|v| v.active && v.key_down)
            .map(|v| v.hand)
            .collect();
        assert_eq!(held_hands, vec![Hand::RightCantabile]);

        // 4. Verify PianistSplit mode routes a single-channel MIDI piano controller by split_note
        synth.reset();
        synth.set_keyboard_mode(KeyboardMode::PianistSplit);
        synth.set_split_note(60); // C4 split
        synth.handle_raw_midi(0x90, 48, 100); // C3 on Ch 1 -> LeftBajos (< 60)
        synth.handle_raw_midi(0x90, 64, 100); // E4 on Ch 1 -> RightCantabile (>= 60)
        let has_left = synth
            .voices
            .iter()
            .any(|v| v.active && v.midi_note == 48 && v.hand == Hand::LeftBajos);
        let has_right = synth
            .voices
            .iter()
            .any(|v| v.active && v.midi_note == 64 && v.hand == Hand::RightCantabile);
        assert!(has_left && has_right);
    }

    #[test]
    fn test_voice_stealing_and_multi_sample_rate_invariance() {
        let preset = BandoneonPreset::v4_unified();
        let mut synth = SynthEngine::new(48000.0, preset.clone());
        synth.handle_midi_event(MidiEvent::ControlChange11(110));

        // 1. Trigger 16 distinct notes and render until all 16 pallet valves are fully open
        for i in 0..16u8 {
            let hand = if i < 8 {
                Hand::LeftBajos
            } else {
                Hand::RightCantabile
            };
            synth.handle_midi_event(MidiEvent::NoteOn {
                note: 38 + i * 2,
                velocity: 100,
                hand,
            });
        }
        let mut buf_l = [0.0f32; 2400];
        let mut buf_r = [0.0f32; 2400];
        synth.render(&mut buf_l, &mut buf_r);
        for v in &synth.voices {
            assert!(v.active && v.valve_aperture > 0.95);
        }

        // 2. Retrigger an existing note (same button: note 38, LeftBajos) -> warm valve_aperture preserved
        synth.handle_midi_event(MidiEvent::NoteOn {
            note: 38,
            velocity: 110,
            hand: Hand::LeftBajos,
        });
        let same_v = synth
            .voices
            .iter()
            .find(|v| v.midi_note == 38 && v.hand == Hand::LeftBajos)
            .unwrap();
        assert!(
            same_v.valve_aperture > 0.95,
            "Expected same-button retrigger to preserve warm valve_aperture, got {}",
            same_v.valve_aperture
        );

        // 3. Trigger a 17th distinct note (note 92, RightCantabile) -> steals oldest voice (note 40)
        //    and MUST reset valve_aperture to 0.0 so the new button starts closed
        synth.handle_midi_event(MidiEvent::NoteOn {
            note: 92,
            velocity: 120,
            hand: Hand::RightCantabile,
        });
        let stolen_v = synth
            .voices
            .iter()
            .find(|v| v.midi_note == 92 && v.hand == Hand::RightCantabile)
            .unwrap();
        assert!(
            stolen_v.valve_aperture == 0.0,
            "Expected stolen voice for a different button to reset valve_aperture to 0.0, got {}",
            stolen_v.valve_aperture
        );

        // 4. Verify sample-rate invariance and stability across 44.1 kHz, 48 kHz, and 96 kHz
        for &sr in &[44100.0f32, 48000.0, 96000.0] {
            let mut sr_synth = SynthEngine::new(sr, preset.clone());
            sr_synth.handle_midi_event(MidiEvent::NoteOn {
                note: 45,
                velocity: 127,
                hand: Hand::LeftBajos,
            });
            sr_synth.handle_midi_event(MidiEvent::NoteOn {
                note: 81,
                velocity: 127,
                hand: Hand::RightCantabile,
            });
            for step in 0..8 {
                let cc11 = if step % 2 == 0 { 127 } else { 0 };
                sr_synth.handle_midi_event(MidiEvent::ControlChange11(cc11));
                let mut q_l = [0.0f32; 256];
                let mut q_r = [0.0f32; 256];
                sr_synth.render(&mut q_l, &mut q_r);
                for (&l, &r) in q_l.iter().zip(q_r.iter()) {
                    assert!(
                        l.is_finite() && r.is_finite() && l.abs() <= 1.0 && r.abs() <= 1.0,
                        "Invalid sample at sr={sr}: l={l}, r={r}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_post_processor_spatial_compressor_reverb_stability() {
        let mut synth = SynthEngine::new(
            48000.0,
            BandoneonPreset::for_version(SynthVersion::V4Unified),
        );
        synth.set_post_enabled(true);
        synth.set_post_params(0.65, 0.35, 0.60, 0.70);
        synth.handle_midi_event(MidiEvent::ControlChange11(120));
        synth.handle_midi_event(MidiEvent::NoteOn {
            note: 48,
            velocity: 118,
            hand: Hand::LeftBajos,
        });
        synth.handle_midi_event(MidiEvent::NoteOn {
            note: 69,
            velocity: 118,
            hand: Hand::RightCantabile,
        });

        let mut left = [0.0f32; 128];
        let mut right = [0.0f32; 128];
        let mut max_gr_db = 0.0f32;
        for _ in 0..80 {
            synth.render(&mut left, &mut right);
            assert!(left.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
            assert!(right.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
            max_gr_db = max_gr_db.max(synth.post_gain_reduction_db());
        }
        assert!(
            max_gr_db > 0.1,
            "Expected positive compressor gain reduction under forte chord, got {max_gr_db}"
        );

        // Release notes and verify reverb tail exists then decays cleanly
        synth.handle_midi_event(MidiEvent::NoteOff {
            note: 48,
            release_velocity: 64,
            hand: Hand::LeftBajos,
        });
        synth.handle_midi_event(MidiEvent::NoteOff {
            note: 69,
            release_velocity: 64,
            hand: Hand::RightCantabile,
        });
        synth.handle_midi_event(MidiEvent::ControlChange11(0));
        for _ in 0..40 {
            synth.render(&mut left, &mut right);
        }
        let reverb_tail_rms = (left.iter().map(|x| x * x).sum::<f32>() / 128.0).sqrt();
        assert!(
            reverb_tail_rms > 1e-4,
            "Expected audible salon reverb tail shortly after release, got {reverb_tail_rms}"
        );

        // AllNotesOff clears voices and reverb tank
        synth.handle_midi_event(MidiEvent::AllNotesOff);
        synth.render(&mut left, &mut right);
        let cleared_rms = (left.iter().map(|x| x * x).sum::<f32>() / 128.0).sqrt();
        assert!(cleared_rms < 1e-5, "Expected silence after AllNotesOff, got {cleared_rms}");
    }
}
