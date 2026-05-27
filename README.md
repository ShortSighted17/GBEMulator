# Game Boy Emulator

A Gameboy emulator written in Rust as a learning project.

## Running

you no longer need Rust toolchain, though i'd leave the instructions for the tech savvy. all you need is the executable, just drag a *.gb file onto it.

for the Rust toolchain: ([rustup.rs](https://rustup.rs)) and a Game Boy ROM (`.gb` file).
There are some examples i tested with included in the roms directory.

```sh
cargo run --release -- path/to/rom.gb
```

## Controls

| Key       | Game Boy |
| --------- | -------- |
| Arrows    | D-pad    |
| Z         | A        |
| X         | B        |
| Enter     | Start    |
| RShift    | Select   |

## Saves

For cartridges with battery-backed RAM, the emulator writes a
`<rom>.sav` file next to the ROM on clean exit, and reloads it on startup. The format
is a raw cartridge-RAM dump, interoperable with BGB / SameBoy / Gambatte.

## Tested with

Anything from the roms directory. Some of the Blargg test ROMs still fail, but wont interrupt with gameplay for what's included.
