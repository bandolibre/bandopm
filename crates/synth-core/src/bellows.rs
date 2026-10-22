/// Real-time bellows reservoir pressure model driven by Bandolibre `CC#11` and `Note On` velocity.
#[derive(Debug, Clone)]
pub struct BellowsModel {
    sample_rate: f32,
    /// Latest MIDI CC#11 normalized in `[0.0, 1.0]`.
    target_cc11: f32,
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
    /// Whether Table Mode is active (auto-generates bellows pressure from Note On velocity when CC#11 is 0).
    pub table_mode: bool,
    /// Fallback pressure derived from active note velocity in Table Mode.
    table_mode_pressure: f32,
}

impl BellowsModel {
    pub fn new(sample_rate: f32) -> Self {
        let mut model = Self {
            sample_rate: sample_rate.max(8000.0),
            target_cc11: 0.0,
            current_pressure: 0.0,
            velocity_impulse: 0.0,
            curve_gamma: 1.25,
            attack_coeff: 0.01,
            release_coeff: 0.005,
            impulse_decay_coeff: 0.999,
            table_mode: false,
            table_mode_pressure: 0.0,
        };
        model.set_smoothing(1.25, 3.5, 8.0);
        model
    }

    pub fn set_smoothing(&mut self, gamma: f32, attack_ms: f32, release_ms: f32) {
        self.curve_gamma = gamma.clamp(0.5, 3.0);
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
        self.target_cc11 = norm;
    }

    /// Sets continuous high-resolution normalized expression in `[0.0, 1.0]`.
    #[inline]
    pub fn set_expression_norm(&mut self, expr: f32) {
        self.target_cc11 = expr.clamp(0.0, 1.0);
    }

    /// Called on `Note On` with the Bandolibre blade velocity (`1..=127`).
    #[inline]
    pub fn notify_note_on(&mut self, velocity: u8) {
        let vel_norm = (velocity.min(127) as f32) / 127.0;
        // Inject a bounded aerodynamic transient boost only on sharp high-velocity attacks
        let boost = 0.06 * vel_norm.powi(3);
        if boost > self.velocity_impulse {
            self.velocity_impulse = boost;
        }
        if self.table_mode {
            self.table_mode_pressure = vel_norm.powf(self.curve_gamma);
        }
    }

    #[inline]
    pub fn notify_all_notes_off(&mut self) {
        self.table_mode_pressure = 0.0;
    }

    /// Advances the bellows state by one audio sample and returns instantaneous reservoir pressure.
    #[inline]
    pub fn tick(&mut self, total_volume_flow: f32, chamber_coupling: f32) -> f32 {
        let raw_target = if self.table_mode && self.target_cc11 < 1e-4 {
            self.table_mode_pressure
        } else {
            self.target_cc11.powf(self.curve_gamma)
        };

        let coeff = if raw_target > self.current_pressure {
            self.attack_coeff
        } else {
            self.release_coeff
        };

        self.current_pressure += coeff * (raw_target - self.current_pressure);
        if self.current_pressure.abs() < 1e-12 {
            self.current_pressure = 0.0;
        }

        self.velocity_impulse *= self.impulse_decay_coeff;
        if self.velocity_impulse < 1e-12 {
            self.velocity_impulse = 0.0;
        }

        // Shared manifold pressure drop proportional to total airflow drawn by active reeds
        let drop = (chamber_coupling * 0.02 * total_volume_flow.abs()).min(0.25);
        ((self.current_pressure + self.velocity_impulse) * (1.0 - drop)).clamp(0.0, 1.5)
    }

    #[inline]
    pub fn current_pressure(&self) -> f32 {
        self.current_pressure
    }
}
