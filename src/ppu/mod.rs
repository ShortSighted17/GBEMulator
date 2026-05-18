// src/ppu/mod.rs
//
// The DMG PPU.
//
// Timing model — one scanline is 456 T-cycles ("dots"), one frame is 154
// scanlines (= 70224 dots). Within each visible scanline (LY 0..=143) the
// PPU walks three modes:
//
//   Mode 2  OAM Scan   dots   0.. 79      (80 dots)
//   Mode 3  Drawing    dots  80..251      (172 dots, nominal)
//   Mode 0  HBlank     dots 252..455      (204 dots, nominal)
//
// Lines 144..=153 are entirely Mode 1 (VBlank). VBlank fires once per
// frame at the moment LY transitions 143 -> 144.
//
// Rendering model — at the moment we enter HBlank on a visible scanline,
// we render that entire scanline at once (BG, then window, then sprites).
//
// STAT interrupt — four sources are ORed into a single STAT IRQ line,
// and the interrupt fires on the rising edge of that line.

#[cfg(test)]
mod tests;

pub const SCREEN_W: usize = 160;
pub const SCREEN_H: usize = 144;

const DOTS_PER_LINE:    u32 = 456;
const LINES_PER_FRAME:  u32 = 154;
const VISIBLE_LINES:    u8  = 144;

const OAM_END:      u32 = 80;
const DRAWING_END:  u32 = 80 + 172;

// LCDC bits.
const LCDC_BG_ENABLE:        u8 = 1 << 0;
const LCDC_OBJ_ENABLE:       u8 = 1 << 1;
const LCDC_OBJ_SIZE:         u8 = 1 << 2; // 0 = 8x8, 1 = 8x16
const LCDC_BG_MAP_AREA:      u8 = 1 << 3;
const LCDC_BG_DATA_AREA:     u8 = 1 << 4;
const LCDC_WINDOW_ENABLE:    u8 = 1 << 5;
const LCDC_WINDOW_MAP_AREA:  u8 = 1 << 6;
const LCDC_LCD_ENABLE:       u8 = 1 << 7;

// OAM attribute bits.
const OAM_ATTR_PRIORITY:   u8 = 1 << 7; // 1 = sprite behind BG colors 1..3
const OAM_ATTR_Y_FLIP:     u8 = 1 << 6;
const OAM_ATTR_X_FLIP:     u8 = 1 << 5;
const OAM_ATTR_PALETTE:    u8 = 1 << 4; // 0 = OBP0, 1 = OBP1

// STAT interrupt-source-enable bits.
const STAT_LYC_IE:    u8 = 1 << 6;
const STAT_MODE2_IE:  u8 = 1 << 5;
const STAT_MODE1_IE:  u8 = 1 << 4;
const STAT_MODE0_IE:  u8 = 1 << 3;

const MAX_SPRITES_PER_LINE: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpuMode {
    HBlank  = 0,
    VBlank  = 1,
    OamScan = 2,
    Drawing = 3,
}

/// One sprite selected for the current scanline.
#[derive(Clone, Copy)]
struct SpriteHit {
    oam_index: u8, // 0..40
    x:         u8, // OAM x byte (screen_x + 8)
    y:         u8, // OAM y byte (screen_y + 16)
    tile:      u8,
    attr:      u8,
}

pub struct Ppu {
    vram: [u8; 0x2000],
    oam:  [u8; 0xA0],

    pub lcdc: u8, pub stat: u8,
    pub scy:  u8, pub scx:  u8,
    pub ly:   u8, pub lyc:  u8,
    pub bgp:  u8, pub obp0: u8, pub obp1: u8,
    pub wy:   u8, pub wx:   u8,

    mode: PpuMode,
    dots: u32,
    wlc: u8,

    stat_line_prev: bool,

    pub framebuffer: [u8; SCREEN_W * SCREEN_H],

