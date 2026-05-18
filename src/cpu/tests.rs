// src/cpu/tests.rs

use crate::cpu::Cpu;
use crate::cpu::registers::Flag;
use crate::memory::MockBus;

fn cpu_with_program(bytes: &[u8]) -> Cpu<MockBus> {
    let mut bus = MockBus::new();
    bus.load(0x0100, bytes);
    Cpu::new(bus)
}

// ── carryover from sessions 1–3 (slimmer set; new ones added below) ─────

#[test] fn nop() {
    let mut cpu = cpu_with_program(&[0x00]);
    assert_eq!(cpu.step(), 4);
}

#[test] fn add_hc() {
    let mut cpu = cpu_with_program(&[0x3E, 0x3A, 0x06, 0xC6, 0x80]);
    cpu.step(); cpu.step(); cpu.step();
    assert_eq!(cpu.regs.a, 0x00);
    assert!(cpu.regs.get_flag(Flag::Z) && cpu.regs.get_flag(Flag::H) && cpu.regs.get_flag(Flag::C));
}

#[test] fn sub_underflow() {
    let mut cpu = cpu_with_program(&[0x3E, 0x00, 0xD6, 0x01]);
    cpu.step(); cpu.step();
    assert_eq!(cpu.regs.a, 0xFF);
    assert!(cpu.regs.get_flag(Flag::C));
}

#[test] fn inc_b_wraps() {
    let mut cpu = cpu_with_program(&[0x06, 0xFF, 0x04]);
    cpu.step(); cpu.step();
    assert_eq!(cpu.regs.b, 0x00);
    assert!(cpu.regs.get_flag(Flag::Z));
}

#[test] fn ldh_round_trip() {
    let mut cpu = cpu_with_program(&[0x3E, 0xAB, 0xE0, 0x80, 0xAF, 0xF0, 0x80]);
    cpu.step(); cpu.step(); cpu.step();
    assert_eq!(cpu.regs.a, 0x00);
    cpu.step();
    assert_eq!(cpu.regs.a, 0xAB);
}

#[test] fn push_pop_bc() {
    let mut cpu = cpu_with_program(&[0x01, 0x34, 0x12, 0xC5, 0x01, 0x00, 0x00, 0xC1]);
    cpu.regs.sp = 0xFFFE;
    cpu.step(); cpu.step(); cpu.step(); cpu.step();
    assert_eq!(cpu.regs.bc(), 0x1234);
}

#[test] fn pop_af_masks_low_nibble() {
    let mut cpu = cpu_with_program(&[0x01, 0xFF, 0xFF, 0xC5, 0xF1]);
    cpu.regs.sp = 0xFFFE;
    cpu.step(); cpu.step(); cpu.step();
    assert_eq!(cpu.regs.a, 0xFF);
    assert_eq!(cpu.regs.f, 0xF0);
}

#[test] fn call_and_ret() {
    let mut bus = MockBus::new();
    bus.load(0x0100, &[0xCD, 0x50, 0x01, 0x76]);
    bus.load(0x0150, &[0x3E, 0x77, 0xC9]);
    let mut cpu = Cpu::new(bus);
    cpu.regs.sp = 0xFFFE;
    cpu.step(); cpu.step(); cpu.step();
    assert_eq!(cpu.regs.a, 0x77);
    assert_eq!(cpu.regs.pc, 0x0103);
}

// ── Session 6: rotates, CPL/SCF/CCF, ADD HL, ADD SP ─────────────────────

#[test] fn rlca_carries_top_bit() {
    let mut cpu = cpu_with_program(&[0x3E, 0x85, 0x07]); // LD A,0x85; RLCA
    cpu.step(); cpu.step();
    assert_eq!(cpu.regs.a, 0x0B); // 1000_0101 → 0000_1011
    assert!(cpu.regs.get_flag(Flag::C));
    assert!(!cpu.regs.get_flag(Flag::Z)); // un-prefixed rotates clear Z
}

#[test] fn rrca_carries_low_bit() {
    let mut cpu = cpu_with_program(&[0x3E, 0x01, 0x0F]); // LD A,0x01; RRCA
    cpu.step(); cpu.step();
    assert_eq!(cpu.regs.a, 0x80);
    assert!(cpu.regs.get_flag(Flag::C));
}

#[test] fn rla_uses_old_carry() {
    let mut cpu = cpu_with_program(&[0x3E, 0x80, 0x17]); // LD A,0x80; RLA  (C starts 1 from boot defaults)
    cpu.regs.set_flag(Flag::C, false);
    cpu.step(); cpu.step();
    assert_eq!(cpu.regs.a, 0x00); // top bit moves to C, no bit comes in
    assert!(cpu.regs.get_flag(Flag::C));
}

