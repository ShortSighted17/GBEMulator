// src/cpu/execute.rs

use crate::cpu::Cpu;
use crate::cpu::ImeState;
use crate::cpu::registers::Flag;
use crate::memory::Bus;

impl<B: Bus> Cpu<B> {
    pub(crate) fn execute(&mut self, opcode: u8) -> u32 {
        match opcode {
            0x00 => 4, // NOP
            
            // 0x10 STOP — on DMG, behave as a 2-byte NOP.
            // The byte after 0x10 should be 0x00; we just consume it.
            0x10 => { self.fetch_byte(); 4 }

            // ── LD rr, d16 ──────────────────────────────────────────────
            0x01 => { let v = self.fetch_word(); self.regs.set_bc(v); 12 }
            0x11 => { let v = self.fetch_word(); self.regs.set_de(v); 12 }
            0x21 => { let v = self.fetch_word(); self.regs.set_hl(v); 12 }
            0x31 => { let v = self.fetch_word(); self.regs.sp = v;    12 }

            // ── LD r, d8 ────────────────────────────────────────────────
            0x06 => { let v = self.fetch_byte(); self.regs.b = v; 8 }
            0x0E => { let v = self.fetch_byte(); self.regs.c = v; 8 }
            0x16 => { let v = self.fetch_byte(); self.regs.d = v; 8 }
            0x1E => { let v = self.fetch_byte(); self.regs.e = v; 8 }
            0x26 => { let v = self.fetch_byte(); self.regs.h = v; 8 }
            0x2E => { let v = self.fetch_byte(); self.regs.l = v; 8 }
            0x36 => { let v = self.fetch_byte(); self.bus.write(self.regs.hl(), v); 12 }
            0x3E => { let v = self.fetch_byte(); self.regs.a = v; 8 }

            // ── Indirect loads via BC/DE ────────────────────────────────
            0x02 => { self.bus.write(self.regs.bc(), self.regs.a); 8 }
            0x12 => { self.bus.write(self.regs.de(), self.regs.a); 8 }
            0x0A => { self.regs.a = self.bus.read(self.regs.bc()); 8 }
            0x1A => { self.regs.a = self.bus.read(self.regs.de()); 8 }

            // ── HL+/HL- ─────────────────────────────────────────────────
            0x22 => { let hl = self.regs.hl(); self.bus.write(hl, self.regs.a);
                      self.regs.set_hl(hl.wrapping_add(1)); 8 }
            0x32 => { let hl = self.regs.hl(); self.bus.write(hl, self.regs.a);
                      self.regs.set_hl(hl.wrapping_sub(1)); 8 }
            0x2A => { let hl = self.regs.hl(); self.regs.a = self.bus.read(hl);
                      self.regs.set_hl(hl.wrapping_add(1)); 8 }
            0x3A => { let hl = self.regs.hl(); self.regs.a = self.bus.read(hl);
                      self.regs.set_hl(hl.wrapping_sub(1)); 8 }

            // ── LD A,(a16) / LD (a16),A ─────────────────────────────────
            0xFA => { let a = self.fetch_word(); self.regs.a = self.bus.read(a); 16 }
            0xEA => { let a = self.fetch_word(); self.bus.write(a, self.regs.a); 16 }

            // ── High-RAM page ───────────────────────────────────────────
            0xE0 => { let off = self.fetch_byte() as u16;
                      self.bus.write(0xFF00 + off, self.regs.a); 12 }
            0xF0 => { let off = self.fetch_byte() as u16;
                      self.regs.a = self.bus.read(0xFF00 + off); 12 }
            0xE2 => { let addr = 0xFF00 + self.regs.c as u16;
                      self.bus.write(addr, self.regs.a); 8 }
            0xF2 => { let addr = 0xFF00 + self.regs.c as u16;
                      self.regs.a = self.bus.read(addr); 8 }

            // ── LD (a16),SP ─────────────────────────────────────────────
            0x08 => { let a = self.fetch_word(); self.bus.write_word(a, self.regs.sp); 20 }

            // ── LD r, r' ────────────────────────────────────────────────
            0x40..=0x7F if opcode != 0x76 => self.execute_ld_r_r(opcode),
            0x76 => { self.halted = true; 4 }

            // ── ALU r ───────────────────────────────────────────────────
            0x80..=0xBF => self.execute_alu_r(opcode),

            // ── ALU d8 ──────────────────────────────────────────────────
            0xC6 => { let v = self.fetch_byte(); self.alu_add(v); 8 }
            0xCE => { let v = self.fetch_byte(); self.alu_adc(v); 8 }
            0xD6 => { let v = self.fetch_byte(); self.alu_sub(v); 8 }
            0xDE => { let v = self.fetch_byte(); self.alu_sbc(v); 8 }
            0xE6 => { let v = self.fetch_byte(); self.alu_and(v); 8 }
            0xEE => { let v = self.fetch_byte(); self.alu_xor(v); 8 }
            0xF6 => { let v = self.fetch_byte(); self.alu_or(v);  8 }
            0xFE => { let v = self.fetch_byte(); self.alu_cp(v);  8 }

            // ── INC/DEC 8-bit ───────────────────────────────────────────
            0x04 => { self.regs.b = self.alu_inc8(self.regs.b); 4 }
            0x14 => { self.regs.d = self.alu_inc8(self.regs.d); 4 }
            0x24 => { self.regs.h = self.alu_inc8(self.regs.h); 4 }
            0x34 => { let hl=self.regs.hl(); let v=self.bus.read(hl); let r=self.alu_inc8(v);
                      self.bus.write(hl,r); 12 }
            0x0C => { self.regs.c = self.alu_inc8(self.regs.c); 4 }
            0x1C => { self.regs.e = self.alu_inc8(self.regs.e); 4 }
            0x2C => { self.regs.l = self.alu_inc8(self.regs.l); 4 }
            0x3C => { self.regs.a = self.alu_inc8(self.regs.a); 4 }
            0x05 => { self.regs.b = self.alu_dec8(self.regs.b); 4 }
            0x15 => { self.regs.d = self.alu_dec8(self.regs.d); 4 }
            0x25 => { self.regs.h = self.alu_dec8(self.regs.h); 4 }
            0x35 => { let hl=self.regs.hl(); let v=self.bus.read(hl); let r=self.alu_dec8(v);
                      self.bus.write(hl,r); 12 }
            0x0D => { self.regs.c = self.alu_dec8(self.regs.c); 4 }
            0x1D => { self.regs.e = self.alu_dec8(self.regs.e); 4 }
            0x2D => { self.regs.l = self.alu_dec8(self.regs.l); 4 }
            0x3D => { self.regs.a = self.alu_dec8(self.regs.a); 4 }

            // ── INC/DEC 16-bit ──────────────────────────────────────────
            0x03 => { self.regs.set_bc(self.regs.bc().wrapping_add(1)); 8 }
            0x13 => { self.regs.set_de(self.regs.de().wrapping_add(1)); 8 }
            0x23 => { self.regs.set_hl(self.regs.hl().wrapping_add(1)); 8 }
            0x33 => { self.regs.sp = self.regs.sp.wrapping_add(1); 8 }
            0x0B => { self.regs.set_bc(self.regs.bc().wrapping_sub(1)); 8 }
            0x1B => { self.regs.set_de(self.regs.de().wrapping_sub(1)); 8 }
            0x2B => { self.regs.set_hl(self.regs.hl().wrapping_sub(1)); 8 }
            0x3B => { self.regs.sp = self.regs.sp.wrapping_sub(1); 8 }

            // ── Rotates / accumulator quirks / 16-bit ADD / ADD SP ──────
            0x07 => { self.alu_rlca(); 4 }
            0x0F => { self.alu_rrca(); 4 }
            0x17 => { self.alu_rla();  4 }
            0x1F => { self.alu_rra();  4 }
            0x27 => { self.alu_daa(); 4 }
            0x2F => { self.alu_cpl();  4 }
            0x37 => { self.alu_scf();  4 }
            0x3F => { self.alu_ccf();  4 }

            // ADD HL, rr
            0x09 => { self.alu_add_hl(self.regs.bc()); 8 }
            0x19 => { self.alu_add_hl(self.regs.de()); 8 }
            0x29 => { self.alu_add_hl(self.regs.hl()); 8 }
            0x39 => { self.alu_add_hl(self.regs.sp);   8 }

            // ADD SP, r8
            0xE8 => { let off = self.fetch_byte() as i8;
                      self.regs.sp = self.alu_add_sp_i8(off); 16 }
            // LD HL, SP+r8
            0xF8 => { let off = self.fetch_byte() as i8;
                      let v = self.alu_add_sp_i8(off);
                      self.regs.set_hl(v); 12 }
            // LD SP, HL
            0xF9 => { self.regs.sp = self.regs.hl(); 8 }

            // ── Stack: PUSH / POP ───────────────────────────────────────
            0xC5 => { self.push(self.regs.bc()); 16 }
            0xD5 => { self.push(self.regs.de()); 16 }
            0xE5 => { self.push(self.regs.hl()); 16 }
            0xF5 => { self.push(self.regs.af()); 16 }
            0xC1 => { let v = self.pop(); self.regs.set_bc(v); 12 }
            0xD1 => { let v = self.pop(); self.regs.set_de(v); 12 }
            0xE1 => { let v = self.pop(); self.regs.set_hl(v); 12 }
            0xF1 => { let v = self.pop(); self.regs.set_af(v); 12 }

            // ── JP variants ─────────────────────────────────────────────
            0xC3 => { let addr = self.fetch_word(); self.regs.pc = addr; 16 }
            0xC2 => self.jp_conditional(!self.regs.get_flag(Flag::Z)),
            0xCA => self.jp_conditional( self.regs.get_flag(Flag::Z)),
            0xD2 => self.jp_conditional(!self.regs.get_flag(Flag::C)),
            0xDA => self.jp_conditional( self.regs.get_flag(Flag::C)),
            0xE9 => { self.regs.pc = self.regs.hl(); 4 }

            // ── JR variants ─────────────────────────────────────────────
            0x18 => self.jr_unconditional(),
            0x20 => self.jr_conditional(!self.regs.get_flag(Flag::Z)),
            0x28 => self.jr_conditional( self.regs.get_flag(Flag::Z)),
            0x30 => self.jr_conditional(!self.regs.get_flag(Flag::C)),
            0x38 => self.jr_conditional( self.regs.get_flag(Flag::C)),

            // ── CALL / RET / RETI / RST ─────────────────────────────────
            0xCD => { let a = self.fetch_word(); self.push(self.regs.pc); self.regs.pc = a; 24 }
            0xC4 => self.call_conditional(!self.regs.get_flag(Flag::Z)),
            0xCC => self.call_conditional( self.regs.get_flag(Flag::Z)),
            0xD4 => self.call_conditional(!self.regs.get_flag(Flag::C)),
            0xDC => self.call_conditional( self.regs.get_flag(Flag::C)),

            0xC9 => { self.regs.pc = self.pop(); 16 }
            0xC0 => self.ret_conditional(!self.regs.get_flag(Flag::Z)),
            0xC8 => self.ret_conditional( self.regs.get_flag(Flag::Z)),
            0xD0 => self.ret_conditional(!self.regs.get_flag(Flag::C)),
            0xD8 => self.ret_conditional( self.regs.get_flag(Flag::C)),
            0xD9 => { self.regs.pc = self.pop();
                      self.ime = true;
                      self.ime_state = ImeState::Enabled;
                      16 }

            0xC7 => { self.push(self.regs.pc); self.regs.pc = 0x00; 16 }
            0xCF => { self.push(self.regs.pc); self.regs.pc = 0x08; 16 }
            0xD7 => { self.push(self.regs.pc); self.regs.pc = 0x10; 16 }
            0xDF => { self.push(self.regs.pc); self.regs.pc = 0x18; 16 }
            0xE7 => { self.push(self.regs.pc); self.regs.pc = 0x20; 16 }
            0xEF => { self.push(self.regs.pc); self.regs.pc = 0x28; 16 }
            0xF7 => { self.push(self.regs.pc); self.regs.pc = 0x30; 16 }
            0xFF => { self.push(self.regs.pc); self.regs.pc = 0x38; 16 }

            // ── Interrupt control ───────────────────────────────────────
            0xF3 => { self.ime = false; self.ime_state = ImeState::Disabled; 4 }
            0xFB => {
                // EI: enable IME *after the next instruction*.
                if self.ime_state == ImeState::Disabled {
                    self.ime_state = ImeState::EnablePending;
                }
                4
            }

            // ── CB prefix ───────────────────────────────────────────────
            0xCB => { let cb = self.fetch_byte(); self.execute_cb(cb) }

            _ => panic!(
                "Unimplemented opcode: 0x{:02X} at PC=0x{:04X}",
                opcode, self.regs.pc.wrapping_sub(1)
            ),
        }
    }

