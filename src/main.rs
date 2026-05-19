// src/main.rs

use gb_emulator::audio::AudioBackend;
use gb_emulator::emulator::Emulator;
use gb_emulator::memory::Bus;
use gb_emulator::ppu::{SCREEN_W, SCREEN_H};
use gb_emulator::joypad::{Button, bit as joy_bit};
use minifb::{Key, Window, WindowOptions, Scale};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const PALETTE: [u32; 4] = [0xE0F8D0, 0x88C070, 0x346856, 0x081820];

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <rom.gb> [--trace] [--blargg] [--tail-blargg]", args[0]);
        eprintln!();
        eprintln!("Flags:");
        eprintln!("  --trace        Print gameboy-doctor compatible CPU trace");
        eprintln!("  --blargg       Headless run, exit on Passed/Failed serial output");
        eprintln!("  --tail-blargg  Windowed run, print Blargg test text output to stdout");
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
    let trace        = args.iter().any(|a| a == "--trace");
    let blargg       = args.iter().any(|a| a == "--blargg");
    let tail_blargg  = args.iter().any(|a| a == "--tail-blargg");

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

    let save_path = save_path_for(&rom_path);
    if emu.has_battery() {
        match emu.load_save(&save_path) {
            Ok(true)  => println!("loaded save from {:?}", save_path),
            Ok(false) => println!("no save file at {:?}; starting fresh", save_path),
            Err(e)    => eprintln!("warning: could not load save: {}", e),
        }
    }

    let audio = match AudioBackend::new() {
        Ok(backend) => {
            println!("audio: device opened at {} Hz", backend.sample_rate);
            emu.cpu.bus.apu.set_sample_rate(backend.sample_rate);
            Some(backend)
        }
        Err(e) => {
            eprintln!("audio: disabled ({}); running silently", e);
            None
        }
    };

    run_windowed(&mut emu, audio.as_ref(), tail_blargg);

    if emu.has_battery() {
        match emu.save_to_path(&save_path) {
            Ok(())  => println!("wrote save to {:?}", save_path),
            Err(e)  => eprintln!("warning: could not write save: {}", e),
        }
    }
}

fn save_path_for(rom_path: &Path) -> PathBuf {
    let mut p = rom_path.to_path_buf();
    p.set_extension("sav");
    p
}

fn run_windowed(emu: &mut Emulator, audio: Option<&AudioBackend>, tail_blargg: bool) {
    let mut window = Window::new(
        "gb_emulator",
        SCREEN_W, SCREEN_H,
        WindowOptions { scale: Scale::X4, resize: false, ..WindowOptions::default() },
    ).expect("failed to open window");

    if audio.is_none() {
        window.set_target_fps(60);
    }

    let mut rgb_buf = vec![0u32; SCREEN_W * SCREEN_H];

    // Blargg tail state: how many bytes of the $A004 string have we
    // already printed? Each frame we re-check the string and print
    // any new characters.
    let mut blargg_emitted: usize = 0;

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

        if let Some(audio) = audio {
            drain_audio(emu, audio);
        } else {
            let _ = emu.cpu.bus.apu.take_samples();
        }

        if tail_blargg {
            blargg_emitted = tail_blargg_text(emu, blargg_emitted);
        }
    }
}

/// Poll the Blargg-style text-output region at 0xA004 and print any
/// new characters appended since the last poll. Returns the new
/// "emitted" count.
///
/// Format (per dmg_sound/readme.txt):
///   0xA000      overall status ($80 = running, else final code)
///   0xA001..3   signature bytes $DE $B0 $61
///   0xA004..    zero-terminated text string
fn tail_blargg_text(emu: &mut Emulator, already_emitted: usize) -> usize {
    // Check the signature first; if it isn't there, the test ROM
    // hasn't initialized this region yet.
    let sig0 = emu.cpu.bus.read(0xA001);
    let sig1 = emu.cpu.bus.read(0xA002);
    let sig2 = emu.cpu.bus.read(0xA003);
    if sig0 != 0xDE || sig1 != 0xB0 || sig2 != 0x61 {
        return already_emitted;
    }

    // Find current string length (until NUL, capped at 4 KiB).
    let mut total = 0usize;
    while total < 4096 {
        let b = emu.cpu.bus.read(0xA004 + total as u16);
        if b == 0 { break; }
        total += 1;
    }

    if total > already_emitted {
        use std::io::Write;
        let mut stdout = std::io::stdout();
        for i in already_emitted..total {
            let b = emu.cpu.bus.read(0xA004 + i as u16);
            // Most Blargg text is plain ASCII; print as char.
            stdout.write_all(&[b]).ok();
        }
        stdout.flush().ok();
    }
    total
}

fn drain_audio(emu: &mut Emulator, audio: &AudioBackend) {
    let samples = emu.cpu.bus.apu.take_samples();
    let mut idx = 0;
    let max_wait = Duration::from_millis(50);
    let start = std::time::Instant::now();

    while idx < samples.len() {
        let pushed = audio.try_push(&samples[idx..]);
        idx += pushed;
        if idx < samples.len() {
            if start.elapsed() >= max_wait {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
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