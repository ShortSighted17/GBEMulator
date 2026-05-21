// src/apu/mod.rs
//
// The DMG APU.
//
// Powerdown / write-while-off behaviour:
//   - Writes to 0xFF10..=0xFF25 while the APU is off are dropped,
//     with exceptions: NR52, the four length registers
//     (NR11/NR21/NR31/NR41) which still update the internal length
//     counter, and wave RAM.
//
//   - Powerdown clears all register bytes 0xFF10..=0xFF25, but the
//     four channels' length counters survive on DMG. Wave RAM also
//     survives.
//
//   - The 512 Hz timer that FEEDS the frame sequencer keeps running
//     even while the APU is off (per gg8 wiki). Only the frame-
//     sequencer DISPATCH (which clocks length/sweep/envelope) is
//     suspended. This is what allows the frame-sequencer phase to
//     be predictable after a power cycle.
//
// Extra length clocking (DMG): when an NRx4 write enables length
// (bit 6 going 0→1) during the "first half" of a length period
// (the next frame-seq step won't clock length), length is clocked
// once immediately. See channel.rs for the per-channel handling.
// We compute `in_first_half` here and thread it into the channel
// write methods.

pub mod channel;

#[cfg(test)]
mod tests;

use channel::{Ch1State, Ch2State, Ch3State, Ch4State, LengthCounter};

const FRAME_SEQ_PERIOD: u32 = 8192;

/// Default internal sample rate, used by tests and as the initial
/// value before the host audio backend tells us its actual rate.
pub const SAMPLE_RATE: u32 = 48_000;

const CPU_HZ: u32 = 4_194_304;

pub struct Apu {
    nr50: u8,
    nr51: u8,
    master_enable: bool,

    pub ch1: Ch1State,
    pub ch2: Ch2State,
    pub ch3: Ch3State,
    pub ch4: Ch4State,

    pub wave_ram: [u8; 0x10],

    /// T-cycle accumulator toward the next 512 Hz tick. Always runs,
    /// even when APU is off, so the frame-sequencer phase remains
    /// continuous across power cycles.
    frame_seq_counter: u32,
    /// 0..7. Advances every 512 Hz tick when master_enable is true.
    /// On power-on, real hardware resets this so the next step is 0.
    pub frame_seq_step: u8,

    sample_rate: u32,
    sample_accumulator: u32,
    pub sample_buffer: Vec<f32>,
}

impl Apu {
    pub fn new() -> Self {
        Self {
            nr50: 0,
            nr51: 0,
            master_enable: false,

            ch1: Ch1State::default(),
            ch2: Ch2State::default(),
            ch3: Ch3State::default(),
            ch4: Ch4State::default(),

            wave_ram: [0; 0x10],

            frame_seq_counter: 0,
            frame_seq_step: 0,

            sample_rate: SAMPLE_RATE,
            sample_accumulator: 0,
            sample_buffer: Vec::new(),
        }
    }

    pub fn set_sample_rate(&mut self, rate: u32) {
        self.sample_rate = rate;
        self.sample_accumulator = 0;
        self.sample_buffer.clear();
    }

    /// True when the NEXT frame-seq step will NOT clock length.
    /// Length is clocked at steps 0/2/4/6 (even). `frame_seq_step`
    /// is the LAST DISPATCHED step, so next = (step + 1) & 7.
    /// Next is odd ⇔ step is even ⇔ no length clock coming up.
    fn in_first_half(&self) -> bool {
        self.frame_seq_step % 2 == 0
    }

    pub fn read_reg(&self, addr: u16) -> u8 {
        match addr {
            0xFF10 => self.ch1.nr10 | 0x80,
            0xFF11 => self.ch1.nr11 | 0x3F,
            0xFF12 => self.ch1.nr12,
            0xFF13 => 0xFF,
            0xFF14 => self.ch1.nr14 | 0xBF,

            0xFF15 => 0xFF,
            0xFF16 => self.ch2.nr21 | 0x3F,
            0xFF17 => self.ch2.nr22,
            0xFF18 => 0xFF,
            0xFF19 => self.ch2.nr24 | 0xBF,

            0xFF1A => self.ch3.nr30 | 0x7F,
            0xFF1B => 0xFF,
            0xFF1C => self.ch3.nr32 | 0x9F,
            0xFF1D => 0xFF,
            0xFF1E => self.ch3.nr34 | 0xBF,

            0xFF1F => 0xFF,
            0xFF20 => 0xFF,
            0xFF21 => self.ch4.nr42,
            0xFF22 => self.ch4.nr43,
            0xFF23 => self.ch4.nr44 | 0xBF,

            0xFF24 => self.nr50,
            0xFF25 => self.nr51,
            0xFF26 => self.read_nr52(),

            0xFF27..=0xFF2F => 0xFF,

            0xFF30..=0xFF3F => self.wave_ram[(addr - 0xFF30) as usize],

            _ => 0xFF,
        }
    }

