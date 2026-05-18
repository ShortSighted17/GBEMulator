// src/ppu/tests.rs

use crate::ppu::{Ppu, PpuMode, SCREEN_W};

fn fresh_on() -> Ppu {
    let mut p = Ppu::new();
    p.lcdc = 0x91;
    p
}

#[test]
fn ly_increments_each_scanline() {
    let mut p = fresh_on();
    p.step(456);
    assert_eq!(p.ly, 1);
    p.step(456);
    assert_eq!(p.ly, 2);
}

#[test]
fn ly_wraps_after_153() {
    let mut p = fresh_on();
    p.step(70224);
    assert_eq!(p.ly, 0);
}

#[test]
fn vblank_irq_at_line_144() {
    let mut p = fresh_on();
    p.step(144 * 456);
    assert_eq!(p.ly, 144);
    assert_eq!(p.debug_mode(), PpuMode::VBlank);
    assert!(p.vblank_irq);
}

#[test]
fn vblank_lasts_ten_lines() {
    let mut p = fresh_on();
    p.step(144 * 456);
    p.vblank_irq = false;
    p.step(9 * 456);
    assert_eq!(p.ly, 153);
    assert_eq!(p.debug_mode(), PpuMode::VBlank);
    assert!(!p.vblank_irq);
    p.step(456);
    assert_eq!(p.ly, 0);
    assert_eq!(p.debug_mode(), PpuMode::OamScan);
}

#[test]
fn mode_progression_within_visible_line() {
    let mut p = fresh_on();
    assert_eq!(p.debug_mode(), PpuMode::OamScan);
    p.step(80);
    assert_eq!(p.debug_mode(), PpuMode::Drawing);
    p.step(172);
    assert_eq!(p.debug_mode(), PpuMode::HBlank);
}

#[test]
fn lcd_off_freezes_state() {
    let mut p = fresh_on();
    p.write_reg(0xFF40, 0x00);
    assert_eq!(p.ly, 0);
    p.step(70224);
    assert_eq!(p.ly, 0);
    assert!(!p.vblank_irq);
}

#[test]
fn frame_ready_flips_at_vblank_entry() {
    let mut p = fresh_on();
    p.step(144 * 456 - 1);
    assert!(!p.frame_ready);
    p.step(1);
    assert!(p.frame_ready);
    assert!(p.vblank_irq);
}

#[test]
fn vram_round_trip() {
    let mut p = Ppu::new();
    p.write_vram(0x8000, 0xAB);
    p.write_vram(0x9FFF, 0xCD);
    assert_eq!(p.read_vram(0x8000), 0xAB);
    assert_eq!(p.read_vram(0x9FFF), 0xCD);
}

#[test]
fn stat_reports_current_mode_in_low_bits() {
    let mut p = fresh_on();
    assert_eq!(p.read_reg(0xFF41) & 0x03, 2);
    p.step(80);
    assert_eq!(p.read_reg(0xFF41) & 0x03, 3);
}

#[test]
fn lyc_match_bit_in_stat() {
    let mut p = fresh_on();
    p.lyc = 0;
    assert!(p.read_reg(0xFF41) & 0x04 != 0);
    p.lyc = 5;
    assert!(p.read_reg(0xFF41) & 0x04 == 0);
}

#[test]
fn stat_writes_dont_clobber_mode_bits() {
    let mut p = fresh_on();
    p.write_reg(0xFF41, 0xFF);
    assert_eq!(p.read_reg(0xFF41) & 0x03, p.debug_mode() as u8);
}

#[test]
fn ly_is_read_only() {
    let mut p = fresh_on();
    p.step(456);
    p.write_reg(0xFF44, 99);
    assert_eq!(p.ly, 1);
}

#[test]
fn wlc_resets_each_frame() {
    let mut p = fresh_on();
    p.write_reg(0xFF40, 0x91 | 0x20);
    p.write_reg(0xFF4A, 0);
    p.write_reg(0xFF4B, 7);
    p.step(70224);
    assert_eq!(p.debug_wlc(), 0);
}

#[test]
fn wlc_does_not_advance_when_window_off() {
    let mut p = fresh_on();
    p.write_reg(0xFF40, 0x91);
    p.write_reg(0xFF4A, 0);
    p.write_reg(0xFF4B, 7);
    p.step(144 * 456);
    assert_eq!(p.debug_wlc(), 0);
}

#[test]
fn wlc_does_not_advance_before_wy() {
    let mut p = fresh_on();
    p.write_reg(0xFF40, 0x91 | 0x20);
    p.write_reg(0xFF4A, 100);
    p.write_reg(0xFF4B, 7);
    p.step(50 * 456);
    assert_eq!(p.debug_wlc(), 0);
}

