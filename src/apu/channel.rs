// src/apu/channel.rs
//
// Per-channel state and shared sub-units.
//
// Length-register quirk (DMG): while the APU is powered off, writes to
// NR11/NR21/NR31/NR41 still update the internal length counter, but
// nothing else — the duty bits and the raw NRxx byte stay at zero.
// That's why each channel has both a `write_nrX1` (full, used when on)
// and a `write_nrX1_length_only` (used when off).

/// The four duty patterns, indexed by NR11/NR21 bits 7-6.
const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1],  // 12.5%
    [1, 0, 0, 0, 0, 0, 0, 1],  // 25%
    [1, 0, 0, 0, 0, 1, 1, 1],  // 50%
    [0, 1, 1, 1, 1, 1, 1, 0],  // 75%
];

// ────────────────────────────────────────────────────────────────────────
// Shared sub-units
// ────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone, Copy)]
pub struct LengthCounter {
    pub counter: u16,
    pub enabled: bool,
}

impl LengthCounter {
    pub fn tick(&mut self) -> bool {
        if self.enabled && self.counter > 0 {
            self.counter -= 1;
            if self.counter == 0 {
                return true;
            }
        }
        false
    }
}

#[derive(Default, Clone, Copy)]
pub struct Envelope {
    pub volume:       u8,
    pub initial:      u8,
    pub direction_up: bool,
    pub period:       u8,
    pub timer:        u8,
}

impl Envelope {
    pub fn reload(&mut self, nrx2: u8) {
        self.initial      = nrx2 >> 4;
        self.direction_up = nrx2 & 0x08 != 0;
        self.period       = nrx2 & 0x07;
        self.volume       = self.initial;
        self.timer        = self.period;
    }

    pub fn tick(&mut self) {
        if self.period == 0 { return; }
        if self.timer > 0 { self.timer -= 1; }
        if self.timer == 0 {
            self.timer = self.period;
            if self.direction_up && self.volume < 15 {
                self.volume += 1;
            } else if !self.direction_up && self.volume > 0 {
                self.volume -= 1;
            }
        }
    }
}

#[derive(Default, Clone, Copy)]
pub struct Sweep {
    pub period:  u8,
    pub negate:  bool,
    pub shift:   u8,
    pub timer:   u8,
    pub enabled: bool,
    pub shadow:  u16,
}

impl Sweep {
    pub fn trigger(&mut self, nr10: u8, current_freq: u16) -> Result<(), ()> {
        self.period = (nr10 >> 4) & 0x07;
        self.negate = nr10 & 0x08 != 0;
        self.shift  = nr10 & 0x07;
        self.shadow = current_freq;
        self.timer  = if self.period == 0 { 8 } else { self.period };
        self.enabled = self.period != 0 || self.shift != 0;
        if self.shift != 0 {
            let next = self.calculate();
            if next > 2047 { return Err(()); }
        }
        Ok(())
    }

    pub fn calculate(&self) -> u16 {
        let delta = self.shadow >> self.shift;
        if self.negate {
            self.shadow.wrapping_sub(delta)
        } else {
            self.shadow.wrapping_add(delta)
        }
    }
}

// ────────────────────────────────────────────────────────────────────────
// Channel 1 — square with sweep
// ────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone, Copy)]
pub struct Ch1State {
    pub nr10: u8, pub nr11: u8, pub nr12: u8, pub nr13: u8, pub nr14: u8,

    pub length:   LengthCounter,
    pub envelope: Envelope,
    pub sweep:    Sweep,

    pub freq:        u16,
    pub duty:        u8,
    pub duty_step:   u8,
    pub freq_timer:  u32,

    pub dac_on:      bool,
    pub enabled:     bool,
}

impl Ch1State {
    pub fn write_nr10(&mut self, value: u8) {
        self.nr10 = value;
    }