    fn read_reg(&self, idx: u8) -> u8 {
        match idx & 0x07 {
            0 => self.regs.b, 1 => self.regs.c,
            2 => self.regs.d, 3 => self.regs.e,
            4 => self.regs.h, 5 => self.regs.l,
            6 => self.bus.read(self.regs.hl()),
            7 => self.regs.a,
            _ => unreachable!(),
        }
    }

    fn write_reg(&mut self, idx: u8, value: u8) {
        match idx & 0x07 {
            0 => self.regs.b = value, 1 => self.regs.c = value,
            2 => self.regs.d = value, 3 => self.regs.e = value,
            4 => self.regs.h = value, 5 => self.regs.l = value,
            6 => self.bus.write(self.regs.hl(), value),
            7 => self.regs.a = value,
            _ => unreachable!(),
        }
    }

    fn execute_ld_r_r(&mut self, opcode: u8) -> u32 {
        let dest = (opcode >> 3) & 0x07;
        let src  = opcode & 0x07;
        let value = self.read_reg(src);
        self.write_reg(dest, value);
        if dest == 6 || src == 6 { 8 } else { 4 }
    }

    fn execute_alu_r(&mut self, opcode: u8) -> u32 {
        let op  = (opcode >> 3) & 0x07;
        let src = opcode & 0x07;
        let v = self.read_reg(src);
        match op {
            0 => self.alu_add(v), 1 => self.alu_adc(v),
            2 => self.alu_sub(v), 3 => self.alu_sbc(v),
            4 => self.alu_and(v), 5 => self.alu_xor(v),
            6 => self.alu_or(v),  7 => self.alu_cp(v),
            _ => unreachable!(),
        }
        if src == 6 { 8 } else { 4 }
    }