    pub fn write_reg(&mut self, addr: u16, value: u8) {
        if !self.master_enable {
            match addr {
                0xFF11 => { self.ch1.write_nr11_length_only(value); return; }
                0xFF16 => { self.ch2.write_nr21_length_only(value); return; }
                0xFF1B => { self.ch3.write_nr31_length_only(value); return; }
                0xFF20 => { self.ch4.write_nr41_length_only(value); return; }
                0xFF26 => { self.write_nr52(value); return; }
                0xFF30..=0xFF3F => {
                    self.wave_ram[(addr - 0xFF30) as usize] = value;
                    return;
                }
                _ => {}
            }
            if (0xFF10..=0xFF25).contains(&addr) {
                return;
            }
        }

        // Cache once per write — passed to NRx4 writes for the
        // extra-length-clock quirk.
        let fh = self.in_first_half();

        match addr {
            0xFF10 => self.ch1.write_nr10(value),
            0xFF11 => self.ch1.write_nr11(value),
            0xFF12 => self.ch1.write_nr12(value),
            0xFF13 => self.ch1.write_nr13(value),
            0xFF14 => self.ch1.write_nr14(value, fh),

            0xFF15 => {}
            0xFF16 => self.ch2.write_nr21(value),
            0xFF17 => self.ch2.write_nr22(value),
            0xFF18 => self.ch2.write_nr23(value),
            0xFF19 => self.ch2.write_nr24(value, fh),

            0xFF1A => self.ch3.write_nr30(value),
            0xFF1B => self.ch3.write_nr31(value),
            0xFF1C => self.ch3.write_nr32(value),
            0xFF1D => self.ch3.write_nr33(value),
            0xFF1E => self.ch3.write_nr34(value, fh),

            0xFF1F => {}
            0xFF20 => self.ch4.write_nr41(value),
            0xFF21 => self.ch4.write_nr42(value),
            0xFF22 => self.ch4.write_nr43(value),
            0xFF23 => self.ch4.write_nr44(value, fh),

            0xFF24 => self.nr50 = value,
            0xFF25 => self.nr51 = value,
            0xFF26 => self.write_nr52(value),

            0xFF27..=0xFF2F => {}

            0xFF30..=0xFF3F => self.wave_ram[(addr - 0xFF30) as usize] = value,

            _ => {}
        }
    }

    fn read_nr52(&self) -> u8 {
        let mut v = 0x70u8;
        if self.master_enable { v |= 0x80; }
        if self.ch1.enabled { v |= 0x01; }
        if self.ch2.enabled { v |= 0x02; }
        if self.ch3.enabled { v |= 0x04; }
        if self.ch4.enabled { v |= 0x08; }
        v
    }

    fn write_nr52(&mut self, value: u8) {
        let new_enable = value & 0x80 != 0;

        if !new_enable && self.master_enable {
            // Power-off: clear register bytes, preserve length counters
            // (DMG) and wave RAM. Do NOT touch frame_seq_counter: the
            // 512 Hz timer keeps running even while off, so its phase
            // must remain continuous.
            let saved_lengths: [LengthCounter; 4] = [
                self.ch1.length,
                self.ch2.length,
                self.ch3.length,
                self.ch4.length,
            ];

            self.ch1 = Ch1State::default();
            self.ch2 = Ch2State::default();
            self.ch3 = Ch3State::default();
            self.ch4 = Ch4State::default();
            self.nr50 = 0;
            self.nr51 = 0;

            self.ch1.length = saved_lengths[0];
            self.ch2.length = saved_lengths[1];
            self.ch3.length = saved_lengths[2];
            self.ch4.length = saved_lengths[3];
        }

        if new_enable && !self.master_enable {
            // Power-on: "next step will be 0" per the gg8 wiki.
            // Implement this by resetting frame_seq_step to 7 — the
            // step that fired RIGHT BEFORE step 0 — so the next time
            // the 512Hz timer ticks, we land on step 0. The
            // frame_seq_counter itself is NOT reset, so the time
            // until that first step depends on the 512Hz timer phase
            // (which kept running during powerdown).
            self.frame_seq_step = 7;
        }

        self.master_enable = new_enable;
    }

