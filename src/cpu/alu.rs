// src/cpu/alu.rs

use crate::cpu::Cpu;
use crate::cpu::registers::Flag;
use crate::memory::Bus;

impl<B: Bus> Cpu<B> {
    // ── 8-bit ALU on A ──────────────────────────────────────────────────

    pub(crate) fn alu_add(&mut self, value: u8) {
        let a = self.regs.a;
        let result = a.wrapping_add(value);
        self.regs.set_flag(Flag::Z, result == 0);
        self.regs.set_flag(Flag::N, false);
        self.regs.set_flag(Flag::H, (a & 0x0F) + (value & 0x0F) > 0x0F);
        self.regs.set_flag(Flag::C, (a as u16) + (value as u16) > 0xFF);
        self.regs.a = result;
    }

    pub(crate) fn alu_adc(&mut self, value: u8) {
        let a = self.regs.a;
        let c = if self.regs.get_flag(Flag::C) { 1u8 } else { 0u8 };
        let result = a.wrapping_add(value).wrapping_add(c);
        self.regs.set_flag(Flag::Z, result == 0);
        self.regs.set_flag(Flag::N, false);
        self.regs.set_flag(Flag::H, (a & 0x0F) + (value & 0x0F) + c > 0x0F);
        self.regs.set_flag(Flag::C, (a as u16) + (value as u16) + (c as u16) > 0xFF);
        self.regs.a = result;
    }

    pub(crate) fn alu_sub(&mut self, value: u8) {
        let a = self.regs.a;
        let result = a.wrapping_sub(value);
        self.regs.set_flag(Flag::Z, result == 0);
        self.regs.set_flag(Flag::N, true);
        self.regs.set_flag(Flag::H, (a & 0x0F) < (value & 0x0F));
        self.regs.set_flag(Flag::C, (a as u16) < (value as u16));
        self.regs.a = result;
    }

    pub(crate) fn alu_sbc(&mut self, value: u8) {
        let a = self.regs.a;
        let c = if self.regs.get_flag(Flag::C) { 1u8 } else { 0u8 };
        let result = a.wrapping_sub(value).wrapping_sub(c);
        self.regs.set_flag(Flag::Z, result == 0);
        self.regs.set_flag(Flag::N, true);
        self.regs.set_flag(Flag::H, (a & 0x0F) < (value & 0x0F) + c);
        self.regs.set_flag(Flag::C, (a as u16) < (value as u16) + (c as u16));
        self.regs.a = result;
    }

    pub(crate) fn alu_and(&mut self, value: u8) {
        let result = self.regs.a & value;
        self.regs.a = result;
        self.regs.set_flag(Flag::Z, result == 0);
        self.regs.set_flag(Flag::N, false);
        self.regs.set_flag(Flag::H, true);
        self.regs.set_flag(Flag::C, false);
    }

    pub(crate) fn alu_xor(&mut self, value: u8) {
        let result = self.regs.a ^ value;
        self.regs.a = result;
        self.regs.set_flag(Flag::Z, result == 0);
        self.regs.set_flag(Flag::N, false);
        self.regs.set_flag(Flag::H, false);
        self.regs.set_flag(Flag::C, false);
    }

    pub(crate) fn alu_or(&mut self, value: u8) {
        let result = self.regs.a | value;
        self.regs.a = result;
        self.regs.set_flag(Flag::Z, result == 0);
        self.regs.set_flag(Flag::N, false);
        self.regs.set_flag(Flag::H, false);
        self.regs.set_flag(Flag::C, false);
    }

    pub(crate) fn alu_cp(&mut self, value: u8) {
        let a = self.regs.a;
        let result = a.wrapping_sub(value);
        self.regs.set_flag(Flag::Z, result == 0);
        self.regs.set_flag(Flag::N, true);
        self.regs.set_flag(Flag::H, (a & 0x0F) < (value & 0x0F));
        self.regs.set_flag(Flag::C, a < value);
    }

    pub(crate) fn alu_inc8(&mut self, value: u8) -> u8 {
        let result = value.wrapping_add(1);
        self.regs.set_flag(Flag::Z, result == 0);
        self.regs.set_flag(Flag::N, false);
        self.regs.set_flag(Flag::H, (value & 0x0F) + 1 > 0x0F);
        result
    }

    pub(crate) fn alu_dec8(&mut self, value: u8) -> u8 {
        let result = value.wrapping_sub(1);
        self.regs.set_flag(Flag::Z, result == 0);
        self.regs.set_flag(Flag::N, true);
        self.regs.set_flag(Flag::H, (value & 0x0F) == 0);
        result
    }

