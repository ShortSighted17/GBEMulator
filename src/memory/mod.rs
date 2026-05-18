// src/memory/mod.rs

pub mod cartridge;

use std::io::Write;
use crate::timer::Timer;
use crate::ppu::Ppu;
use crate::joypad::Joypad;

pub trait Bus {
    fn read(&self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, value: u8);

    fn read_word(&self, addr: u16) -> u16 {
        let lo = self.read(addr) as u16;
        let hi = self.read(addr.wrapping_add(1)) as u16;
        (hi << 8) | lo
    }
    fn write_word(&mut self, addr: u16, value: u16) {
        self.write(addr, value as u8);
        self.write(addr.wrapping_add(1), (value >> 8) as u8);
    }
}

pub struct Mmu {
    rom: Vec<u8>,
    rom_bank: u8,
    has_mbc1: bool,

    eram: [u8; 0x2000],
    eram_enabled: bool,

    wram: [u8; 0x2000],
    hram: [u8; 0x7F],

    io: [u8; 0x80],
    pub ie: u8,

    pub timer: Timer,
    pub ppu:   Ppu,
    pub joypad: Joypad,

    dma_source: u8,

    pub serial_buffer: String,
}

impl Mmu {
    pub fn new() -> Self {
        Self {
            rom: vec![0; 0x8000],
            rom_bank: 1,
            has_mbc1: false,
            eram: [0; 0x2000],
            eram_enabled: false,
            wram: [0; 0x2000],
            hram: [0; 0x7F],
            io: [0; 0x80],
            ie: 0,
            timer: Timer::new(),
            ppu:   Ppu::new(),
            joypad: Joypad::new(),
            dma_source: 0,
            serial_buffer: String::new(),
        }
    }

    pub fn load_rom(&mut self, rom: &[u8]) {
        self.rom = rom.to_vec();
        let mbc_type = self.rom.get(0x0147).copied().unwrap_or(0);
        self.has_mbc1 = matches!(mbc_type, 0x01 | 0x02 | 0x03);
        self.rom_bank = 1;
    }

    pub fn tick(&mut self, cycles: u32) {
        self.timer.step(cycles);
        if self.timer.interrupt_request {
            self.timer.interrupt_request = false;
            self.io[0x0F] |= 0x04;
        }

        self.ppu.step(cycles);
        if self.ppu.vblank_irq {
            self.ppu.vblank_irq = false;
            self.io[0x0F] |= 0x01;
        }
        if self.ppu.stat_irq {
            self.ppu.stat_irq = false;
            self.io[0x0F] |= 0x02;
        }

        if self.joypad.interrupt_request {
            self.joypad.interrupt_request = false;
            self.io[0x0F] |= 0x10; // IF bit 4 (joypad)
        }
    }

    fn oam_dma(&mut self, value: u8) {
        self.dma_source = value;
        let base = (value as u16) << 8;
        for i in 0..0xA0u16 {
            let byte = self.read(base + i);
            self.ppu.write_oam(0xFE00 + i, byte);
        }
    }

    fn rom_bank0(&self, addr: u16) -> u8 {
        *self.rom.get(addr as usize).unwrap_or(&0xFF)
    }
    fn rom_bankn(&self, addr: u16) -> u8 {
        let off = (self.rom_bank as usize) * 0x4000 + (addr as usize - 0x4000);
        *self.rom.get(off).unwrap_or(&0xFF)
    }

    fn io_read(&self, addr: u16) -> u8 {
        match addr {
            0xFF00 => self.joypad.read(),
            0xFF04..=0xFF07 => self.timer.read(addr),
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => self.ppu.read_reg(addr),
            0xFF46 => self.dma_source,
            _ => self.io[(addr - 0xFF00) as usize],
        }
    }

    fn io_write(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF00 => self.joypad.write(value),
            0xFF04..=0xFF07 => self.timer.write(addr, value),
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => self.ppu.write_reg(addr, value),
            0xFF46 => self.oam_dma(value),
            0xFF02 => {
                self.io[0x02] = value;
                if value == 0x81 {
                    let ch = self.io[0x01] as char;
                    self.serial_buffer.push(ch);
                    print!("{}", ch);
                    let _ = std::io::stdout().flush();
                    self.io[0x02] = 0x01;
                }
            }
            _ => self.io[(addr - 0xFF00) as usize] = value,
        }
    }
}