    pub fn write_nr11(&mut self, value: u8) {
        self.nr11 = value;
        self.duty = value >> 6;
        self.length.counter = 64 - (value as u16 & 0x3F);
    }

    /// DMG: while the APU is off, NR11 writes still reload the length
    /// counter but don't touch duty or the readable byte.
    pub fn write_nr11_length_only(&mut self, value: u8) {
        self.length.counter = 64 - (value as u16 & 0x3F);
    }

    pub fn write_nr12(&mut self, value: u8) {
        self.nr12 = value;
        self.dac_on = value & 0xF8 != 0;
        if !self.dac_on { self.enabled = false; }
    }

    pub fn write_nr13(&mut self, value: u8) {
        self.nr13 = value;
        self.freq = (self.freq & 0x0700) | value as u16;
    }

    pub fn write_nr14(&mut self, value: u8) {
        self.nr14 = value;
        self.freq = (self.freq & 0x00FF) | ((value as u16 & 0x07) << 8);
        self.length.enabled = value & 0x40 != 0;
        if value & 0x80 != 0 {
            self.trigger();
        }
    }

    fn trigger(&mut self) {
        if self.dac_on { self.enabled = true; }
        if self.length.counter == 0 {
            self.length.counter = 64;
        }
        self.freq_timer = (2048 - self.freq as u32) * 4;
        self.envelope.reload(self.nr12);
        if self.sweep.trigger(self.nr10, self.freq).is_err() {
            self.enabled = false;
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        let mut remaining = cycles;
        while remaining > 0 {
            if self.freq_timer == 0 {
                self.freq_timer = (2048 - self.freq as u32) * 4;
                self.duty_step = (self.duty_step + 1) & 7;
            }
            let chunk = remaining.min(self.freq_timer);
            self.freq_timer -= chunk;
            remaining -= chunk;
        }
    }

    pub fn sample(&self) -> u8 {
        if !self.enabled || !self.dac_on { return 0; }
        let pattern_bit = DUTY_TABLE[self.duty as usize][self.duty_step as usize];
        if pattern_bit == 0 { 0 } else { self.envelope.volume }
    }

    pub fn tick_length(&mut self) {
        if self.length.tick() { self.enabled = false; }
    }

    pub fn tick_sweep(&mut self) {
        if !self.sweep.enabled { return; }
        if self.sweep.timer > 0 { self.sweep.timer -= 1; }
        if self.sweep.timer == 0 {
            self.sweep.timer = if self.sweep.period == 0 { 8 } else { self.sweep.period };
            if self.sweep.period != 0 && self.sweep.shift != 0 {
                let next = self.sweep.calculate();
                if next > 2047 {
                    self.enabled = false;
                } else {
                    self.sweep.shadow = next;
                    self.freq = next;
                    self.nr13 = (next & 0xFF) as u8;
                    self.nr14 = (self.nr14 & 0xF8) | ((next >> 8) as u8 & 0x07);
                    if self.sweep.calculate() > 2047 {
                        self.enabled = false;
                    }
                }
            }
        }
    }

    pub fn tick_envelope(&mut self) {
        self.envelope.tick();
    }
}

// ────────────────────────────────────────────────────────────────────────
// Channel 2 — square, no sweep
// ────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone, Copy)]
pub struct Ch2State {
    pub nr21: u8, pub nr22: u8, pub nr23: u8, pub nr24: u8,

    pub length:   LengthCounter,
    pub envelope: Envelope,

    pub freq:        u16,
    pub duty:        u8,
    pub duty_step:   u8,
    pub freq_timer:  u32,
    pub dac_on:      bool,
    pub enabled:     bool,
}

impl Ch2State {
    pub fn write_nr21(&mut self, value: u8) {
        self.nr21 = value;
        self.duty = value >> 6;
        self.length.counter = 64 - (value as u16 & 0x3F);
    }

    pub fn write_nr21_length_only(&mut self, value: u8) {
        self.length.counter = 64 - (value as u16 & 0x3F);
    }

