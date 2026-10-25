/// Real-time bellows reservoir pressure model driven by Bandolibre `CC#11` and `Note On` velocity.
#[derive(Debug, Clone)]
pub struct BellowsModel {
    sample_rate: f32,
    /// Latest MIDI CC#11 normalized in `[0.0, 1.0]`.
    target_cc11: f32,
    /// Precomputed `target_cc11.powf(curve_gamma)` so `tick()` is branchless and `powf`-free.
    target_pressure: f32,
    /// Instantaneous smoothed bellows reservoir pressure in `[0.0, 1.5]`.
    current_pressure: f32,
    /// Transient pressure boost injected by sharp `Note On` blade velocity.
    velocity_impulse: f32,
    /// Power-law exponent `gamma` mapping CC#11 to physical pressure.
    curve_gamma: f32,
    /// One-pole attack coefficient.
    attack_coeff: f32,
    /// One-pole release coefficient.
    release_coeff: f32,
    /// Impulse decay coefficient.
    impulse_decay_coeff: f32,
    /// Natural sustain pulsation depth on held notes (`0.0` in `v1..v4`, active in `v5`).
    pulsation_depth: f32,
    /// Number of consecutive audio samples with stationary CC#11 / expression input.
    steady_control_samples: u32,
    /// Envelope tracking how long bellows pressure has been held steady (`[0.0, 1.0]`).
    steady_hold_env: f32,
    /// Primary physiological *fueye* sustain pulsation phase `[0.0, 1.0)` (~1.55 Hz).
    pulse_phase_1: f32,
    /// Secondary acoustic shimmer phase `[0.0, 1.0)` (~3.40 Hz).
    pulse_phase_2: f32,
    pulse_inc_1: f32,
    pulse_inc_2: f32,
    hold_rise_inc: f32,
}

