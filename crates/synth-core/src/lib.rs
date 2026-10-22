//! `synth-core`: Platform-agnostic real-time physical modeling bandoneon DSP engine.
//!
//! Designed for the **Bandolibre** open-source MIDI bandoneon controller:
//! - Zero heap allocations, zero locks, and zero I/O inside the audio render loop.
//! - Coupled 8' + 4' blown-closed free-reed oscillators with unsteady Bernoulli orifice flow.
//! - Separate Left-Hand (*Bajos*) and Right-Hand (*Cantabile*) physical scaling laws.
//! - 6-mode wooden *cabezal* and bellows enclosure resonator bank.

pub mod bellows;
pub mod body;
pub mod preset;
pub mod reed;
pub mod voice;

pub use bellows::BellowsModel;
pub use body::BodyResonator;
pub use preset::{BandoneonPreset, BodyModeParam, Hand, ManualScalingParams, ResolvedNoteParams};
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
        let body = BodyResonator::new(sr, &preset.left_body_modes, &preset.body_modes);
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
        }
    }

    /// Updates the active preset without allocating memory.
    pub fn set_preset(&mut self, preset: BandoneonPreset) {
        self.bellows.set_smoothing(
            preset.bellows_curve_gamma,
            preset.bellows_attack_ms,
            preset.bellows_release_ms,
        );
        self.body
            .update_modes(&preset.left_body_modes, &preset.body_modes);
        self.preset = preset;
    }

    pub fn preset(&self) -> &BandoneonPreset {
        &self.preset
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
        self.last_total_flow = 0.0;
    }

    /// Infers the default bandoneon manual (`Hand`) from MIDI note or MIDI channel if not specified.
    #[inline]
    pub fn default_hand_for_note(midi_note: u8) -> Hand {
        if midi_note < 60 {
            Hand::LeftBajos
        } else {
            Hand::RightCantabile
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
            MidiEvent::AllNotesOff => {
                for v in &mut self.voices {
                    v.active = false;
                    v.key_down = false;
                }
                self.bellows.notify_all_notes_off();
            }
        }
    }

    /// Parses raw 3-byte MIDI messages matching Bandolibre firmware conventions:
    /// - `L_MIDI_CH = 0` (MIDI Ch 1) = Left Wing (`Hand::LeftBajos`)
    /// - `R_MIDI_CH = 1` (MIDI Ch 2) = Right Wing (`Hand::RightCantabile`)
    /// - Supports both 7-bit `CC#11` and 14-bit `CC#11` (MSB) + `CC#43` (LSB) expression pairs.
    pub fn handle_raw_midi(&mut self, status: u8, data1: u8, data2: u8) {
        let cmd = status & 0xF0;
        let ch = status & 0x0F;
        let hand = if ch == 0 {
            Hand::LeftBajos
        } else if ch == 1 {
            Hand::RightCantabile
        } else if data1 < 60 {
            Hand::LeftBajos
        } else {
            Hand::RightCantabile
        };
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
    fn step_manuals_and_body(&mut self) -> (f32, f32) {
        let p_bellows = self
            .bellows
            .tick(self.last_total_flow, self.preset.chamber_coupling);

        let mut sum_l = 0.0f32;
        let mut sum_r = 0.0f32;
        let mut flow_sum = 0.0f32;

        for v in &mut self.voices {
            if v.active {
                let (sig, flow) = v.tick(self.sample_rate, p_bellows, &self.preset);
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
    pub fn render(&mut self, out_left: &mut [f32], out_right: &mut [f32]) {
        let frames = out_left.len().min(out_right.len());
        let gain = self.preset.master_gain;
        for i in 0..frames {
            let (body_l, body_r) = self.step_manuals_and_body();
            let stereo_l = (0.78 * body_l + 0.22 * body_r) * gain;
            let stereo_r = (0.22 * body_l + 0.78 * body_r) * gain;
            out_left[i] = Self::soft_limit(stereo_l);
            out_right[i] = Self::soft_limit(stereo_r);
        }
    }

    /// Renders a mono signal mix (routing LeftBajos through the Left box resonator and
    /// RightCantabile through the Right box resonator).
    pub fn render_mono(&mut self, out_mono: &mut [f32]) {
        let gain = self.preset.master_gain;
        for sample in out_mono.iter_mut() {
            let (body_l, body_r) = self.step_manuals_and_body();
            *sample = Self::soft_limit((body_l + body_r) * gain);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
