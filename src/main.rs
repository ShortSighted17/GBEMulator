// src/main.rs

use gb_emulator::emulator::Emulator;
use gb_emulator::ppu::{SCREEN_W, SCREEN_H};
use gb_emulator::joypad::{Button, bit as joy_bit};
use minifb::{Key, Window, WindowOptions, Scale};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const PALETTE: [u32; 4] = [0xE0F8D0, 0x88C070, 0x346856, 0x081820];

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <rom.gb> [--trace] [--blargg]", args[0]);
        eprintln!();
        eprintln!("Controls when running with a window:");
        eprintln!("  Arrow keys → D-pad");
        eprintln!("  Z          → A");
        eprintln!("  X          → B");
        eprintln!("  Enter      → Start");
        eprintln!("  RShift     → Select");
        eprintln!("  Esc        → Quit (and save, if cart is battery-backed)");
        std::process::exit(1);
    }

    let rom_path = PathBuf::from(&args[1]);
    let trace    = args.iter().any(|a| a == "--trace");
    let blargg   = args.iter().any(|a| a == "--blargg");

    let rom = fs::read(&rom_path).expect("failed to read ROM");
    let mut emu = Emulator::new(&rom);
    emu.cpu.trace = trace;

    if blargg {
        let cycles = emu.run_blargg(500_000_000);
        if !trace {
            println!("\n--- finished after {} cycles ---", cycles);
            emu.print_state();
        }
        return;
    }

    // Derive the .sav path from the ROM path: just swap the extension.
    let save_path = save_path_for(&rom_path);

    // Load any prior save before we start the run loop.
    if emu.has_battery() {
        match emu.load_save(&save_path) {
            Ok(true)  => println!("loaded save from {:?}", save_path),
            Ok(false) => println!("no save file at {:?}; starting fresh", save_path),
            Err(e)    => eprintln!("warning: could not load save: {}", e),
        }
    }

    run_windowed(&mut emu);

    // Run loop has exited cleanly — persist save before we drop the emulator.
    if emu.has_battery() {
        match emu.save_to_path(&save_path) {
            Ok(())  => println!("wrote save to {:?}", save_path),
            Err(e)  => eprintln!("warning: could not write save: {}", e),
        }
    }
}

/// Replace (or append) the ROM path's extension with `.sav`.
/// `roms/pokemon/pokemon_red.gb` → `roms/pokemon/pokemon_red.sav`.
fn save_path_for(rom_path: &Path) -> PathBuf {
    let mut p = rom_path.to_path_buf();
    p.set_extension("sav");
    p
}

fn run_windowed(emu: &mut Emulator) {
    let mut window = Window::new(
        "gb_emulator",
        SCREEN_W, SCREEN_H,
        WindowOptions { scale: Scale::X4, resize: false, ..WindowOptions::default() },
    ).expect("failed to open window");

    window.set_target_fps(60);

    let mut rgb_buf = vec![0u32; SCREEN_W * SCREEN_H];

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let mask = collect_buttons(&window);
        emu.cpu.bus.joypad.set_state(mask);

        emu.run_frame();

        let fb = &emu.cpu.bus.ppu.framebuffer;
        for (i, &shade) in fb.iter().enumerate() {
            rgb_buf[i] = PALETTE[(shade & 0x03) as usize];
        }
        window.update_with_buffer(&rgb_buf, SCREEN_W, SCREEN_H)
              .expect("window update failed");
    }
}

fn collect_buttons(window: &Window) -> u8 {
    let mut mask = 0u8;
    if window.is_key_down(Key::Right)      { mask |= joy_bit(Button::Right);  }
    if window.is_key_down(Key::Left)       { mask |= joy_bit(Button::Left);   }
    if window.is_key_down(Key::Up)         { mask |= joy_bit(Button::Up);     }
    if window.is_key_down(Key::Down)       { mask |= joy_bit(Button::Down);   }
    if window.is_key_down(Key::Z)          { mask |= joy_bit(Button::A);      }
    if window.is_key_down(Key::X)          { mask |= joy_bit(Button::B);      }
    if window.is_key_down(Key::Enter)      { mask |= joy_bit(Button::Start);  }
    if window.is_key_down(Key::RightShift) { mask |= joy_bit(Button::Select); }
    mask
}