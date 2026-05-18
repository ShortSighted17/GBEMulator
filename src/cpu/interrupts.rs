// src/cpu/interrupts.rs
//
// Interrupt service routine dispatch.
// Priority: VBlank(bit 0) > STAT(1) > Timer(2) > Serial(3) > Joypad(4).
// Vectors: 0x40, 0x48, 0x50, 0x58, 0x60.

use crate::cpu::Cpu;
use crate::memory::Bus;

impl<B: Bus> Cpu<B> {
    /// If an interrupt should fire right now, do it and return Some(cycles).
    /// Otherwise return None.
    pub(crate) fn service_interrupt(&mut self) -> Option<u32> {
        if !self.ime { return None; }

        let if_flags = self.bus.read(0xFF0F);
        let ie       = self.bus.read(0xFFFF);
        let pending  = if_flags & ie & 0x1F;
        if pending == 0 { return None; }

        // Lowest-numbered bit wins (VBlank highest priority).
        let bit = pending.trailing_zeros() as u8;

        // Clear the bit in IF.
        self.bus.write(0xFF0F, if_flags & !(1 << bit));

        // Disable IME until RETI.
        self.ime = false;
        self.ime_state = crate::cpu::ImeState::Disabled;

        // Wake from HALT if we were halted.
        self.halted = false;

        // Push current PC, jump to vector.
        self.push(self.regs.pc);
        self.regs.pc = 0x40 + (bit as u16) * 8;

        Some(20)
    }
}
