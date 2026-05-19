// src/apu/tests.rs

use super::*;

// ────────────────────────────────────────────────────────────────────────
// Register masks
// ────────────────────────────────────────────────────────────────────────

#[test]
fn nr10_unused_bit7_reads_as_one() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF10, 0x00);
    assert_eq!(apu.read_reg(0xFF10), 0x80);
}

#[test]
fn nr13_is_write_only() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF13, 0xAB);
    assert_eq!(apu.read_reg(0xFF13), 0xFF);
}

#[test]
fn nr14_only_length_enable_readable() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF14, 0xFF);
    assert_eq!(apu.read_reg(0xFF14), 0xFF);
    apu.write_reg(0xFF14, 0x00);
    assert_eq!(apu.read_reg(0xFF14), 0xBF);
}

// ────────────────────────────────────────────────────────────────────────
// NR52 master enable
// ────────────────────────────────────────────────────────────────────────

#[test]
fn nr52_powered_off_by_default() {
    let apu = Apu::new();
    assert_eq!(apu.read_reg(0xFF26), 0x70);
}

#[test]
fn writes_ignored_while_apu_off() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF10, 0x77);
    apu.write_reg(0xFF26, 0x80);
    assert_eq!(apu.read_reg(0xFF10), 0x80);
}

#[test]
fn powerdown_clears_apu_registers() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF12, 0xF3);
    apu.write_reg(0xFF24, 0x77);
    apu.write_reg(0xFF26, 0x00);
    apu.write_reg(0xFF26, 0x80);
    assert_eq!(apu.read_reg(0xFF12), 0x00);
    assert_eq!(apu.read_reg(0xFF24), 0x00);
}

#[test]
fn wave_ram_survives_powerdown() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    for i in 0..16u16 {
        apu.write_reg(0xFF30 + i, i as u8);
    }
    apu.write_reg(0xFF26, 0x00);
    for i in 0..16u16 {
        assert_eq!(apu.read_reg(0xFF30 + i), i as u8);
    }
}

// ────────────────────────────────────────────────────────────────────────
// Frame sequencer
// ────────────────────────────────────────────────────────────────────────
//
// Note on semantics: `frame_seq_step` is the LAST STEP DISPATCHED.
// On power-on it is set to 7 so that the next 512 Hz tick advances
// it to 0 and dispatches step 0 (length clock). This models the
// "next step will be 0" behaviour per the gg8 wiki.

#[test]
fn frame_sequencer_advances_every_8192_cycles() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    // After power-on, last-dispatched-step is set to 7 so the next
    // tick lands on step 0.
    assert_eq!(apu.frame_seq_step, 7);
    apu.step(8191);
    assert_eq!(apu.frame_seq_step, 7);
    apu.step(1);
    assert_eq!(apu.frame_seq_step, 0);
}

// ────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────

fn powered() -> Apu {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);
    apu.write_reg(0xFF24, 0x77);
    apu.write_reg(0xFF25, 0xFF);
    apu
}

// ────────────────────────────────────────────────────────────────────────
// CH1 / CH2 — trigger and DAC
// ────────────────────────────────────────────────────────────────────────

#[test]
fn ch1_trigger_enables_channel_when_dac_on() {
    let mut apu = powered();
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF14, 0x80);
    assert!(apu.ch1.enabled);
    assert_eq!(apu.ch1.envelope.volume, 15);
}

#[test]
fn ch1_trigger_with_dac_off_stays_disabled() {
    let mut apu = powered();
    apu.write_reg(0xFF12, 0x00);
    apu.write_reg(0xFF14, 0x80);
    assert!(!apu.ch1.enabled);
}

#[test]
fn writing_zero_to_nr12_disables_channel() {
    let mut apu = powered();
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF14, 0x80);
    assert!(apu.ch1.enabled);
    apu.write_reg(0xFF12, 0x00);
    assert!(!apu.ch1.enabled);
}

// ────────────────────────────────────────────────────────────────────────
// Envelope
// ────────────────────────────────────────────────────────────────────────

