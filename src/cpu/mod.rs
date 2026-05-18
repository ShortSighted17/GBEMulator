// src/cpu/mod.rs

pub mod registers;
pub mod alu;
mod execute;
mod execute_cb;
mod interrupts;

#[cfg(test)]
mod tests;

use crate::memory::Bus;
use registers::Registers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImeState {
    Disabled,
    EnablePending,
    Enabled,
}

pub struct Cpu<B: Bus> {
    pub regs: Registers,
    pub bus: B,
    pub halted: bool,
    pub ime: bool,
    pub(crate) ime_state: ImeState,
    /// When true, prints a gameboy-doctor-compatible trace line before each fetch.
    pub trace: bool,
}

impl<B: Bus> Cpu<B> {
    pub fn new(bus: B) -> Self {
        Self {
            regs: Registers::new(),
            bus,
            halted: false,
            ime: false,
            ime_state: ImeState::Disabled,
            trace: false,
        }
    }

    pub(crate) fn fetch_byte(&mut self) -> u8 {
        let b = self.bus.read(self.regs.pc);
        self.regs.pc = self.regs.pc.wrapping_add(1);
        b
    }

    pub(crate) fn fetch_word(&mut self) -> u16 {
        let lo = self.fetch_byte() as u16;
        let hi = self.fetch_byte() as u16;
        (hi << 8) | lo
    }

    fn emit_trace(&self) {
        let r = &self.regs;
        println!(
            "A:{:02X} F:{:02X} B:{:02X} C:{:02X} D:{:02X} E:{:02X} H:{:02X} L:{:02X} SP:{:04X} PC:{:04X} PCMEM:{:02X},{:02X},{:02X},{:02X}",
            r.a, r.f, r.b, r.c, r.d, r.e, r.h, r.l, r.sp, r.pc,
            self.bus.read(r.pc),
            self.bus.read(r.pc.wrapping_add(1)),
            self.bus.read(r.pc.wrapping_add(2)),
            self.bus.read(r.pc.wrapping_add(3)),
        );
    }

    pub fn step(&mut self) -> u32 {
        if let Some(cycles) = self.service_interrupt() {
            return cycles;
        }

        if self.halted {
            let pending = self.bus.read(0xFF0F) & self.bus.read(0xFFFF) & 0x1F;
            if pending != 0 {
                self.halted = false;
            }
            return 4;
        }

        if self.trace { self.emit_trace(); }

        let about_to_enable = self.ime_state == ImeState::EnablePending;
        let opcode = self.fetch_byte();
        let cycles = self.execute(opcode);
        if about_to_enable {
            self.ime = true;
            self.ime_state = ImeState::Enabled;
        }
        cycles
    }
}