    fn jr_unconditional(&mut self) -> u32 {
        let offset = self.fetch_byte() as i8 as i16;
        self.regs.pc = (self.regs.pc as i16).wrapping_add(offset) as u16;
        12
    }

    fn jr_conditional(&mut self, taken: bool) -> u32 {
        let offset = self.fetch_byte() as i8 as i16;
        if taken {
            self.regs.pc = (self.regs.pc as i16).wrapping_add(offset) as u16;
            12
        } else { 8 }
    }

    fn jp_conditional(&mut self, taken: bool) -> u32 {
        let addr = self.fetch_word();
        if taken { self.regs.pc = addr; 16 } else { 12 }
    }

    fn call_conditional(&mut self, taken: bool) -> u32 {
        let addr = self.fetch_word();
        if taken { self.push(self.regs.pc); self.regs.pc = addr; 24 } else { 12 }
    }

    fn ret_conditional(&mut self, taken: bool) -> u32 {
        if taken { self.regs.pc = self.pop(); 20 } else { 8 }
    }

    pub(crate) fn push(&mut self, value: u16) {
        self.regs.sp = self.regs.sp.wrapping_sub(2);
        self.bus.write_word(self.regs.sp, value);
    }

    pub(crate) fn pop(&mut self) -> u16 {
        let v = self.bus.read_word(self.regs.sp);
        self.regs.sp = self.regs.sp.wrapping_add(2);
        v
    }
}