    pub fn write_nr22(&mut self, value: u8) {
        self.nr22 = value;
        self.dac_on = value & 0xF8 != 0;
        if !self.dac_on { self.enabled = false; }
    }

    pub fn write_nr23(&mut self, value: u8) {
        self.nr23 = value;
        self.freq = (self.freq & 0x0700) | value as u16;
    }

    pub fn write_nr24(&mut self, value: u8) {
        self.nr24 = value;
        self.freq = (self.freq & 0x00FF) | ((value as u16 & 0x07) << 8);
        self.length.enabled = value & 0x40 != 0;
        if value & 0x80 != 0 {
            self.trigger();
        }
    }

    fn trigger(&mut self) {
        if self.dac_on { self.enabled = true; }
        if self.length.counter == 0 { self.length.counter = 64; }
        self.freq_timer = (2048 - self.freq as u32) * 4;
        self.envelope.reload(self.nr22);
    }

    pub fn tick(&mut self, cycles: u32) {
        let mut remaining = cycles;
        while remaining > 0 {
            if self.freq_timer == 0 {
                self.freq_timer = (2048 - self.freq as u32) * 4;
                self.duty_step = (self.duty_step + 1) & 7;
            }
            let chunk = remaining.min(self.freq_timer);
            self.freq_timer -= chunk;
            remaining -= chunk;
        }
    }

    pub fn sample(&self) -> u8 {
        if !self.enabled || !self.dac_on { return 0; }
        let pattern_bit = DUTY_TABLE[self.duty as usize][self.duty_step as usize];
        if pattern_bit == 0 { 0 } else { self.envelope.volume }
    }

    pub fn tick_length(&mut self) {
        if self.length.tick() { self.enabled = false; }
    }

    pub fn tick_envelope(&mut self) {
        self.envelope.tick();
    }
}

// ────────────────────────────────────────────────────────────────────────
// Channel 3 — wave
// ────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone, Copy)]
pub struct Ch3State {
    pub nr30: u8, pub nr31: u8, pub nr32: u8, pub nr33: u8, pub nr34: u8,

    pub length: LengthCounter,

    pub freq:           u16,
    pub volume_shift:   u8,
    pub sample_index:   u8,
    pub freq_timer:     u32,
    pub dac_on:         bool,
    pub enabled:        bool,
}

impl Ch3State {
    pub fn write_nr30(&mut self, value: u8) {
        self.nr30 = value;
        self.dac_on = value & 0x80 != 0;
        if !self.dac_on { self.enabled = false; }
    }

    pub fn write_nr31(&mut self, value: u8) {
        self.nr31 = value;
        self.length.counter = 256 - value as u16;
    }

    pub fn write_nr31_length_only(&mut self, value: u8) {
        self.length.counter = 256 - value as u16;
    }

    pub fn write_nr32(&mut self, value: u8) {
        self.nr32 = value;
        self.volume_shift = (value >> 5) & 0x03;
    }

    pub fn write_nr33(&mut self, value: u8) {
        self.nr33 = value;
        self.freq = (self.freq & 0x0700) | value as u16;
    }

    pub fn write_nr34(&mut self, value: u8) {
        self.nr34 = value;
        self.freq = (self.freq & 0x00FF) | ((value as u16 & 0x07) << 8);
        self.length.enabled = value & 0x40 != 0;
        if value & 0x80 != 0 {
            self.trigger();
        }
    }

    fn trigger(&mut self) {
        if self.dac_on { self.enabled = true; }
        if self.length.counter == 0 { self.length.counter = 256; }
        self.freq_timer = (2048 - self.freq as u32) * 2;
        self.sample_index = 0;
    }