    /// Per-pixel BG/window *color index* (pre-BGP, 0..=3) for the current
    /// scanline. Sprite rendering uses this to evaluate the BG-priority
    /// attribute — "color 0" there means the raw color id, not the
    /// post-palette shade. Scratch buffer, no value between scanlines.
    bg_color_line: [u8; SCREEN_W],

    pub vblank_irq: bool,
    pub stat_irq:   bool,
    pub frame_ready: bool,
}

impl Default for Ppu { fn default() -> Self { Self::new() } }

impl Ppu {
    pub fn new() -> Self {
        Self {
            vram: [0; 0x2000],
            oam:  [0; 0xA0],
            lcdc: 0x91, stat: 0x85,
            scy: 0, scx: 0, ly: 0, lyc: 0,
            bgp: 0xFC, obp0: 0xFF, obp1: 0xFF,
            wy: 0, wx: 0,
            mode: PpuMode::OamScan, dots: 0, wlc: 0,
            stat_line_prev: false,
            framebuffer: [0; SCREEN_W * SCREEN_H],
            bg_color_line: [0; SCREEN_W],
            vblank_irq: false, stat_irq: false, frame_ready: false,
        }
    }

    // ── Memory access ───────────────────────────────────────────────────

    pub fn read_vram (&self, addr: u16) -> u8 { self.vram[(addr - 0x8000) as usize] }
    pub fn write_vram(&mut self, addr: u16, v: u8) { self.vram[(addr - 0x8000) as usize] = v; }
    pub fn read_oam  (&self, addr: u16) -> u8 { self.oam [(addr - 0xFE00) as usize] }
    pub fn write_oam (&mut self, addr: u16, v: u8) { self.oam [(addr - 0xFE00) as usize] = v; }

    pub fn read_reg(&self, addr: u16) -> u8 {
        match addr {
            0xFF40 => self.lcdc,
            0xFF41 => 0x80 | (self.stat & 0x78) | self.mode_bits() | self.lyc_match_bit(),
            0xFF42 => self.scy,  0xFF43 => self.scx,
            0xFF44 => self.ly,   0xFF45 => self.lyc,
            0xFF47 => self.bgp,  0xFF48 => self.obp0, 0xFF49 => self.obp1,
            0xFF4A => self.wy,   0xFF4B => self.wx,
            _ => 0xFF,
        }
    }

    pub fn write_reg(&mut self, addr: u16, value: u8) {
        match addr {
            0xFF40 => {
                let was_on  = self.lcdc & LCDC_LCD_ENABLE != 0;
                let will_on = value     & LCDC_LCD_ENABLE != 0;
                self.lcdc = value;
                if was_on && !will_on {
                    self.ly = 0; self.dots = 0; self.mode = PpuMode::HBlank;
                    self.wlc = 0; self.stat_line_prev = false;
                }
                if !was_on && will_on {
                    self.ly = 0; self.dots = 0; self.mode = PpuMode::OamScan;
                    self.wlc = 0; self.stat_line_prev = false;
                }
            }
            0xFF41 => {
                self.stat = (self.stat & 0x07) | (value & 0x78);
                self.update_stat_line();
            }
            0xFF42 => self.scy = value,  0xFF43 => self.scx = value,
            0xFF44 => {}
            0xFF45 => { self.lyc = value; self.update_stat_line(); }
            0xFF47 => self.bgp = value,  0xFF48 => self.obp0 = value,
            0xFF49 => self.obp1 = value,
            0xFF4A => self.wy = value,   0xFF4B => self.wx = value,
            _ => {}
        }
    }

    // ── State machine ───────────────────────────────────────────────────

