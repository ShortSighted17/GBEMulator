// src/emulator.rs

use crate::cpu::Cpu;
use crate::memory::Mmu;

pub struct Emulator {
    pub cpu: Cpu<Mmu>,
}

impl Emulator {
    pub fn new(rom: &[u8]) -> Self {
        let mut mmu = Mmu::new();
        mmu.load_rom(rom);
        Self { cpu: Cpu::new(mmu) }
    }

    pub fn step(&mut self) -> u32 {
        let cycles = self.cpu.step();
        self.cpu.bus.tick(cycles);
        cycles
    }

    pub fn run_blargg(&mut self, max_cycles: u64) -> u64 {
        let mut total: u64 = 0;
        while total < max_cycles {
            total += self.step() as u64;
            let buf = &self.cpu.bus.serial_buffer;
            if buf.contains("Passed") || buf.contains("Failed") {
                for _ in 0..2000 { total += self.step() as u64; }
                break;
            }
        }
        total
    }

    pub fn print_state(&self) {
        let r = &self.cpu.regs;
        println!("\nPC = 0x{:04X}", r.pc);
        println!("A  = 0x{:02X}  F = 0x{:02X}", r.a, r.f);
        println!("BC = 0x{:04X}", r.bc());
        println!("DE = 0x{:04X}", r.de());
        println!("HL = 0x{:04X}", r.hl());
        println!("SP = 0x{:04X}", r.sp);
    }
}