impl BellowsModel {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        let mut model = Self {
            sample_rate: sr,
            target_cc11: 0.0,
            target_pressure: 0.0,
            current_pressure: 0.0,
            velocity_impulse: 0.0,
            curve_gamma: 1.25,
            attack_coeff: 0.01,
            release_coeff: 0.005,
            impulse_decay_coeff: 0.999,
            pulsation_depth: 0.0,
            steady_control_samples: 0,
            steady_hold_env: 0.0,
            pulse_phase_1: 0.0,
            pulse_phase_2: 0.25,
            pulse_inc_1: 1.55 / sr,
            pulse_inc_2: 3.40 / sr,
            hold_rise_inc: 1.0 / (0.18 * sr),
        };
        model.set_smoothing(1.25, 3.5, 8.0);
        model
    }

    #[inline]
    pub fn set_pulsation_depth(&mut self, depth: f32) {
        self.pulsation_depth = depth.clamp(0.0, 0.25);
    }

    pub fn set_smoothing(&mut self, gamma: f32, attack_ms: f32, release_ms: f32) {
        self.curve_gamma = gamma.clamp(0.5, 3.0);
        self.target_pressure = if self.target_cc11 <= 0.0 {
            0.0
        } else {
            self.target_cc11.powf(self.curve_gamma)
        };
        self.attack_coeff = Self::ms_to_coeff(self.sample_rate, attack_ms.max(0.5));
        self.release_coeff = Self::ms_to_coeff(self.sample_rate, release_ms.max(0.5));
        self.impulse_decay_coeff = 1.0 - Self::ms_to_coeff(self.sample_rate, 18.0);
    }

    #[inline]
    fn ms_to_coeff(sample_rate: f32, time_ms: f32) -> f32 {
        let tau_samples = (time_ms * 0.001 * sample_rate).max(1.0);
        (1.0 - (-1.0 / tau_samples).exp()).clamp(0.0001, 1.0)
    }

    /// Sets the MIDI CC#11 expression value (`0..=127`).
    #[inline]
    pub fn set_cc11(&mut self, cc11: u8) {
        let norm = (cc11.min(127) as f32) / 127.0;
        if (norm - self.target_cc11).abs() > 1e-6 {
            self.steady_control_samples = 0;
            self.steady_hold_env = 0.0;
        }
        self.target_cc11 = norm;
        self.target_pressure = if norm <= 0.0 {
            0.0
        } else {
            norm.powf(self.curve_gamma)
        };
    }

    /// Sets continuous high-resolution normalized expression in `[0.0, 1.0]`.
    #[inline]
    pub fn set_expression_norm(&mut self, expr: f32) {
        let norm = expr.clamp(0.0, 1.0);
        if (norm - self.target_cc11).abs() > 1e-6 {
            self.steady_control_samples = 0;
            self.steady_hold_env = 0.0;
        }
        self.target_cc11 = norm;
        self.target_pressure = if norm <= 0.0 {
            0.0
        } else {
            norm.powf(self.curve_gamma)
        };
    }

    /// Called on `Note On` with the Bandolibre blade velocity (`1..=127`), up to `v4`: a sharp
    /// attack adds a short boost to the shared pressure. `v5` does not call it.
    #[inline]
    pub fn notify_note_on(&mut self, velocity: u8) {
        let vel_norm = (velocity.min(127) as f32) / 127.0;
        // Inject a bounded aerodynamic transient boost only on sharp high-velocity attacks
        let boost = 0.06 * vel_norm.powi(3);
        if boost > self.velocity_impulse {
            self.velocity_impulse = boost;
        }
    }

    /// Advances the bellows state by one audio sample and returns instantaneous reservoir pressure.
    #[inline(always)]
    pub fn tick(&mut self, total_volume_flow: f32, chamber_coupling: f32) -> f32 {
        let delta = self.target_pressure - self.current_pressure;
        let coeff = if delta > 0.0 {
            self.attack_coeff
        } else {
            self.release_coeff
        };

        self.current_pressure += coeff * delta;
        if self.current_pressure.abs() < 1e-12 {
            self.current_pressure = 0.0;
        }

        self.velocity_impulse *= self.impulse_decay_coeff;
        if self.velocity_impulse < 1e-12 {
            self.velocity_impulse = 0.0;
        }

        let pulse_scale = if self.pulsation_depth > 0.0 {
            self.steady_control_samples = self.steady_control_samples.saturating_add(1);
            if self.steady_control_samples > 4800 && self.velocity_impulse < 5.0e-4 {
                self.steady_hold_env = (self.steady_hold_env + self.hold_rise_inc).min(1.0);
            } else {
                self.steady_hold_env *= 0.995;
            }
            self.pulse_phase_1 += self.pulse_inc_1;
            if self.pulse_phase_1 >= 1.0 {
                self.pulse_phase_1 -= 1.0;
            }
            self.pulse_phase_2 += self.pulse_inc_2;
            if self.pulse_phase_2 >= 1.0 {
                self.pulse_phase_2 -= 1.0;
            }
            let c1 = 2.0 * self.pulse_phase_1 - 1.0;
            let s1 = 4.0 * c1 * (1.0 - c1.abs());
            let c2 = 2.0 * self.pulse_phase_2 - 1.0;
            let s2 = 4.0 * c2 * (1.0 - c2.abs());
            let wave = 0.72 * s1 + 0.28 * s2; // zero-mean physiological fueye respiration in [-1, 1]
            1.0 + self.pulsation_depth * self.steady_hold_env * wave
        } else {
            1.0
        };

        // Shared manifold pressure drop proportional to total airflow drawn by active reeds
        let drop = (chamber_coupling * 0.02 * total_volume_flow.abs()).min(0.25);
        ((self.current_pressure * pulse_scale + self.velocity_impulse) * (1.0 - drop))
            .clamp(0.0, 1.5)
    }

    #[inline]
    pub fn current_pressure(&self) -> f32 {
        self.current_pressure
    }
}