#[test] fn cpl_flips_a_keeps_z_c() {
    let mut cpu = cpu_with_program(&[0x3E, 0x55, 0x2F]); // LD A,0x55; CPL
    cpu.step(); cpu.step();
    assert_eq!(cpu.regs.a, 0xAA);
    assert!(cpu.regs.get_flag(Flag::N) && cpu.regs.get_flag(Flag::H));
}

#[test] fn scf_then_ccf() {
    let mut cpu = cpu_with_program(&[0x37, 0x3F]);
    cpu.step();
    assert!(cpu.regs.get_flag(Flag::C));
    cpu.step();
    assert!(!cpu.regs.get_flag(Flag::C));
}

#[test] fn add_hl_de() {
    // HL = 0x8A23, DE = 0x0605 → HL = 0x9028, H=1 (bit 11 carry), C=0
    let mut cpu = cpu_with_program(&[0x21, 0x23, 0x8A, 0x11, 0x05, 0x06, 0x19]);
    cpu.step(); cpu.step(); cpu.step();
    assert_eq!(cpu.regs.hl(), 0x9028);
    assert!(cpu.regs.get_flag(Flag::H));
    assert!(!cpu.regs.get_flag(Flag::C));
    assert!(!cpu.regs.get_flag(Flag::N));
}

#[test] fn add_sp_negative_offset() {
    // SP = 0xFFF8 ; ADD SP, -2 → 0xFFF6
    let mut cpu = cpu_with_program(&[0x31, 0xF8, 0xFF, 0xE8, 0xFE]);
    cpu.step(); cpu.step();
    assert_eq!(cpu.regs.sp, 0xFFF6);
    assert!(!cpu.regs.get_flag(Flag::Z));
    assert!(!cpu.regs.get_flag(Flag::N));
}

// ── Session 7: CB ───────────────────────────────────────────────────────

#[test] fn cb_swap_b() {
    let mut cpu = cpu_with_program(&[0x06, 0xAB, 0xCB, 0x30]); // LD B,0xAB; SWAP B
    cpu.step(); cpu.step();
    assert_eq!(cpu.regs.b, 0xBA);
}

#[test] fn cb_bit_zero_sets_z() {
    // LD A,0x00; BIT 0, A → Z=1, H=1, N=0
    let mut cpu = cpu_with_program(&[0x3E, 0x00, 0xCB, 0x47]);
    cpu.step(); cpu.step();
    assert!(cpu.regs.get_flag(Flag::Z));
    assert!(cpu.regs.get_flag(Flag::H));
    assert!(!cpu.regs.get_flag(Flag::N));
}

#[test] fn cb_bit_one_clears_z() {
    let mut cpu = cpu_with_program(&[0x3E, 0x01, 0xCB, 0x47]);
    cpu.step(); cpu.step();
    assert!(!cpu.regs.get_flag(Flag::Z));
}

#[test] fn cb_res_clears_specific_bit() {
    // LD A,0xFF; RES 3, A → 0xF7
    let mut cpu = cpu_with_program(&[0x3E, 0xFF, 0xCB, 0x9F]);
    cpu.step(); cpu.step();
    assert_eq!(cpu.regs.a, 0xF7);
}

#[test] fn cb_set_sets_specific_bit() {
    // LD A,0x00; SET 5, A → 0x20
    let mut cpu = cpu_with_program(&[0x3E, 0x00, 0xCB, 0xEF]);
    cpu.step(); cpu.step();
    assert_eq!(cpu.regs.a, 0x20);
}

#[test] fn cb_srl_carries_low_bit_and_clears_top() {
    // LD A,0x01; SRL A → A=0x00, Z=1, C=1
    let mut cpu = cpu_with_program(&[0x3E, 0x01, 0xCB, 0x3F]);
    cpu.step(); cpu.step();
    assert_eq!(cpu.regs.a, 0x00);
    assert!(cpu.regs.get_flag(Flag::Z));
    assert!(cpu.regs.get_flag(Flag::C));
}

#[test] fn cb_sra_preserves_sign() {
    // LD A,0x80; SRA A → A=0xC0 (bit 7 preserved), C=0
    let mut cpu = cpu_with_program(&[0x3E, 0x80, 0xCB, 0x2F]);
    cpu.step(); cpu.step();
    assert_eq!(cpu.regs.a, 0xC0);
    assert!(!cpu.regs.get_flag(Flag::C));
}
