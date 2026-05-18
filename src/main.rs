// src/main.rs

use gb_emulator::emulator::Emulator;
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <rom.gb> [--trace]", args[0]);
        std::process::exit(1);
    }

    let rom_path = &args[1];
    let trace = args.iter().any(|a| a == "--trace");

    let rom = fs::read(rom_path).expect("failed to read ROM");
    let mut emu = Emulator::new(&rom);
    emu.cpu.trace = trace;

    let cycles = emu.run_blargg(500_000_000);

    if !trace {
        println!("\n--- finished after {} cycles ---", cycles);
        emu.print_state();
    }
}