#[test]
fn envelope_decays_at_each_envelope_tick() {
    let mut apu = powered();
    apu.write_reg(0xFF12, 0xF1);
    apu.write_reg(0xFF13, 0x00);
    apu.write_reg(0xFF14, 0x87);
    apu.step(8192 * 8);
    assert_eq!(apu.ch1.envelope.volume, 14);
    apu.step(8192 * 8);
    assert_eq!(apu.ch1.envelope.volume, 13);
}

#[test]
fn envelope_saturates_at_zero() {
    let mut apu = powered();
    apu.write_reg(0xFF12, 0x21);
    apu.write_reg(0xFF14, 0x80);
    apu.step(8192 * 8 * 5);
    assert_eq!(apu.ch1.envelope.volume, 0);
}

#[test]
fn envelope_period_zero_disables_envelope() {
    let mut apu = powered();
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF14, 0x80);
    apu.step(8192 * 8 * 20);
    assert_eq!(apu.ch1.envelope.volume, 15);
}

// ────────────────────────────────────────────────────────────────────────
// Length counter
// ────────────────────────────────────────────────────────────────────────

#[test]
fn length_counter_silences_channel() {
    let mut apu = powered();
    apu.write_reg(0xFF11, 0xBF);
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF14, 0xC0);
    assert!(apu.ch1.enabled);
    apu.step(8192 * 2 + 1);
    assert!(!apu.ch1.enabled);
}

#[test]
fn length_disabled_means_no_expiry() {
    let mut apu = powered();
    apu.write_reg(0xFF11, 0xBF);
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF14, 0x80);
    apu.step(8192 * 64);
    assert!(apu.ch1.enabled);
}

// ────────────────────────────────────────────────────────────────────────
// Sweep
// ────────────────────────────────────────────────────────────────────────

#[test]
fn sweep_increases_frequency() {
    let mut apu = powered();
    apu.write_reg(0xFF10, 0x11);
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF13, 0x00);
    apu.write_reg(0xFF14, 0x80);
    apu.write_reg(0xFF13, 0x00);
    apu.write_reg(0xFF14, 0x81);
    let start = apu.ch1.freq;
    apu.step(8192 * 3 + 1);
    assert!(apu.ch1.freq > start);
}

#[test]
fn sweep_overflow_disables_channel() {
    let mut apu = powered();
    apu.write_reg(0xFF10, 0x11);
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF13, 0xFE);
    apu.write_reg(0xFF14, 0x87);
    apu.step(8192 * 3 + 1);
    assert!(!apu.ch1.enabled);
}

// ────────────────────────────────────────────────────────────────────────
// Sample emission
// ────────────────────────────────────────────────────────────────────────

#[test]
fn sample_emission_rate_matches_target() {
    let mut apu = powered();
    apu.step(CPU_HZ);
    let samples = apu.take_samples();
    let pairs = samples.len() / 2;
    assert!(
        (pairs as i64 - SAMPLE_RATE as i64).abs() <= 1,
        "expected ~{} stereo samples, got {}", SAMPLE_RATE, pairs
    );
    assert_eq!(samples.len() % 2, 0);
}

#[test]
fn ch1_at_440hz_alternates_high_low() {
    let mut apu = powered();
    apu.write_reg(0xFF11, 0x80);
    apu.write_reg(0xFF12, 0xF0);
    apu.write_reg(0xFF13, 0xD6);
    apu.write_reg(0xFF14, 0x86);
    apu.step(CPU_HZ / 100);
    let samples = apu.take_samples();
    let any_zero    = samples.iter().any(|&s| s == 0.0);
    let any_nonzero = samples.iter().any(|&s| s != 0.0);
    assert!(any_zero);
    assert!(any_nonzero);
}

// ────────────────────────────────────────────────────────────────────────
// CH3 — wave
// ────────────────────────────────────────────────────────────────────────

#[test]
fn ch3_trigger_with_dac_off_stays_disabled() {
    let mut apu = powered();
    apu.write_reg(0xFF1A, 0x00);
    apu.write_reg(0xFF1E, 0x80);
    assert!(!apu.ch3.enabled);
}

