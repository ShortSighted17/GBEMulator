// src/cpu/execute_cb.rs
//
// The CB-prefix opcode table. Four 64-instruction families decoded by
// bit fields:
//   0x00..=0x3F  Rotates/shifts.  op = (opcode >> 3) & 0x07, src = opcode & 7
//                op: 0 RLC, 1 RRC, 2 RL, 3 RR, 4 SLA, 5 SRA, 6 SWAP, 7 SRL
//   0x40..=0x7F  BIT n, r.  n = (opcode >> 3) & 0x07
//   0x80..=0xBF  RES n, r.
//   0xC0..=0xFF  SET n, r.
//
// All rotate/shift forms set Z based on the result.

use crate::cpu::Cpu;
use crate::cpu::registers::Flag;
use crate::memory::Bus;

impl<B: Bus> Cpu<B> {
    pub(crate) fn execute_cb(&mut self, opcode: u8) -> u32 {
        let reg = opcode & 0x07;
        let cycles = if reg == 6 { 16 } else { 8 };

        // BIT n, r is special: 12 cycles when reg=(HL), not 16.
        let bit_cycles_hl = 12;

        match opcode {
            0x00..=0x3F => {
                let op = (opcode >> 3) & 0x07;
                let v = self.cb_read(reg);
                let r = match op {
                    0 => self.cb_rlc(v),
                    1 => self.cb_rrc(v),
                    2 => self.cb_rl(v),
                    3 => self.cb_rr(v),
                    4 => self.cb_sla(v),
                    5 => self.cb_sra(v),
                    6 => self.cb_swap(v),
                    7 => self.cb_srl(v),
                    _ => unreachable!(),
                };
                self.cb_write(reg, r);
                cycles
            }
            0x40..=0x7F => {
                let n = (opcode >> 3) & 0x07;
                let v = self.cb_read(reg);
                self.cb_bit(n, v);
                if reg == 6 { bit_cycles_hl } else { 8 }
            }
            0x80..=0xBF => {
                let n = (opcode >> 3) & 0x07;
                let v = self.cb_read(reg);
                self.cb_write(reg, v & !(1 << n));
                cycles
            }
            0xC0..=0xFF => {
                let n = (opcode >> 3) & 0x07;
                let v = self.cb_read(reg);
                self.cb_write(reg, v | (1 << n));
                cycles
            }
        }
    }

    // ── Register lookup (mirrors read_reg/write_reg in execute.rs) ──────

    fn cb_read(&self, idx: u8) -> u8 {
        match idx & 0x07 {
            0 => self.regs.b, 1 => self.regs.c,
            2 => self.regs.d, 3 => self.regs.e,
            4 => self.regs.h, 5 => self.regs.l,
            6 => self.bus.read(self.regs.hl()),
            7 => self.regs.a,
            _ => unreachable!(),
        }
    }

    fn cb_write(&mut self, idx: u8, value: u8) {
        match idx & 0x07 {
            0 => self.regs.b = value, 1 => self.regs.c = value,
            2 => self.regs.d = value, 3 => self.regs.e = value,
            4 => self.regs.h = value, 5 => self.regs.l = value,
            6 => self.bus.write(self.regs.hl(), value),
            7 => self.regs.a = value,
            _ => unreachable!(),
        }
    }

    // ── Rotate/shift primitives. All set Z based on result, N=0, H=0. ───

    fn cb_rlc(&mut self, value: u8) -> u8 {
        let c = value >> 7;
        let r = (value << 1) | c;
        self.set_zhnc(r == 0, false, false, c != 0);
        r
    }

    fn cb_rrc(&mut self, value: u8) -> u8 {
        let c = value & 1;
        let r = (value >> 1) | (c << 7);
        self.set_zhnc(r == 0, false, false, c != 0);
        r
    }

    fn cb_rl(&mut self, value: u8) -> u8 {
        let old_c = if self.regs.get_flag(Flag::C) { 1u8 } else { 0u8 };
        let new_c = value >> 7;
        let r = (value << 1) | old_c;
        self.set_zhnc(r == 0, false, false, new_c != 0);
        r
    }

    fn cb_rr(&mut self, value: u8) -> u8 {
        let old_c = if self.regs.get_flag(Flag::C) { 1u8 } else { 0u8 };
        let new_c = value & 1;
        let r = (value >> 1) | (old_c << 7);
        self.set_zhnc(r == 0, false, false, new_c != 0);
        r
    }

    fn cb_sla(&mut self, value: u8) -> u8 {
        let c = value >> 7;
        let r = value << 1;
        self.set_zhnc(r == 0, false, false, c != 0);
        r
    }

    /// SRA: arithmetic right shift (bit 7 preserved).
    fn cb_sra(&mut self, value: u8) -> u8 {
        let c = value & 1;
        let r = (value >> 1) | (value & 0x80);
        self.set_zhnc(r == 0, false, false, c != 0);
        r
    }

    fn cb_swap(&mut self, value: u8) -> u8 {
        let r = (value << 4) | (value >> 4);
        self.set_zhnc(r == 0, false, false, false);
        r
    }

    /// SRL: logical right shift (bit 7 → 0).
    fn cb_srl(&mut self, value: u8) -> u8 {
        let c = value & 1;
        let r = value >> 1;
        self.set_zhnc(r == 0, false, false, c != 0);
        r
    }

    /// BIT n, value: Z = !((value >> n) & 1). N=0, H=1. C preserved.
    fn cb_bit(&mut self, n: u8, value: u8) {
        let bit_set = (value >> (n & 7)) & 1 != 0;
        self.regs.set_flag(Flag::Z, !bit_set);
        self.regs.set_flag(Flag::N, false);
        self.regs.set_flag(Flag::H, true);
    }

    fn set_zhnc(&mut self, z: bool, n: bool, h: bool, c: bool) {
        self.regs.set_flag(Flag::Z, z);
        self.regs.set_flag(Flag::N, n);
        self.regs.set_flag(Flag::H, h);
        self.regs.set_flag(Flag::C, c);
    }
}