#[test]
fn stat_lyc_match_fires_interrupt() {
    let mut p = fresh_on();
    p.write_reg(0xFF41, 0x40);
    p.write_reg(0xFF45, 5);
    p.stat_irq = false;
    p.step(5 * 456);
    assert_eq!(p.ly, 5);
    assert!(p.stat_irq);
}

#[test]
fn stat_lyc_no_interrupt_if_source_disabled() {
    let mut p = fresh_on();
    p.write_reg(0xFF41, 0x00);
    p.write_reg(0xFF45, 5);
    p.stat_irq = false;
    p.step(5 * 456);
    assert!(!p.stat_irq);
}

#[test]
fn stat_mode0_hblank_fires_interrupt() {
    let mut p = fresh_on();
    p.write_reg(0xFF41, 0x08);
    p.stat_irq = false;
    p.step(252);
    assert_eq!(p.debug_mode(), PpuMode::HBlank);
    assert!(p.stat_irq);
}

#[test]
fn stat_mode2_oam_fires_interrupt() {
    let mut p = fresh_on();
    p.step(80);
    assert_eq!(p.debug_mode(), PpuMode::Drawing);
    p.write_reg(0xFF41, 0x20);
    p.stat_irq = false;
    p.step(376);
    assert_eq!(p.ly, 1);
    assert_eq!(p.debug_mode(), PpuMode::OamScan);
    assert!(p.stat_irq);
}

#[test]
fn stat_does_not_refire_within_same_mode() {
    let mut p = fresh_on();
    p.write_reg(0xFF41, 0x08);
    p.stat_irq = false;
    p.step(252);
    assert!(p.stat_irq);
    p.stat_irq = false;
    p.step(100);
    assert_eq!(p.debug_mode(), PpuMode::HBlank);
    assert!(!p.stat_irq);
}

#[test]
fn stat_line_resets_so_next_hblank_fires() {
    let mut p = fresh_on();
    p.write_reg(0xFF41, 0x08);
    p.stat_irq = false;
    p.step(252);
    assert!(p.stat_irq);
    p.stat_irq = false;
    p.step(456);
    assert_eq!(p.debug_mode(), PpuMode::HBlank);
    assert!(p.stat_irq);
}

// ── Drop 3.4: sprite tests ──────────────────────────────────────────────

/// Put a solid 8x8 tile at VRAM tile index `idx` (using 0x8000-mode).
/// The tile uses color id 3 for every pixel.
fn put_solid_tile(p: &mut Ppu, idx: u8) {
    let base = 0x8000u16 + (idx as u16) * 16;
    for row in 0..8u16 {
        p.write_vram(base + row * 2,     0xFF);
        p.write_vram(base + row * 2 + 1, 0xFF);
    }
}

/// Put a 4-pixel-wide stripe at VRAM tile index `idx`. The left half
/// of each row is color id 3, the right half is color id 0 (transparent
/// for sprites). Useful for X-flip tests.
fn put_left_stripe_tile(p: &mut Ppu, idx: u8) {
    let base = 0x8000u16 + (idx as u16) * 16;
    for row in 0..8u16 {
        p.write_vram(base + row * 2,     0xF0); // left 4 pixels set in low bit
        p.write_vram(base + row * 2 + 1, 0xF0); // and in high bit
    }
}

fn put_oam(p: &mut Ppu, slot: u8, y: u8, x: u8, tile: u8, attr: u8) {
    let base = 0xFE00u16 + (slot as u16) * 4;
    p.write_oam(base,     y);
    p.write_oam(base + 1, x);
    p.write_oam(base + 2, tile);
    p.write_oam(base + 3, attr);
}

/// Drive the PPU one full frame so all 144 scanlines render.
fn render_one_frame(p: &mut Ppu) {
    p.step(70224);
}

#[test]
fn sprite_basic_render() {
    let mut p = fresh_on();
    // OBJ enable + BG enable, 8x8 sprites. BGP identity, OBP0 identity.
    p.write_reg(0xFF40, 0x91 | 0x02);
    p.write_reg(0xFF47, 0xE4); // BGP: 3,2,1,0 (identity)
    p.write_reg(0xFF48, 0xE4); // OBP0: identity
    put_solid_tile(&mut p, 1);
    // Place sprite at screen (10, 20).
    put_oam(&mut p, 0, /*y*/ 20 + 16, /*x*/ 10 + 8, /*tile*/ 1, /*attr*/ 0);

    render_one_frame(&mut p);

    // All 8x8 pixels of the sprite should be shade 3.
    for row in 0..8 {
        for col in 0..8 {
            let idx = (20 + row) * SCREEN_W + (10 + col);
            assert_eq!(p.framebuffer[idx], 3, "pixel ({},{}) should be 3", 10+col, 20+row);
        }
    }
    // A neighboring pixel just outside the sprite should be BG (0 here).
    let idx = 20 * SCREEN_W + (10 + 8); // just right of sprite
    assert_eq!(p.framebuffer[idx], 0);
}