#[test]
fn ch3_trigger_with_dac_on_enables() {
    let mut apu = powered();
    apu.write_reg(0xFF1A, 0x80);
    apu.write_reg(0xFF1C, 0x20);
    apu.write_reg(0xFF1E, 0x80);
    assert!(apu.ch3.enabled);
}

#[test]
fn ch3_walks_through_wave_ram() {
    let mut apu = powered();
    for i in 0..16u8 {
        let upper = (i * 2) & 0x0F;
        let lower = (i * 2 + 1) & 0x0F;
        apu.write_reg(0xFF30 + i as u16, (upper << 4) | lower);
    }
    apu.write_reg(0xFF1A, 0x80);
    apu.write_reg(0xFF1C, 0x20);
    apu.write_reg(0xFF1D, 0x00);
    apu.write_reg(0xFF1E, 0x80);

    assert_eq!(apu.ch3.sample(&apu.wave_ram), 0);
    assert_eq!(apu.ch3.sample_index, 0);

    apu.step(4096 + 1);
    assert_eq!(apu.ch3.sample_index, 1);
    assert_eq!(apu.ch3.sample(&apu.wave_ram), 1);

    apu.step(4096 * 2);
    assert_eq!(apu.ch3.sample_index, 3);
    assert_eq!(apu.ch3.sample(&apu.wave_ram), 3);
}

#[test]
fn ch3_volume_shift_mutes_at_zero() {
    let mut apu = powered();
    for i in 0..16u16 { apu.write_reg(0xFF30 + i, 0xFF); }
    apu.write_reg(0xFF1A, 0x80);
    apu.write_reg(0xFF1C, 0x00);
    apu.write_reg(0xFF1E, 0x80);
    assert_eq!(apu.ch3.sample(&apu.wave_ram), 0);
}

#[test]
fn ch3_volume_shift_half_halves_amplitude() {
    let mut apu = powered();
    for i in 0..16u16 { apu.write_reg(0xFF30 + i, 0xFF); }
    apu.write_reg(0xFF1A, 0x80);
    apu.write_reg(0xFF1C, 0x40);
    apu.write_reg(0xFF1E, 0x80);
    assert_eq!(apu.ch3.sample(&apu.wave_ram), 7);
}

#[test]
fn ch3_length_counter_is_8bit() {
    let mut apu = powered();
    apu.write_reg(0xFF1A, 0x80);
    apu.write_reg(0xFF1B, 0xFF);
    apu.write_reg(0xFF1C, 0x20);
    apu.write_reg(0xFF1E, 0xC0);
    assert!(apu.ch3.enabled);
    apu.step(8192 * 2 + 1);
    assert!(!apu.ch3.enabled);
}

#[test]
fn ch3_trigger_resets_sample_index() {
    let mut apu = powered();
    apu.write_reg(0xFF1A, 0x80);
    apu.write_reg(0xFF1C, 0x20);
    apu.write_reg(0xFF1D, 0x00);
    apu.write_reg(0xFF1E, 0x80);
    apu.step(4096 * 5 + 1);
    assert_eq!(apu.ch3.sample_index, 5);
    apu.write_reg(0xFF1E, 0x80);
    assert_eq!(apu.ch3.sample_index, 0);
}

// ────────────────────────────────────────────────────────────────────────
// CH4 — noise
// ────────────────────────────────────────────────────────────────────────

#[test]
fn ch4_trigger_seeds_lfsr_to_all_ones() {
    let mut apu = powered();
    apu.write_reg(0xFF21, 0xF0);
    apu.write_reg(0xFF22, 0x00);
    apu.write_reg(0xFF23, 0x80);
    assert_eq!(apu.ch4.lfsr, 0x7FFF);
}

#[test]
fn ch4_trigger_with_dac_off_stays_disabled() {
    let mut apu = powered();
    apu.write_reg(0xFF21, 0x00);
    apu.write_reg(0xFF23, 0x80);
    assert!(!apu.ch4.enabled);
}

