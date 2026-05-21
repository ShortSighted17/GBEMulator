// src/cpu/registers.rs

const FLAG_Z: u8 = 0b1000_0000;
const FLAG_N: u8 = 0b0100_0000;
const FLAG_H: u8 = 0b0010_0000;
const FLAG_C: u8 = 0b0001_0000;

#[derive(Debug, Default, Clone, Copy)]
pub struct Registers {
    pub a: u8, pub b: u8, pub c: u8, pub d: u8,
    pub e: u8, pub f: u8, pub h: u8, pub l: u8,
    pub sp: u16, pub pc: u16,
}

#[derive(Debug, Clone, Copy)]
pub enum Flag { Z, N, H, C }

impl Registers {
    pub fn new() -> Self {
        // Post-boot DMG state.
        Self {
            a: 0x01, f: 0xB0,
            b: 0x00, c: 0x13,
            d: 0x00, e: 0xD8,
            h: 0x01, l: 0x4D,
            sp: 0xFFFE,
            pc: 0x0100,
        }
    }

    pub fn af(&self) -> u16 { ((self.a as u16) << 8) | (self.f as u16) }
    pub fn bc(&self) -> u16 { ((self.b as u16) << 8) | (self.c as u16) }
    pub fn de(&self) -> u16 { ((self.d as u16) << 8) | (self.e as u16) }
    pub fn hl(&self) -> u16 { ((self.h as u16) << 8) | (self.l as u16) }

    pub fn set_af(&mut self, v: u16) {
        self.a = (v >> 8) as u8;
        self.f = (v as u8) & 0xF0;
    }
    pub fn set_bc(&mut self, v: u16) { 
        self.b = (v >> 8) as u8;
        self.c = v as u8; 
    }
    pub fn set_de(&mut self, v: u16) {
        self.d = (v >> 8) as u8;
        self.e = v as u8;
    }
    pub fn set_hl(&mut self, v: u16) {
        self.h = (v >> 8) as u8;
        self.l = v as u8;
    }

    pub fn get_flag(&self, flag: Flag) -> bool {
        let mask = match flag {
            Flag::Z => FLAG_Z, Flag::N => FLAG_N,
            Flag::H => FLAG_H, Flag::C => FLAG_C,
        };
        self.f & mask != 0
    }

    pub fn set_flag(&mut self, flag: Flag, value: bool) {
        let mask = match flag {
            Flag::Z => FLAG_Z, Flag::N => FLAG_N,
            Flag::H => FLAG_H, Flag::C => FLAG_C,
        };
        if value { self.f |= mask; } else { self.f &= !mask; }
        self.f &= 0xF0;
    }
}