#[test]
fn sprite_transparency_skips_color_0() {
    let mut p = fresh_on();
    p.write_reg(0xFF40, 0x91 | 0x02);
    p.write_reg(0xFF47, 0xE4);
    p.write_reg(0xFF48, 0xE4);
    // A tile whose right half is color 0 (transparent for sprites).
    put_left_stripe_tile(&mut p, 1);
    put_oam(&mut p, 0, 20 + 16, 10 + 8, 1, 0);

    render_one_frame(&mut p);

    // Left 4 pixels: drawn (shade 3).
    for col in 0..4 {
        let idx = 20 * SCREEN_W + (10 + col);
        assert_eq!(p.framebuffer[idx], 3);
    }
    // Right 4 pixels: BG showing through (shade 0).
    for col in 4..8 {
        let idx = 20 * SCREEN_W + (10 + col);
        assert_eq!(p.framebuffer[idx], 0, "transparent sprite pixel should keep BG");
    }
}

#[test]
fn sprite_x_flip_mirrors_horizontally() {
    let mut p = fresh_on();
    p.write_reg(0xFF40, 0x91 | 0x02);
    p.write_reg(0xFF47, 0xE4);
    p.write_reg(0xFF48, 0xE4);
    put_left_stripe_tile(&mut p, 1);
    // X-flip attribute set.
    put_oam(&mut p, 0, 20 + 16, 10 + 8, 1, 0x20);

    render_one_frame(&mut p);

    // After X-flip, the *right* 4 pixels are now opaque.
    for col in 0..4 {
        let idx = 20 * SCREEN_W + (10 + col);
        assert_eq!(p.framebuffer[idx], 0, "left should be BG after X-flip");
    }
    for col in 4..8 {
        let idx = 20 * SCREEN_W + (10 + col);
        assert_eq!(p.framebuffer[idx], 3, "right should be sprite after X-flip");
    }
}

#[test]
fn sprite_ten_per_line_limit() {
    let mut p = fresh_on();
    p.write_reg(0xFF40, 0x91 | 0x02);
    p.write_reg(0xFF47, 0xE4);
    p.write_reg(0xFF48, 0xE4);
    put_solid_tile(&mut p, 1);

    // Place 11 sprites all on line 20. X coordinates 10, 20, 30, ..., 110.
    // The 11th (X=110) must be silently dropped.
    for i in 0..11u8 {
        put_oam(&mut p, i, 20 + 16, (10 + i * 10) + 8, 1, 0);
    }

    render_one_frame(&mut p);

    // Sprites 0..9 (X=10..100) should be visible.
    for i in 0..10 {
        let x = 10 + i * 10;
        let idx = 20 * SCREEN_W + x;
        assert_eq!(p.framebuffer[idx], 3, "sprite #{} (x={}) should be visible", i, x);
    }
    // Sprite 10 (X=110) is dropped — pixel must be BG (0).
    let idx = 20 * SCREEN_W + 110;
    assert_eq!(p.framebuffer[idx], 0, "11th sprite must be dropped");
}

#[test]
fn sprite_8x16_mode_uses_two_tiles() {
    let mut p = fresh_on();
    // 8x16 sprite mode.
    p.write_reg(0xFF40, 0x91 | 0x02 | 0x04);
    p.write_reg(0xFF47, 0xE4);
    p.write_reg(0xFF48, 0xE4);

    // Tile 2: solid color 3 (top half).
    put_solid_tile(&mut p, 2);
    // Tile 3 (= 2|1): solid color 1 (using BGP 0xE4: id 1 -> shade 1).
    let base = 0x8000u16 + 3 * 16;
    for row in 0..8u16 {
        p.write_vram(base + row * 2,     0xFF); // low bit set
        p.write_vram(base + row * 2 + 1, 0x00); // high bit clear -> id 1
    }

    // Sprite uses tile index 2; in 8x16, low bit of index is ignored,
    // so the two halves resolve to tiles 2 and 3.
    put_oam(&mut p, 0, 20 + 16, 10 + 8, 2, 0);

    render_one_frame(&mut p);

    // Top 8 rows: shade 3 (tile 2).
    for row in 0..8 {
        let idx = (20 + row) * SCREEN_W + 10;
        assert_eq!(p.framebuffer[idx], 3, "top half row {} should be 3", row);
    }
    // Bottom 8 rows: shade 1 (tile 3 with color id 1, BGP identity).
    for row in 8..16 {
        let idx = (20 + row) * SCREEN_W + 10;
        assert_eq!(p.framebuffer[idx], 1, "bottom half row {} should be 1", row);
    }
}