    pub fn step(&mut self, cycles: u32) {
        let mut remaining = cycles;
        while remaining > 0 {
            let to_next_sample = self.cycles_until_next_sample();
            let chunk = remaining.min(to_next_sample);

            // Channel frequency timers and frame-sequencer dispatch
            // only run when the APU is on.
            if self.master_enable {
                self.ch1.tick(chunk);
                self.ch2.tick(chunk);
                self.ch3.tick(chunk);
                self.ch4.tick(chunk);
            }

            // The 512 Hz timer feeding the frame sequencer keeps
            // running regardless of APU power. Only the dispatch
            // (length/sweep/envelope) is gated on master_enable.
            self.frame_seq_counter += chunk;
            while self.frame_seq_counter >= FRAME_SEQ_PERIOD {
                self.frame_seq_counter -= FRAME_SEQ_PERIOD;
                if self.master_enable {
                    self.tick_frame_sequencer();
                }
            }

            self.emit_samples(chunk);
            remaining -= chunk;
        }
    }

    fn cycles_until_next_sample(&self) -> u32 {
        let needed = CPU_HZ as u64 - self.sample_accumulator as u64;
        let rate = self.sample_rate as u64;
        ((needed + rate - 1) / rate) as u32
    }

    fn tick_frame_sequencer(&mut self) {
        // Advance step counter FIRST, then dispatch on the new value.
        // This is the inverse of our previous "dispatch then advance"
        // logic; the change is consistent with "next step will be 0"
        // semantics — when frame_seq_step is 7 on power-on, the next
        // tick advances it to 0 and runs step 0.
        self.frame_seq_step = (self.frame_seq_step + 1) & 7;
        match self.frame_seq_step {
            0 => {
                self.ch1.tick_length();
                self.ch2.tick_length();
                self.ch3.tick_length();
                self.ch4.tick_length();
            }
            2 => {
                self.ch1.tick_length();
                self.ch2.tick_length();
                self.ch3.tick_length();
                self.ch4.tick_length();
                self.ch1.tick_sweep();
            }
            4 => {
                self.ch1.tick_length();
                self.ch2.tick_length();
                self.ch3.tick_length();
                self.ch4.tick_length();
            }
            6 => {
                self.ch1.tick_length();
                self.ch2.tick_length();
                self.ch3.tick_length();
                self.ch4.tick_length();
                self.ch1.tick_sweep();
            }
            7 => {
                self.ch1.tick_envelope();
                self.ch2.tick_envelope();
                self.ch4.tick_envelope();
            }
            _ => {}
        }
    }

    fn emit_samples(&mut self, cycles: u32) {
        let mut acc = self.sample_accumulator as u64
            + cycles as u64 * self.sample_rate as u64;
        let cpu_hz = CPU_HZ as u64;
        while acc >= cpu_hz {
            acc -= cpu_hz;
            let (l, r) = self.current_sample();
            self.sample_buffer.push(l);
            self.sample_buffer.push(r);
        }
        self.sample_accumulator = acc as u32;
    }

    pub fn current_sample(&self) -> (f32, f32) {
        if !self.master_enable { return (0.0, 0.0); }

        let s1 = self.ch1.sample() as u32;
        let s2 = self.ch2.sample() as u32;
        let s3 = self.ch3.sample(&self.wave_ram) as u32;
        let s4 = self.ch4.sample() as u32;

        let mut left  = 0u32;
        let mut right = 0u32;

        if self.nr51 & 0x01 != 0 { right += s1; }
        if self.nr51 & 0x02 != 0 { right += s2; }
        if self.nr51 & 0x04 != 0 { right += s3; }
        if self.nr51 & 0x08 != 0 { right += s4; }
        if self.nr51 & 0x10 != 0 { left  += s1; }
        if self.nr51 & 0x20 != 0 { left  += s2; }
        if self.nr51 & 0x40 != 0 { left  += s3; }
        if self.nr51 & 0x80 != 0 { left  += s4; }

        let vol_r = (self.nr50 & 0x07) + 1;
        let vol_l = ((self.nr50 >> 4) & 0x07) + 1;

        let l = (left  * vol_l as u32) as f32 / 480.0;
        let r = (right * vol_r as u32) as f32 / 480.0;
        (l, r)
    }

    pub fn take_samples(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.sample_buffer)
    }
}

impl Default for Apu {
    fn default() -> Self { Self::new() }
}