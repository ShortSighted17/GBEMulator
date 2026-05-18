// src/main.rs

use gb_emulator::emulator::Emulator;
use gb_emulator::ppu::{SCREEN_W, SCREEN_H};
use gb_emulator::joypad::{Button, bit as joy_bit};
use minifb::{Key, Window, WindowOptions, Scale};
use std::env;
use std::fs;

/// DMG green palette, shade 0 (lightest) → shade 3 (darkest), in 0xRRGGBB.
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
        eprintln!("  Esc        → Quit");
        std::process::exit(1);
    }

    let rom_path = &args[1];
    let trace   = args.iter().any(|a| a == "--trace");
    let blargg  = args.iter().any(|a| a == "--blargg");

    let rom = fs::read(rom_path).expect("failed to read ROM");
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

    run_windowed(emu);
}

fn run_windowed(mut emu: Emulator) {
    let mut window = Window::new(
        "gb_emulator",
        SCREEN_W, SCREEN_H,
        WindowOptions { scale: Scale::X4, resize: false, ..WindowOptions::default() },
    ).expect("failed to open window");

    window.set_target_fps(60);

    let mut rgb_buf = vec![0u32; SCREEN_W * SCREEN_H];

    while window.is_open() && !window.is_key_down(Key::Escape) {
        // 1) Sample the keyboard and push to the joypad before stepping
        //    this frame. minifb's edge tracking is implicit in our
        //    Joypad::set_state, which compares against the previous mask.
        let mask = collect_buttons(&window);
        emu.cpu.bus.joypad.set_state(mask);

        // 2) Run one frame.
        emu.run_frame();

        // 3) Blit framebuffer.
        let fb = &emu.cpu.bus.ppu.framebuffer;
        for (i, &shade) in fb.iter().enumerate() {
            rgb_buf[i] = PALETTE[(shade & 0x03) as usize];
        }
        window.update_with_buffer(&rgb_buf, SCREEN_W, SCREEN_H)
              .expect("window update failed");
    }
}

/// Build the joypad "pressed" bitmask from the current keyboard state.
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