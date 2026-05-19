// src/lib.rs
//
// Library root. Anything that should be testable, reusable, or callable
// from a future GUI front-end lives here. `main.rs` is just a thin
// wrapper that picks command-line args and calls into this.

#![allow(dead_code)] // remove once the emulator is mostly complete

pub mod cpu;
pub mod memory;
pub mod emulator;

pub mod ppu;
pub mod timer;
pub mod joypad;
pub mod apu;
pub mod audio;