    /// DAA: decimal-adjust A after a BCD add/subtract.
    /// Reads N, H, C; writes Z, H=0, C (only when N=0).
    pub(crate) fn alu_daa(&mut self) {
        let mut a = self.regs.a;
        let n = self.regs.get_flag(Flag::N);
        let h = self.regs.get_flag(Flag::H);
        let c = self.regs.get_flag(Flag::C);
        let mut new_c = c;

        if !n {
            // After ADD/ADC: adjust upward.
            if c || a > 0x99 {
                a = a.wrapping_add(0x60);
                new_c = true;
            }
            if h || (a & 0x0F) > 0x09 {
                a = a.wrapping_add(0x06);
            }
        } else {
            // After SUB/SBC: adjust downward.
            if c { a = a.wrapping_sub(0x60); }
            if h { a = a.wrapping_sub(0x06); }
        }

        self.regs.a = a;
        self.regs.set_flag(Flag::Z, a == 0);
        self.regs.set_flag(Flag::H, false);
        self.regs.set_flag(Flag::C, new_c);
        // N is preserved.
    }

    // ── Accumulator rotates (un-prefixed: Z is always 0) ────────────────

    pub(crate) fn alu_rlca(&mut self) {
        let a = self.regs.a;
        let c = a >> 7;
        self.regs.a = (a << 1) | c;
        self.regs.set_flag(Flag::Z, false);
        self.regs.set_flag(Flag::N, false);
        self.regs.set_flag(Flag::H, false);
        self.regs.set_flag(Flag::C, c != 0);
    }

    pub(crate) fn alu_rrca(&mut self) {
        let a = self.regs.a;
        let c = a & 1;
        self.regs.a = (a >> 1) | (c << 7);
        self.regs.set_flag(Flag::Z, false);
        self.regs.set_flag(Flag::N, false);
        self.regs.set_flag(Flag::H, false);
        self.regs.set_flag(Flag::C, c != 0);
    }

    pub(crate) fn alu_rla(&mut self) {
        let a = self.regs.a;
        let old_c = if self.regs.get_flag(Flag::C) { 1u8 } else { 0u8 };
        let new_c = a >> 7;
        self.regs.a = (a << 1) | old_c;
        self.regs.set_flag(Flag::Z, false);
        self.regs.set_flag(Flag::N, false);
        self.regs.set_flag(Flag::H, false);
        self.regs.set_flag(Flag::C, new_c != 0);
    }

    pub(crate) fn alu_rra(&mut self) {
        let a = self.regs.a;
        let old_c = if self.regs.get_flag(Flag::C) { 1u8 } else { 0u8 };
        let new_c = a & 1;
        self.regs.a = (a >> 1) | (old_c << 7);
        self.regs.set_flag(Flag::Z, false);
        self.regs.set_flag(Flag::N, false);
        self.regs.set_flag(Flag::H, false);
        self.regs.set_flag(Flag::C, new_c != 0);
    }

    // ── Misc accumulator/flag ops ───────────────────────────────────────

    /// CPL: A = ~A. N=1, H=1. Z and C preserved.
    pub(crate) fn alu_cpl(&mut self) {
        self.regs.a = !self.regs.a;
        self.regs.set_flag(Flag::N, true);
        self.regs.set_flag(Flag::H, true);
    }

    /// SCF: C=1, N=0, H=0. Z preserved.
    pub(crate) fn alu_scf(&mut self) {
        self.regs.set_flag(Flag::N, false);
        self.regs.set_flag(Flag::H, false);
        self.regs.set_flag(Flag::C, true);
    }

    /// CCF: C = !C, N=0, H=0. Z preserved.
    pub(crate) fn alu_ccf(&mut self) {
        let c = self.regs.get_flag(Flag::C);
        self.regs.set_flag(Flag::N, false);
        self.regs.set_flag(Flag::H, false);
        self.regs.set_flag(Flag::C, !c);
    }

    // ── 16-bit add ──────────────────────────────────────────────────────

    /// ADD HL, value. Z preserved. N=0. H from bit 11. C from bit 15.
    pub(crate) fn alu_add_hl(&mut self, value: u16) {
        let hl = self.regs.hl();
        let result = hl.wrapping_add(value);
        self.regs.set_flag(Flag::N, false);
        // Half-carry on the high byte: carry from bit 11 to 12.
        self.regs.set_flag(Flag::H, (hl & 0x0FFF) + (value & 0x0FFF) > 0x0FFF);
        self.regs.set_flag(Flag::C, (hl as u32) + (value as u32) > 0xFFFF);
        self.regs.set_hl(result);
    }

    /// ADD SP, r8 and LD HL, SP+r8.
    /// Z=0, N=0. H and C computed against the *low byte* of SP as if 8-bit add.
    pub(crate) fn alu_add_sp_i8(&mut self, offset: i8) -> u16 {
        let sp = self.regs.sp;
        let off = offset as i16 as u16; // sign-extend then reinterpret
        let result = sp.wrapping_add(off);
        // For flag purposes, treat as: low byte of SP + the offset byte.
        let sp_lo = sp & 0xFF;
        let off_lo = off & 0xFF;
        self.regs.set_flag(Flag::Z, false);
        self.regs.set_flag(Flag::N, false);
        self.regs.set_flag(Flag::H, (sp_lo & 0x0F) + (off_lo & 0x0F) > 0x0F);
        self.regs.set_flag(Flag::C, sp_lo + off_lo > 0xFF);
        result
    }
}
