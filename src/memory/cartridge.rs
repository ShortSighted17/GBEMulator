// src/memory/cartridge.rs
//
// The cartridge owns the ROM bytes and the external RAM, and knows which
// MBC (memory bank controller) it uses. The MMU routes reads/writes in
// the cartridge's address ranges (0x0000–0x7FFF and 0xA000–0xBFFF) through
// here and doesn't care what mapper variant is underneath.
//
// Bank-state for each mapper lives in a small struct, kept inside the
// `Mapper` enum. The actual ROM/RAM byte vectors live directly on the
// `Cartridge` so we don't fight ownership when matching on the variant.

#[derive(Debug, Clone, Copy)]
pub struct Mbc1State {
    rom_bank_lo: u8,   // lower 5 bits of ROM bank, written via 0x2000–0x3FFF
    bank_hi: u8,       // upper 2 bits — either ROM bank or RAM bank
    mode: u8,          // 0 = ROM banking, 1 = RAM banking
    ram_enabled: bool,
}

impl Mbc1State {
    fn new() -> Self {
        Self { rom_bank_lo: 1, bank_hi: 0, mode: 0, ram_enabled: false }
    }
    fn rom_bank(&self) -> usize {
        let hi = if self.mode == 0 { self.bank_hi } else { 0 };
        (((hi & 0x03) << 5) | (self.rom_bank_lo & 0x1F)) as usize
    }
    fn ram_bank(&self) -> usize {
        if self.mode == 1 { (self.bank_hi & 0x03) as usize } else { 0 }
    }
}

/// MBC3 bank state.
///
/// `bank_select` is the value last written to 0x4000–0x5FFF. Values
/// 0x00–0x03 pick a RAM bank; values 0x08–0x0C pick an RTC register.
/// We store the raw value and interpret it at read/write time so the
/// "is RTC selected right now?" question stays a single byte compare.
#[derive(Debug, Clone, Copy)]
pub struct Mbc3State {
    rom_bank: u8,       // 7-bit, 1..127. 0 silently bumped to 1.
    bank_select: u8,    // last write to 0x4000–0x5FFF
    ram_rtc_enabled: bool,
}

impl Mbc3State {
    fn new() -> Self {
        Self { rom_bank: 1, bank_select: 0, ram_rtc_enabled: false }
    }
    fn rom_bank(&self) -> usize { self.rom_bank as usize }
    fn rtc_selected(&self) -> bool { (0x08..=0x0C).contains(&self.bank_select) }
    fn ram_bank(&self) -> usize { (self.bank_select & 0x03) as usize }
}

#[derive(Debug, Clone, Copy)]
pub enum Mapper {
    None,
    Mbc1(Mbc1State),
    Mbc3(Mbc3State),
}

pub struct Cartridge {
    rom: Vec<u8>,
    ram: Vec<u8>,
    mapper: Mapper,
    pub has_battery: bool,
}

impl Cartridge {
    /// Build a cartridge from raw ROM bytes. Parses the header to pick
    /// the mapper and size the RAM. Unknown mapper types are treated
    /// as plain ROM so the rest of the system can still load something.
    pub fn from_rom(rom: Vec<u8>) -> Self {
        let mbc_type = rom.get(0x0147).copied().unwrap_or(0);
        let ram_size = ram_size_from_header(rom.get(0x0149).copied().unwrap_or(0));

        let (mapper, has_battery) = match mbc_type {
            0x00 => (Mapper::None, false),
            0x01 => (Mapper::Mbc1(Mbc1State::new()), false),
            0x02 => (Mapper::Mbc1(Mbc1State::new()), false),
            0x03 => (Mapper::Mbc1(Mbc1State::new()), true),

            // MBC3 family. 0x0F and 0x10 include the RTC; we treat them
            // as battery-backed (the RTC chip itself is battery-backed
            // on real hardware), but the RTC isn't implemented yet.
            0x0F => (Mapper::Mbc3(Mbc3State::new()), true),  // MBC3+TIMER+BAT
            0x10 => (Mapper::Mbc3(Mbc3State::new()), true),  // MBC3+TIMER+RAM+BAT
            0x11 => (Mapper::Mbc3(Mbc3State::new()), false), // MBC3
            0x12 => (Mapper::Mbc3(Mbc3State::new()), false), // MBC3+RAM
            0x13 => (Mapper::Mbc3(Mbc3State::new()), true),  // MBC3+RAM+BAT

            _ => {
                eprintln!(
                    "warning: unsupported MBC type 0x{:02X}, treating as ROM-only",
                    mbc_type
                );
                (Mapper::None, false)
            }
        };

        Self { rom, ram: vec![0; ram_size], mapper, has_battery }
    }

    pub fn empty() -> Self {
        Self {
            rom: vec![0; 0x8000],
            ram: Vec::new(),
            mapper: Mapper::None,
            has_battery: false,
        }
    }

    // ── Battery-backed save persistence ─────────────────────────────────