    pub fn step(&mut self, cycles: u32) {
        if self.lcdc & LCDC_LCD_ENABLE == 0 { return; }

        // Walk the cycle budget in chunks bounded by the next interesting
        // event (mode transition, line wrap). This guarantees we visit
        // every HBlank-entry to render that line — even when a single
        // call to step() spans many scanlines, as in unit tests using
        // step(70224). Real CPU step sizes are small so most iterations
        // exit on the first pass.
        let mut remaining = cycles;
        while remaining > 0 {
            // How many dots until the next event on this scanline.
            let next_boundary = if self.ly >= VISIBLE_LINES {
                // VBlank: only event is line end (then maybe frame end).
                DOTS_PER_LINE
            } else {
                match self.mode {
                    PpuMode::OamScan => OAM_END,
                    PpuMode::Drawing => DRAWING_END,
                    PpuMode::HBlank  => DOTS_PER_LINE,
                    PpuMode::VBlank  => DOTS_PER_LINE, // shouldn't happen here
                }
            };
            let chunk = (next_boundary - self.dots).min(remaining);
            self.dots += chunk;
            remaining -= chunk;

            // Did we hit a boundary?
            if self.dots == DOTS_PER_LINE {
                // End of line → advance.
                self.dots = 0;
                self.advance_line();
            } else if self.ly < VISIBLE_LINES {
                // Mode transition within this scanline.
                let new_mode = if self.dots < OAM_END {
                    PpuMode::OamScan
                } else if self.dots < DRAWING_END {
                    PpuMode::Drawing
                } else {
                    // Entering HBlank — this is when we render the line.
                    if self.mode != PpuMode::HBlank {
                        self.render_scanline();
                    }
                    PpuMode::HBlank
                };
                if new_mode != self.mode {
                    self.mode = new_mode;
                    self.update_stat_line();
                }
            }
        }
    }

    fn advance_line(&mut self) {
        self.ly = self.ly.wrapping_add(1);
        if self.ly as u32 >= LINES_PER_FRAME {
            self.ly = 0; self.wlc = 0;
        }

        if self.ly == VISIBLE_LINES {
            self.mode = PpuMode::VBlank;
            self.vblank_irq = true;
            self.frame_ready = true;
        } else if self.ly < VISIBLE_LINES {
            self.mode = PpuMode::OamScan;
        }
        self.update_stat_line();
    }

    fn mode_bits(&self) -> u8 { self.mode as u8 }
    fn lyc_match_bit(&self) -> u8 { if self.ly == self.lyc { 0x04 } else { 0x00 } }

    fn update_stat_line(&mut self) {
        let lyc_match = self.ly == self.lyc;
        let line =
            (self.stat & STAT_LYC_IE   != 0 && lyc_match) ||
            (self.stat & STAT_MODE2_IE != 0 && self.mode == PpuMode::OamScan) ||
            (self.stat & STAT_MODE1_IE != 0 && self.mode == PpuMode::VBlank)  ||
            (self.stat & STAT_MODE0_IE != 0 && self.mode == PpuMode::HBlank);
        if line && !self.stat_line_prev { self.stat_irq = true; }
        self.stat_line_prev = line;
    }

    #[cfg(test)] pub fn debug_mode(&self) -> PpuMode { self.mode }
    #[cfg(test)] pub fn debug_wlc(&self) -> u8 { self.wlc }

    // ── Rendering ───────────────────────────────────────────────────────

    fn render_scanline(&mut self) {
        if self.lcdc & LCDC_BG_ENABLE == 0 {
            let start = self.ly as usize * SCREEN_W;
            for px in &mut self.framebuffer[start..start + SCREEN_W] {
                *px = 0;
            }
            // BG-disabled → effective color id is 0 for the whole line.
            for c in &mut self.bg_color_line { *c = 0; }
        } else {
            self.render_bg_line();
        }

        // On DMG, when LCDC bit 0 is 0, the window is also suppressed.
        if self.lcdc & LCDC_BG_ENABLE != 0 {
            self.render_window_line();
        }

        if self.lcdc & LCDC_OBJ_ENABLE != 0 {
            self.render_sprites_line();
        }
    }