impl Default for Mmu { fn default() -> Self { Self::new() } }

impl Bus for Mmu {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => self.rom_bank0(addr),
            0x4000..=0x7FFF => self.rom_bankn(addr),
            0x8000..=0x9FFF => self.ppu.read_vram(addr),
            0xA000..=0xBFFF => if self.eram_enabled { self.eram[(addr - 0xA000) as usize] } else { 0xFF },
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize],
            0xFE00..=0xFE9F => self.ppu.read_oam(addr),
            0xFEA0..=0xFEFF => 0xFF,
            0xFF00..=0xFF7F => self.io_read(addr),
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.ie,
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => if self.has_mbc1 { self.eram_enabled = (value & 0x0F) == 0x0A; },
            0x2000..=0x3FFF => if self.has_mbc1 {
                let mut bank = value & 0x1F;
                if bank == 0 { bank = 1; }
                self.rom_bank = (self.rom_bank & 0x60) | bank;
            },
            0x4000..=0x5FFF => {}
            0x6000..=0x7FFF => {}
            0x8000..=0x9FFF => self.ppu.write_vram(addr, value),
            0xA000..=0xBFFF => if self.eram_enabled { self.eram[(addr - 0xA000) as usize] = value; },
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize] = value,
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize] = value,
            0xFE00..=0xFE9F => self.ppu.write_oam(addr, value),
            0xFEA0..=0xFEFF => {}
            0xFF00..=0xFF7F => self.io_write(addr, value),
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = value,
            0xFFFF => self.ie = value,
        }
    }
}

#[cfg(test)]
pub struct MockBus { pub memory: [u8; 0x10000] }

#[cfg(test)]
impl MockBus {
    pub fn new() -> Self { Self { memory: [0; 0x10000] } }
    pub fn load(&mut self, addr: u16, bytes: &[u8]) {
        let start = addr as usize;
        self.memory[start..start + bytes.len()].copy_from_slice(bytes);
    }
}

#[cfg(test)]
impl Bus for MockBus {
    fn read(&self, addr: u16) -> u8 { self.memory[addr as usize] }
    fn write(&mut self, addr: u16, value: u8) { self.memory[addr as usize] = value; }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oam_dma_copies_160_bytes_from_wram() {
        let mut mmu = Mmu::new();
        for i in 0..0xA0u16 {
            mmu.write(0xC000 + i, (i as u8).wrapping_add(0x42));
        }
        mmu.write(0xFF46, 0xC0);
        for i in 0..0xA0u16 {
            assert_eq!(mmu.read(0xFE00 + i), (i as u8).wrapping_add(0x42));
        }
    }

    #[test]
    fn oam_dma_readback_returns_last_source() {
        let mut mmu = Mmu::new();
        mmu.write(0xFF46, 0xC0);
        assert_eq!(mmu.read(0xFF46), 0xC0);
        mmu.write(0xFF46, 0x80);
        assert_eq!(mmu.read(0xFF46), 0x80);
    }

    #[test]
    fn oam_dma_works_from_rom() {
        let mut rom = vec![0u8; 0x8000];
        for i in 0..0xA0u16 {
            rom[0x0500 + i as usize] = i as u8 ^ 0x55;
        }
        let mut mmu = Mmu::new();
        mmu.load_rom(&rom);
        mmu.write(0xFF46, 0x05);
        for i in 0..0xA0u16 {
            assert_eq!(mmu.read(0xFE00 + i), (i as u8) ^ 0x55);
        }
    }

    #[test]
    fn joypad_routed_through_ff00() {
        let mut mmu = Mmu::new();
        // Select d-pad.
        mmu.write(0xFF00, 0xEF);
        // Reading FF00 with nothing pressed: top bits 1, select retained, low nibble 0xF.
        assert_eq!(mmu.read(0xFF00) & 0x0F, 0x0F);

        // Press Right via the joypad directly, then read again.
        mmu.joypad.set_state(crate::joypad::bit(crate::joypad::Button::Right));
        assert_eq!(mmu.read(0xFF00) & 0x01, 0);
    }
}