#[test]
fn ch4_lfsr_advances_on_tick() {
    let mut apu = powered();
    apu.write_reg(0xFF21, 0xF0);
    apu.write_reg(0xFF22, 0x00);
    apu.write_reg(0xFF23, 0x80);
    assert_eq!(apu.ch4.lfsr, 0x7FFF);
    apu.step(8 + 1);
    assert_ne!(apu.ch4.lfsr, 0x7FFF);
}

#[test]
fn ch4_produces_varied_samples() {
    let mut apu = powered();
    apu.write_reg(0xFF21, 0xF0);
    apu.write_reg(0xFF22, 0x00);
    apu.write_reg(0xFF23, 0x80);
    apu.step(CPU_HZ / 200);
    let samples = apu.take_samples();
    let any_zero    = samples.iter().any(|&s| s == 0.0);
    let any_nonzero = samples.iter().any(|&s| s != 0.0);
    assert!(any_zero, "noise should produce silent samples");
    assert!(any_nonzero, "noise should produce loud samples");
}

#[test]
fn ch4_7bit_mode_sets_width_flag() {
    let mut apu = powered();
    apu.write_reg(0xFF21, 0xF0);
    apu.write_reg(0xFF22, 0x08);
    apu.write_reg(0xFF23, 0x80);
    assert!(apu.ch4.width_mode);
}

// ────────────────────────────────────────────────────────────────────────
// Diagnostic: mirror Blargg 11-regs-after-power test #4
// ────────────────────────────────────────────────────────────────────────

/// Mirror what `11-regs after power.s` set_test 4 does. Verifies that
/// the length counter survives the power cycle AND that the frame
/// sequencer's "delay 8192 to avoid extra length clocking" mechanism
/// works as documented.
#[test]
fn blargg_11_test4_length_survives_power_cycle() {
    let mut apu = Apu::new();
    apu.write_reg(0xFF26, 0x80);

    for addr in 0xFF10u16..=0xFF25u16 {
        apu.write_reg(addr, 0xFF);
    }

    apu.write_reg(0xFF26, 0x00);

    apu.write_reg(0xFF20, 0xEE); // NR41 length-only write
    apu.write_reg(0xFF12, 0xF0); // ignored

    apu.write_reg(0xFF26, 0x80); // power on

    println!("After power on, ch4.length.counter = {}", apu.ch4.length.counter);
    assert_eq!(apu.ch4.length.counter, 18);

    // delay_clocks 8192. Under the new model, frame_seq_step was 7
    // after power-on; this brings it to 0 and dispatches step 0,
    // which clocks length. Counter goes 18 → 17.
    apu.step(8192);

    apu.write_reg(0xFF21, 0x08); // NR42: DAC on
    apu.write_reg(0xFF23, 0xC0); // NR44: trigger + length-enable

    println!("After trigger, ch4.enabled = {}, length.counter = {}",
        apu.ch4.enabled, apu.ch4.length.counter);
    assert!(apu.ch4.enabled);
    // After the 8192-cycle delay length ticked once → counter = 17.
    assert_eq!(apu.ch4.length.counter, 17);

    // From here, the test waits 17 length ticks then expects channel
    // to still be enabled, then one more tick to disable it. With
    // counter = 17 going in, 16 ticks brings it to 1 (still enabled),
    // 17 ticks brings it to 0 (disabled). So our test should match
    // `delay_apu 17 + 1` from the Blargg ROM exactly.
    //
    // Length ticks at frame-seq steps 0/2/4/6 — 4 per 8 steps, i.e.
    // one length tick per 2 frame-seq steps = 16384 cycles per length
    // tick. After 16 length ticks (16 * 16384 = 262144 cycles):
    apu.step(16384 * 16);
    println!("After 16 length ticks: enabled = {}, length = {}",
        apu.ch4.enabled, apu.ch4.length.counter);
    assert!(apu.ch4.enabled, "should still be enabled");
    assert_eq!(apu.ch4.length.counter, 1);

    apu.step(16384);
    println!("After 17 length ticks: enabled = {}, length = {}",
        apu.ch4.enabled, apu.ch4.length.counter);
    assert!(!apu.ch4.enabled, "should now be disabled");
}