    fn render_bg_line(&mut self) {
        let ly = self.ly;
        let bg_y = ly.wrapping_add(self.scy);
        let tile_row = (bg_y / 8) as u16;
        let row_in_tile = (bg_y % 8) as u16;

        let map_base: u16 = if self.lcdc & LCDC_BG_MAP_AREA != 0 { 0x9C00 } else { 0x9800 };

        for screen_x in 0..SCREEN_W {
            let bg_x = (screen_x as u8).wrapping_add(self.scx);
            let tile_col = (bg_x / 8) as u16;
            let col_in_tile = bg_x % 8;

            let map_addr = map_base + tile_row * 32 + tile_col;
            let tile_index = self.read_vram(map_addr);
            let tile_addr = self.tile_data_addr(tile_index);

            let byte0 = self.read_vram(tile_addr + row_in_tile * 2);
            let byte1 = self.read_vram(tile_addr + row_in_tile * 2 + 1);

            let bit = 7 - col_in_tile;
            let low  = (byte0 >> bit) & 1;
            let high = (byte1 >> bit) & 1;
            let color_id = (high << 1) | low;

            let shade = (self.bgp >> (color_id * 2)) & 0x03;
            self.framebuffer[ly as usize * SCREEN_W + screen_x] = shade;
            self.bg_color_line[screen_x] = color_id;
        }
    }

    fn render_window_line(&mut self) {
        if self.lcdc & LCDC_WINDOW_ENABLE == 0 { return; }
        if self.ly < self.wy { return; }
        if self.wx >= (SCREEN_W as u8) + 7 { return; }

        let map_base: u16 = if self.lcdc & LCDC_WINDOW_MAP_AREA != 0 { 0x9C00 } else { 0x9800 };
        let win_y = self.wlc;
        let tile_row = (win_y / 8) as u16;
        let row_in_tile = (win_y % 8) as u16;
        let start_x: i32 = self.wx as i32 - 7;

        let mut drew_any = false;
        for screen_x in 0..SCREEN_W {
            let win_x = screen_x as i32 - start_x;
            if win_x < 0 { continue; }
            let win_x = win_x as u8;

            let tile_col = (win_x / 8) as u16;
            let col_in_tile = win_x % 8;

            let map_addr = map_base + tile_row * 32 + tile_col;
            let tile_index = self.read_vram(map_addr);
            let tile_addr = self.tile_data_addr(tile_index);

            let byte0 = self.read_vram(tile_addr + row_in_tile * 2);
            let byte1 = self.read_vram(tile_addr + row_in_tile * 2 + 1);

            let bit = 7 - col_in_tile;
            let low  = (byte0 >> bit) & 1;
            let high = (byte1 >> bit) & 1;
            let color_id = (high << 1) | low;

            let shade = (self.bgp >> (color_id * 2)) & 0x03;
            self.framebuffer[self.ly as usize * SCREEN_W + screen_x] = shade;
            self.bg_color_line[screen_x] = color_id;
            drew_any = true;
        }

        if drew_any { self.wlc = self.wlc.wrapping_add(1); }
    }

