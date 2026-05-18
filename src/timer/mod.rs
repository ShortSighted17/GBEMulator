// src/timer/mod.rs
//
// The Game Boy timer.
//
// Four memory-mapped registers:
//   FF04  DIV   16384 Hz internal counter; any write resets it to 0.
//   FF05  TIMA  counter, increments at the rate selected by TAC.
//   FF06  TMA   value TIMA reloads to when it overflows from 0xFF.
//   FF07  TAC   bit 2 = timer enable; bits 1-0 = rate select.
//
// On TIMA overflow, we set the timer-interrupt flag (IF bit 2) so the
// CPU's existing interrupt dispatch picks it up.

pub struct Timer {
    div_counter:  u32, // counts T-cycles toward the next DIV tick (per 256)
    tima_counter: u32, // counts T-cycles toward the next TIMA tick (per TAC rate)

    div:  u8,
    tima: u8,
    tma:  u8,
    tac:  u8,

    /// Set true on TIMA overflow; the MMU consumes this and ORs it into IF.
    pub interrupt_request: bool,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            div_counter: 0, tima_counter: 0,
            div: 0, tima: 0, tma: 0, tac: 0,
            interrupt_request: false,
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0xFF04 => self.div,
            0xFF05 => self.tima,
            0xFF06 => self.tma,
            // Unused bits of TAC always read as 1.
            0xFF07 => self.tac | 0xF8,
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            // Any write to DIV resets the internal counter and the visible byte.
            0xFF04 => { self.div = 0; self.div_counter = 0; }
            0xFF05 => { self.tima = value; }
            0xFF06 => { self.tma  = value; }
            0xFF07 => { self.tac  = value & 0x07; }
            _ => {}
        }
    }

    /// Advance by `cycles` T-cycles. Should be called every CPU step.
    pub fn step(&mut self, cycles: u32) {
        // DIV ticks every 256 T-cycles regardless of TAC.
        self.div_counter += cycles;
        while self.div_counter >= 256 {
            self.div_counter -= 256;
            self.div = self.div.wrapping_add(1);
        }

        // TIMA only ticks when enabled.
        if self.tac & 0x04 == 0 { return; }

        let period = match self.tac & 0x03 {
            0 => 1024,
            1 => 16,
            2 => 64,
            3 => 256,
            _ => unreachable!(),
        };

        self.tima_counter += cycles;
        while self.tima_counter >= period {
            self.tima_counter -= period;
            let (next, overflow) = self.tima.overflowing_add(1);
            if overflow {
                self.tima = self.tma;
                self.interrupt_request = true;
            } else {
                self.tima = next;
            }
        }
    }
}

impl Default for Timer { fn default() -> Self { Self::new() } }
