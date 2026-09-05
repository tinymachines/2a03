//! N3 step 3, probe 1: the frame sequencer, measured off rung 0. A
//! program writes $4017 (mode from the argument: 0 = 4-step, 1 = 5-step)
//! and spins; every CPU half-step the frame counter's nodes are read
//! (`frm_t0..14` the 15-bit LFSR, `frm_phase_a..e` the phase strobes,
//! `frm_seqmode`, `frm_intmode`, `frame_irq`, `frm_xor_out`), and the
//! probe prints each phase strobe's rise as a CPU cycle count from the
//! $4017 write's own bus cycle, over two full sequences, plus the IRQ
//! flag's rise. These are the numbers step 4's table is authored from.
//!
//!   cargo run --release -p v2a03-sim --example apu-frame-probe -- [0|1] [half_steps]

use v2a03_sim::harness::Harness;
use v2a03_sim::Cpu;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode: u8 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let half_steps: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(150_000);
    // LDA #mode<<7 ; STA $4017 ; JMP spin. Mode 2: no write at all, the
    // frame counter as power-on leaves it, positions from the first frame.
    let prog = if mode == 2 { vec![0x4cu8, 0x00, 0x80] } else { vec![0xa9u8, mode << 7, 0x8d, 0x17, 0x40, 0x4c, 0x05, 0x80] };
    let mut h = Harness::new(Cpu::power_on());
    h.load(0x8000, &prog, 0x8000);
    let nl = h.cpu.engine.netlist().clone();
    let n = |name: &str| nl.node(name).unwrap_or_else(|| panic!("node {name}"));
    let phases: Vec<_> = ["frm_phase_a", "frm_phase_b", "frm_phase_c", "frm_phase_d", "frm_phase_e"].iter().map(|s| n(s)).collect();
    let lfsr: Vec<_> = (0..15).map(|i| n(&format!("frm_t{i}"))).collect();
    let (w4017, seqmode, intmode, irq, xor_out) = (n("w4017"), n("frm_seqmode"), n("frm_intmode"), n("frame_irq"), n("frm_xor_out"));
    let ab: Vec<_> = (0..16).map(|i| n(&format!("ab{i}"))).collect();
    let rw = n("rw");
    let bits = |h: &Harness, ns: &[halfphi::NodeId]| -> u32 { ns.iter().enumerate().map(|(i, &nd)| (h.cpu.engine.is_high(nd) as u32) << i).sum() };

    let mut write_at: Option<usize> = None;
    let mut prev = [false; 5];
    let mut prev_irq = false;
    let mut prev_w = false;
    println!("mode {mode} ({}): phase rises as CPU cycles after the $4017 write cycle", match mode { 0 => "4-step", 1 => "5-step", _ => "no write, power-on state" });
    if mode == 2 {
        write_at = Some(0);
        println!("  no write: cycles counted from the first CPU half-step after power_on (the pin contract's h=0 is 17 phases later)");
    }
    for step in 0..half_steps {
        h.half_step();
        let w = h.cpu.engine.is_high(w4017);
        if w && !prev_w {
            // The write strobe: the bus cycle that wrote $4017.
            write_at = Some(step);
            let lfsr_at_write = bits(&h, &lfsr);
            println!(
                "  $4017 write strobe at half-step {step} (ab={:04x} rw={}), seqmode={} intmode={} lfsr={lfsr_at_write:015b}",
                bits(&h, &ab),
                h.cpu.engine.is_high(rw) as u8,
                h.cpu.engine.is_high(seqmode) as u8,
                h.cpu.engine.is_high(intmode) as u8
            );
        }
        prev_w = w;
        for (i, &p) in phases.iter().enumerate() {
            let v = h.cpu.engine.is_high(p);
            if v && !prev[i] {
                let since = write_at.map(|w| (step as i64 - w as i64) as f64 / 2.0);
                println!(
                    "  phase_{} rises at half-step {step}, {} CPU cycles after the write; lfsr={:015b} xor_out={}",
                    (b'a' + i as u8) as char,
                    since.map(|c| format!("{c:.1}")).unwrap_or("(before)".into()),
                    bits(&h, &lfsr),
                    h.cpu.engine.is_high(xor_out) as u8
                );
            }
            prev[i] = v;
        }
        let irq_now = h.cpu.engine.is_high(irq);
        if irq_now != prev_irq {
            let since = write_at.map(|w| (step as i64 - w as i64) as f64 / 2.0);
            println!("  frame_irq {} at half-step {step}, {:?} CPU cycles after the write", if irq_now { "rises" } else { "falls" }, since);
        }
        prev_irq = irq_now;
    }
}
