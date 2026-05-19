// src/emulator.rs

use std::fs;
use std::path::Path;

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

    /// One CPU step + lockstep tick of every subsystem.
    pub fn step(&mut self) -> u32 {
        let cycles = self.cpu.step();
        self.cpu.bus.tick(cycles);
        cycles
    }

    /// Run until the PPU signals a frame is ready (i.e. it just entered
    /// VBlank). Clears the flag and returns the number of T-cycles spent.
    /// Used by the windowed front-end.
    pub fn run_frame(&mut self) -> u64 {
        let mut total: u64 = 0;
        const SAFETY_LIMIT: u64 = 200_000;
        while total < SAFETY_LIMIT {
            total += self.step() as u64;
            if self.cpu.bus.ppu.frame_ready {
                self.cpu.bus.ppu.frame_ready = false;
                return total;
            }
        }
        total
    }

    /// Headless run used by Blargg-style serial test ROMs.
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

    // ── Battery-backed save persistence ─────────────────────────────────

    /// True if the inserted cartridge declares itself battery-backed.
    /// Used by the front-end to decide whether to bother touching `.sav`.
    pub fn has_battery(&self) -> bool {
        self.cpu.bus.cart.has_battery
    }

    /// Try to load `path` as a save file into the cartridge's external RAM.
    /// Returns Ok(true) if loaded, Ok(false) if the file doesn't exist,
    /// and Err if it exists but couldn't be used (wrong size, IO error).
    /// A fresh game with no prior save is the Ok(false) case.
    pub fn load_save<P: AsRef<Path>>(&mut self, path: P) -> Result<bool, String> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(false);
        }
        let data = fs::read(path).map_err(|e| format!("reading {:?}: {}", path, e))?;
        self.cpu.bus.cart.load_ram(&data)?;
        Ok(true)
    }

    /// Write cartridge external RAM out to `path`. Skips silently if the
    /// cartridge has nothing to save (no battery, or empty RAM array).
    pub fn save_to_path<P: AsRef<Path>>(&self, path: P) -> Result<(), String> {
        if !self.has_battery() {
            return Ok(());
        }
        let ram = self.cpu.bus.cart.ram();
        if ram.is_empty() {
            // MBC3+TIMER+BAT (0x0F) has battery but no RAM — only RTC,
            // which we don't persist yet. Nothing to write.
            return Ok(());
        }
        let path = path.as_ref();
        fs::write(path, ram).map_err(|e| format!("writing {:?}: {}", path, e))
    }
}