    /// Render sprites for the current scanline.
    ///
    /// Algorithm:
    ///   1. OAM scan: collect up to 10 sprites whose Y range covers LY.
    ///      OAM-order is preserved as the secondary sort key.
    ///   2. Sort by X ascending; ties keep OAM order (stable sort).
    ///   3. Draw pixels right-to-left in the sorted order, so that lower-X
    ///      sprites end up on top. Skip color-0 pixels (transparent).
    ///      Honour the BG-priority attribute against bg_color_line.
    fn render_sprites_line(&mut self) {
        let sprite_h: u8 = if self.lcdc & LCDC_OBJ_SIZE != 0 { 16 } else { 8 };

        // Step 1 — OAM scan.
        let mut hits: [SpriteHit; MAX_SPRITES_PER_LINE] = [SpriteHit {
            oam_index: 0, x: 0, y: 0, tile: 0, attr: 0,
        }; MAX_SPRITES_PER_LINE];
        let mut count = 0usize;

        for i in 0..40u8 {
            let base = i as usize * 4;
            let y = self.oam[base];
            // Sprite covers screen lines (y - 16)..(y - 16 + sprite_h).
            // Equivalently: LY is in range iff LY + 16 is in [y, y + sprite_h).
            let ly_plus_16 = self.ly as i16 + 16;
            let top = y as i16;
            let bot = top + sprite_h as i16;
            if ly_plus_16 >= top && ly_plus_16 < bot {
                if count < MAX_SPRITES_PER_LINE {
                    hits[count] = SpriteHit {
                        oam_index: i,
                        x:    self.oam[base + 1],
                        y,
                        tile: self.oam[base + 2],
                        attr: self.oam[base + 3],
                    };
                    count += 1;
                }
                // If we already have 10, additional sprites are silently
                // dropped — they don't bump anyone out.
            }
        }

        if count == 0 { return; }

        // Step 2 — stable sort by X ascending. Bubble sort is fine for n≤10.
        // Stable: equal X keeps OAM order (which is already the input order).
        for a in 0..count {
            for b in 0..count - a - 1 {
                if hits[b].x > hits[b + 1].x {
                    hits.swap(b, b + 1);
                }
            }
        }

        // Step 3 — draw, lowest priority first (highest X), so lower-X
        // sprites overwrite. Already-written pixels track the "owner" via
        // a small bookkeeping array so a later sprite (higher X, lower
        // priority) can't overdraw a lower-X sprite at the same column.
        //
        // Simpler equivalent: iterate sprites lowest priority → highest
        // priority (right-to-left in our sort), and let later (higher
        // priority) sprites overwrite. That naturally gives correct
        // visual layering without the "owner" bookkeeping.
        for hit_i in (0..count).rev() {
            let hit = hits[hit_i];
            let sprite_screen_x = hit.x as i16 - 8;
            let sprite_screen_y = hit.y as i16 - 16;
            let mut row = (self.ly as i16 - sprite_screen_y) as u8; // 0..sprite_h

            if hit.attr & OAM_ATTR_Y_FLIP != 0 {
                row = sprite_h - 1 - row;
            }

            // In 8x16 mode, the low bit of the tile index is ignored.
            let tile_index = if sprite_h == 16 { hit.tile & 0xFE } else { hit.tile };

            // Resolve the 16-byte tile block. Sprites always use the
            // 0x8000-base unsigned addressing — never the signed mode.
            // In 8x16 mode, rows 8..15 spill naturally into the next
            // tile because (tile << 4) + row*2 advances accordingly.
            let tile_addr = 0x8000u16 + (tile_index as u16) * 16 + (row as u16) * 2;
            let byte0 = self.read_vram(tile_addr);
            let byte1 = self.read_vram(tile_addr + 1);

            let palette = if hit.attr & OAM_ATTR_PALETTE != 0 { self.obp1 } else { self.obp0 };

            for col in 0..8u8 {
                let screen_x = sprite_screen_x + col as i16;
                if screen_x < 0 || screen_x >= SCREEN_W as i16 { continue; }

                let bit_col = if hit.attr & OAM_ATTR_X_FLIP != 0 { col } else { 7 - col };
                let low  = (byte0 >> bit_col) & 1;
                let high = (byte1 >> bit_col) & 1;
                let color_id = (high << 1) | low;
                if color_id == 0 { continue; } // transparent

                // BG priority: sprite hidden when BG color id is non-zero.
                if hit.attr & OAM_ATTR_PRIORITY != 0
                    && self.bg_color_line[screen_x as usize] != 0
                {
                    continue;
                }

                let shade = (palette >> (color_id * 2)) & 0x03;
                self.framebuffer[self.ly as usize * SCREEN_W + screen_x as usize] = shade;
            }
        }
    }

    fn tile_data_addr(&self, tile_index: u8) -> u16 {
        if self.lcdc & LCDC_BG_DATA_AREA != 0 {
            0x8000 + (tile_index as u16) * 16
        } else {
            let signed = tile_index as i8 as i16;
            (0x9000_i32 + signed as i32 * 16) as u16
        }
    }
}