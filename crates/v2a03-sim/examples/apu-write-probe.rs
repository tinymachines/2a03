//! The $4017 write, measured off rung 0 at both CPU-cycle parities: a
//! square's length counter is loaded, then $4017 written (mode from the
//! argument), and from the write's strobe every change of the length
//! counter, every quarter and half-frame clock (`frm_/quarter`,
//! `frm_/half`), the mode-1 immediate clock (`frm_mode1_reset_clock`),
//! the phase strobes and the IRQ flag are printed as half-steps after
//! the strobe, with the APU cycle parity (`apu_clk1`) the strobe fell
//! on. blargg's apu_test 1, 4, 5 and 6 ask exactly these questions.
//!
//!   cargo run --release -p v2a03-sim --example apu-write-probe -- [0|1] [0|1: an extra STA zp, three cycles, before the write] [half_steps]
//!
//! A NOP would not do: two CPU cycles is one APU cycle, and the parity
//! that matters is the APU's (`apu_clk1` at the strobe).

use v2a03_sim::harness::Harness;
use v2a03_sim::Cpu;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode: u8 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let nop: bool = args.get(2).map(|s| s == "1").unwrap_or(false);
    let half_steps: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(70_000);
    // LDA #1; STA $4015; LDA #$18; STA $4003 (length index 3: two
    // half-frames); [STA $00]; LDA #mode<<7; STA $4017; JMP spin.
    let mut prog = vec![0xa9u8, 0x01, 0x8d, 0x15, 0x40, 0xa9, 0x18, 0x8d, 0x03, 0x40];
    // DELAY=n puts n NOPs before the write, so it lands well after
    // power-on (the recorder's and the gate's early writes do not).
    let delay: usize = std::env::var("DELAY").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    prog.extend(std::iter::repeat_n(0xea, delay));
    if nop {
        prog.extend([0x85, 0x00]);
    }
    prog.extend([0xa9, mode << 7, 0x8d, 0x17, 0x40]);
    let here = 0x8000 + prog.len() as u16;
    prog.extend([0x4c, here as u8, (here >> 8) as u8]);
    let mut h = Harness::new(Cpu::power_on());
    h.load(0x8000, &prog, 0x8000);
    let nl = h.cpu.engine.netlist().clone();
    let n = |name: &str| nl.node(name).unwrap_or_else(|| panic!("node {name}"));
    let len: Vec<_> = (0..8).map(|i| n(&format!("sq0_len{i}"))).collect();
    let phases: Vec<_> = ["frm_phase_a", "frm_phase_b", "frm_phase_c", "frm_phase_d", "frm_phase_e"].iter().map(|s| n(s)).collect();
    let watch = [("quarter (low)", n("frm_/quarter")), ("half (low)", n("frm_/half")), ("mode1_reset_clock", n("frm_mode1_reset_clock")), ("reset_from_write", n("frm_reset_from_write")), ("queue_reset", n("frm_queue_reset")), ("frame_irq", n("frame_irq")), ("lfsr_reset", n("frm_lfsr_reset")), ("phase_force_reset", n("frm_phase_force_reset"))];
    let (w4017, clk1) = (n("w4017"), n("apu_clk1"));
    let bits = |h: &Harness, ns: &[halfphi::NodeId]| -> u32 { ns.iter().enumerate().map(|(i, &nd)| (h.cpu.engine.is_high(nd) as u32) << i).sum() };

    let mut write_at: Option<usize> = None;
    let mut prev_w = false;
    let mut prev_len = bits(&h, &len);
    let mut prev_watch: Vec<bool> = watch.iter().map(|(_, nd)| h.cpu.engine.is_high(*nd)).collect();
    let mut prev_ph = [false; 5];
    println!("mode {mode}, {}: events as half-steps after the $4017 write strobe", if nop { "an odd cycle added before the write" } else { "no cycle added" });
    for step in 0..half_steps {
        h.half_step();
        let w = h.cpu.engine.is_high(w4017);
        if w && !prev_w {
            write_at = Some(step);
            println!("  strobe at half-step {step}: apu_clk1={} sq0_len={}", h.cpu.engine.is_high(clk1) as u8, bits(&h, &len));
        }
        prev_w = w;
        let since = |s: usize| write_at.map(|w| s as i64 - w as i64);
        let l = bits(&h, &len);
        if l != prev_len {
            println!("  sq0_len {} -> {} at {:?} (apu_clk1={})", prev_len, l, since(step), h.cpu.engine.is_high(clk1) as u8);
            prev_len = l;
        }
        for (i, (name, nd)) in watch.iter().enumerate() {
            let v = h.cpu.engine.is_high(*nd);
            if v != prev_watch[i] {
                if write_at.is_some() {
                    println!("  {name} -> {} at {:?}", v as u8, since(step).unwrap());
                }
                prev_watch[i] = v;
            }
        }
        for (i, &p) in phases.iter().enumerate() {
            let v = h.cpu.engine.is_high(p);
            if v && !prev_ph[i] && write_at.is_some() {
                println!("  phase_{} rises at {}", (b'a' + i as u8) as char, since(step).unwrap());
            }
            prev_ph[i] = v;
        }
    }
}