    /// Read-only access to external RAM. Used by the save-file writer.
    /// Empty slice for cartridges with no RAM.
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// Replace external RAM contents from a save file. Sizes must match
    /// what the cartridge expects (from header byte 0x0149) — mismatched
    /// sizes are rejected so a stale `.sav` from a different ROM can't
    /// corrupt a fresh session.
    pub fn load_ram(&mut self, data: &[u8]) -> Result<(), String> {
        if data.len() != self.ram.len() {
            return Err(format!(
                "save file size mismatch: file is {} bytes, cartridge expects {}",
                data.len(), self.ram.len()
            ));
        }
        self.ram.copy_from_slice(data);
        Ok(())
    }

    // ── ROM area: 0x0000–0x7FFF ─────────────────────────────────────────

    pub fn read_rom(&self, addr: u16) -> u8 {
        match &self.mapper {
            Mapper::None => *self.rom.get(addr as usize).unwrap_or(&0xFF),

            Mapper::Mbc1(s) => {
                if addr < 0x4000 {
                    *self.rom.get(addr as usize).unwrap_or(&0xFF)
                } else {
                    let bank = s.rom_bank();
                    let off = bank * 0x4000 + (addr as usize - 0x4000);
                    *self.rom.get(off).unwrap_or(&0xFF)
                }
            }

            Mapper::Mbc3(s) => {
                if addr < 0x4000 {
                    *self.rom.get(addr as usize).unwrap_or(&0xFF)
                } else {
                    let bank = s.rom_bank();
                    let off = bank * 0x4000 + (addr as usize - 0x4000);
                    *self.rom.get(off).unwrap_or(&0xFF)
                }
            }
        }
    }

    pub fn write_rom(&mut self, addr: u16, value: u8) {
        match &mut self.mapper {
            Mapper::None => {}

            Mapper::Mbc1(s) => match addr {
                0x0000..=0x1FFF => s.ram_enabled = (value & 0x0F) == 0x0A,
                0x2000..=0x3FFF => {
                    let mut bank = value & 0x1F;
                    if bank == 0 { bank = 1; }
                    s.rom_bank_lo = bank;
                }
                0x4000..=0x5FFF => s.bank_hi = value & 0x03,
                0x6000..=0x7FFF => s.mode = value & 0x01,
                _ => {}
            },

            Mapper::Mbc3(s) => match addr {
                0x0000..=0x1FFF => s.ram_rtc_enabled = (value & 0x0F) == 0x0A,
                0x2000..=0x3FFF => {
                    // 7-bit register. 0 still maps to 1 (only that one
                    // value — unlike MBC1, 0x20/0x40/0x60 are real here).
                    let mut bank = value & 0x7F;
                    if bank == 0 { bank = 1; }
                    s.rom_bank = bank;
                }
                0x4000..=0x5FFF => s.bank_select = value,
                0x6000..=0x7FFF => {
                    // RTC latch (0 → 1 sequence). Not implemented yet;
                    // ignoring writes is the right behaviour for now.
                }
                _ => {}
            },
        }
    }

    // ── External RAM area: 0xA000–0xBFFF ────────────────────────────────

    pub fn read_ram(&self, addr: u16) -> u8 {
        match &self.mapper {
            Mapper::None => 0xFF,

            Mapper::Mbc1(s) => {
                if !s.ram_enabled || self.ram.is_empty() { return 0xFF; }
                let off = s.ram_bank() * 0x2000 + (addr as usize - 0xA000);
                *self.ram.get(off).unwrap_or(&0xFF)
            }

            Mapper::Mbc3(s) => {
                if !s.ram_rtc_enabled { return 0xFF; }
                if s.rtc_selected() {
                    // RTC register read — unimplemented. Real cartridges
                    // return the latched register value; 0x00 is a safe
                    // placeholder. Pokémon Red doesn't read RTC.
                    return 0x00;
                }
                if self.ram.is_empty() { return 0xFF; }
                let off = s.ram_bank() * 0x2000 + (addr as usize - 0xA000);
                *self.ram.get(off).unwrap_or(&0xFF)
            }
        }
    }

    pub fn write_ram(&mut self, addr: u16, value: u8) {
        match &self.mapper {
            Mapper::None => {}

            Mapper::Mbc1(s) => {
                if !s.ram_enabled || self.ram.is_empty() { return; }
                let off = s.ram_bank() * 0x2000 + (addr as usize - 0xA000);
                if let Some(slot) = self.ram.get_mut(off) { *slot = value; }
            }

            Mapper::Mbc3(s) => {
                if !s.ram_rtc_enabled { return; }
                if s.rtc_selected() {
                    // RTC write — unimplemented, silently drop.
                    return;
                }
                if self.ram.is_empty() { return; }
                let off = s.ram_bank() * 0x2000 + (addr as usize - 0xA000);
                if let Some(slot) = self.ram.get_mut(off) { *slot = value; }
            }
        }
    }
}

impl Default for Cartridge {
    fn default() -> Self { Self::empty() }
}

fn ram_size_from_header(code: u8) -> usize {
    match code {
        0x00 => 0,
        0x01 => 0x0800,
        0x02 => 0x2000,
        0x03 => 0x8000,
        0x04 => 0x20000,
        0x05 => 0x10000,
        _    => 0,
    }
}