    pub fn tick(&mut self, cycles: u32) {
        let mut remaining = cycles;
        while remaining > 0 {
            if self.freq_timer == 0 {
                self.freq_timer = (2048 - self.freq as u32) * 2;
                self.sample_index = (self.sample_index + 1) & 0x1F;
            }
            let chunk = remaining.min(self.freq_timer);
            self.freq_timer -= chunk;
            remaining -= chunk;
        }
    }

    pub fn sample(&self, wave_ram: &[u8; 16]) -> u8 {
        if !self.enabled || !self.dac_on { return 0; }
        let byte = wave_ram[(self.sample_index >> 1) as usize];
        let nibble = if self.sample_index & 1 == 0 {
            byte >> 4
        } else {
            byte & 0x0F
        };
        match self.volume_shift {
            0 => 0,
            1 => nibble,
            2 => nibble >> 1,
            3 => nibble >> 2,
            _ => 0,
        }
    }

    pub fn tick_length(&mut self) {
        if self.length.tick() { self.enabled = false; }
    }
}

// ────────────────────────────────────────────────────────────────────────
// Channel 4 — noise (LFSR)
// ────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone, Copy)]
pub struct Ch4State {
    pub nr41: u8, pub nr42: u8, pub nr43: u8, pub nr44: u8,

    pub length:   LengthCounter,
    pub envelope: Envelope,

    pub lfsr:        u16,
    pub width_mode:  bool,
    pub freq_timer:  u32,
    pub dac_on:      bool,
    pub enabled:     bool,
}

impl Ch4State {
    pub fn write_nr41(&mut self, value: u8) {
        self.nr41 = value;
        self.length.counter = 64 - (value as u16 & 0x3F);
    }

    pub fn write_nr41_length_only(&mut self, value: u8) {
        self.length.counter = 64 - (value as u16 & 0x3F);
    }

    pub fn write_nr42(&mut self, value: u8) {
        self.nr42 = value;
        self.dac_on = value & 0xF8 != 0;
        if !self.dac_on { self.enabled = false; }
    }

    pub fn write_nr43(&mut self, value: u8) {
        self.nr43 = value;
        self.width_mode = value & 0x08 != 0;
    }

    pub fn write_nr44(&mut self, value: u8) {
        self.nr44 = value;
        self.length.enabled = value & 0x40 != 0;
        if value & 0x80 != 0 {
            self.trigger();
        }
    }

    fn trigger(&mut self) {
        if self.dac_on { self.enabled = true; }
        if self.length.counter == 0 { self.length.counter = 64; }
        self.envelope.reload(self.nr42);
        self.freq_timer = self.noise_period();
        self.lfsr = 0x7FFF;
    }

    fn noise_period(&self) -> u32 {
        let code = (self.nr43 & 0x07) as u32;
        let shift = ((self.nr43 >> 4) & 0x0F) as u32;
        let divisor = if code == 0 { 8 } else { code << 4 };
        divisor << shift
    }

    pub fn tick(&mut self, cycles: u32) {
        let mut remaining = cycles;
        while remaining > 0 {
            if self.freq_timer == 0 {
                self.freq_timer = self.noise_period();
                self.step_lfsr();
            }
            let chunk = remaining.min(self.freq_timer);
            self.freq_timer -= chunk;
            remaining -= chunk;
        }
    }

    fn step_lfsr(&mut self) {
        let bit = (self.lfsr & 0x01) ^ ((self.lfsr >> 1) & 0x01);
        self.lfsr >>= 1;
        self.lfsr |= bit << 14;
        if self.width_mode {
            self.lfsr = (self.lfsr & !(1 << 6)) | (bit << 6);
        }
    }

    pub fn sample(&self) -> u8 {
        if !self.enabled || !self.dac_on { return 0; }
        if self.lfsr & 0x01 == 0 {
            self.envelope.volume
        } else {
            0
        }
    }

    pub fn tick_length(&mut self) {
        if self.length.tick() { self.enabled = false; }
    }

    pub fn tick_envelope(&mut self) {
        self.envelope.tick();
    }
}