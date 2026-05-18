// src/joypad/mod.rs
//
// The DMG joypad.
//
// One memory-mapped register: 0xFF00 (JOYP).
//
//   Bits 7,6 : unused, read as 1
//   Bit 5    : select action buttons   (0 = selected; CPU writes)
//   Bit 4    : select direction buttons (0 = selected; CPU writes)
//   Bits 3..0: button state for the selected set, 0 = pressed
//
// We keep an internal 8-bit `pressed` mask:
//   bit 7: Down   bit 6: Up     bit 5: Left   bit 4: Right
//   bit 3: Start  bit 2: Select bit 1: B      bit 0: A
// and translate to/from the JOYP selection bits on every read.

#[derive(Clone, Copy)]
pub enum Button {
    Right  = 0,
    Left   = 1,
    Up     = 2,
    Down   = 3,
    A      = 4,
    B      = 5,
    Select = 6,
    Start  = 7,
}

pub struct Joypad {
    /// bit set = button pressed. Bits 0..3 are D-pad (right/left/up/down),
    /// bits 4..7 are action (a/b/select/start).
    pressed: u8,

    /// The two upper bits the CPU last wrote — which set is selected.
    /// We store them in their canonical positions (bits 4 and 5).
    select: u8,

    /// Set true on falling edge of any selected, currently-low bit.
    /// MMU consumes this and ORs into IF bit 4 (joypad interrupt).
    pub interrupt_request: bool,
}

impl Joypad {
    pub fn new() -> Self {
        // Default: nothing pressed, both sets deselected (bits 4 and 5 high).
        Self { pressed: 0, select: 0x30, interrupt_request: false }
    }

    /// Update the press state. `pressed_now` uses the same encoding as
    /// `Button` discriminants (bit positions 0..=7). Called once per
    /// frame by the front-end after polling the keyboard.
    pub fn set_state(&mut self, pressed_now: u8) {
        // Recompute current visible low nibble before the change.
        let before = self.read_low_nibble();
        self.pressed = pressed_now;
        let after = self.read_low_nibble();
        // Falling edge on any visible bit ⇒ raise joypad IRQ.
        // (Bits are active-low, so "fall" = was 1 (released/unselected),
        // now 0 (pressed in the selected set).)
        if (before & !after) != 0 {
            self.interrupt_request = true;
        }
    }

    pub fn read(&self) -> u8 {
        // Top two bits always read as 1, then the selection bits the CPU
        // wrote, then the active-low button state for the selected set.
        0xC0 | self.select | self.read_low_nibble()
    }

    pub fn write(&mut self, value: u8) {
        // Keep just the two CPU-writable bits (4 and 5). These ARE the
        // current selection state directly: 0 = the corresponding set is
        // selected. We previously also needed to mask, but it's clearer
        // to store the original bits unchanged so "bit 4 == 0" means
        // exactly what the hardware says it means.
        let new_select = value & 0x30;
        let before = self.read_low_nibble();
        self.select = new_select;
        let after = self.read_low_nibble();
        if (before & !after) != 0 {
            self.interrupt_request = true;
        }
    }

    /// Compute the low 4 bits as JOYP would report them: 0 = pressed.
    /// Either set whose select-bit is 0 contributes its (active-low)
    /// pressed bits; an unselected set contributes all-1s (no presses).
    fn read_low_nibble(&self) -> u8 {
        let mut out = 0x0F;
        // Bit 4 of select == 0 means direction set is selected.
        if self.select & 0x10 == 0 {
            // D-pad lives in `pressed` bits 0..3 (active-high).
            let dpad = !self.pressed & 0x0F;
            out &= dpad;
        }
        // Bit 5 of select == 0 means action set is selected.
        if self.select & 0x20 == 0 {
            let action = !(self.pressed >> 4) & 0x0F;
            out &= action;
        }
        out
    }
}

impl Default for Joypad { fn default() -> Self { Self::new() } }

/// Bit layout helpers for callers building a `pressed` mask. Each `set`
/// shifts the corresponding bit into the right slot.
#[inline]
pub fn bit(b: Button) -> u8 { 1 << (b as u8) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_pressed_reads_all_ones() {
        let p = Joypad::new();
        // Default both sets deselected, no buttons pressed.
        assert_eq!(p.read() & 0x0F, 0x0F);
    }

    #[test]
    fn pressing_a_with_action_selected_shows_low_bit_clear() {
        let mut p = Joypad::new();
        // Action selection: bit 5 = 0, bit 4 = 1 → write 0xDF.
        p.write(0xDF);
        p.set_state(bit(Button::A));
        // A is bit 0 of the action nibble.
        assert_eq!(p.read() & 0x01, 0);
    }

    #[test]
    fn pressing_right_with_dpad_selected_shows_low_bit_clear() {
        let mut p = Joypad::new();
        // D-pad selection: bit 4 = 0, bit 5 = 1 → write 0xEF.
        p.write(0xEF);
        p.set_state(bit(Button::Right));
        assert_eq!(p.read() & 0x01, 0);
    }

    #[test]
    fn dpad_press_invisible_when_only_action_selected() {
        let mut p = Joypad::new();
        p.write(0xDF); // action selection
        p.set_state(bit(Button::Down));
        // Down (bit 3 of dpad nibble) shouldn't appear in the action read.
        assert_eq!(p.read() & 0x0F, 0x0F);
    }

    #[test]
    fn interrupt_fires_on_selected_press() {
        let mut p = Joypad::new();
        p.write(0xDF); // select action
        p.interrupt_request = false;
        p.set_state(bit(Button::Start));
        assert!(p.interrupt_request);
    }

    #[test]
    fn interrupt_does_not_fire_on_unselected_press() {
        let mut p = Joypad::new();
        p.write(0xDF); // action selected, dpad NOT
        p.interrupt_request = false;
        p.set_state(bit(Button::Left)); // a dpad press
        assert!(!p.interrupt_request);
    }
}