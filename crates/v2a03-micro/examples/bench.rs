//! The core rung's throughput beside this chip's rung 0, on the same
//! program (the reference's, from the recorded `golden.pins` header).
//! Half-cycles per second, best of three for the rung (noise only ever
//! slows a run), one run for rung 0 (it is slow enough to be steady).
//! Real time for the 2A03's core is 3.579545 M half-cycles/s (the master
//! clock over 12, two half-cycles per cycle).
//!
//!   cargo run --release -p v2a03-micro --example bench -- [rung-half-cycles] [rung0-half-cycles]

use std::time::Instant;

use v2a03_sim::pins::CorePins;
use v6502_pins::{parse_trace, PinEngine};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n_rung: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(20_000_000);
    let n_r0: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20_000);
    let path = std::env::var("PIN_GOLDEN")
        .map(|d| format!("{d}/golden.pins"))
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../../6502/tools/pin-golden/golden.pins").into());
    let trace = parse_trace(&std::fs::read_to_string(&path).expect("golden.pins")).unwrap();
    let real_time = 21_477_272.0 / 12.0 * 2.0;

    let mut r0 = CorePins::new(&trace.header.loads, trace.header.reset_vector);
    r0.power_cycle();
    let s = r0.stack_pointer();
    let t = Instant::now();
    for _ in 0..n_r0 {
        r0.half_step();
    }
    let r0_rate = n_r0 as f64 / t.elapsed().as_secs_f64();
    println!("rung 0 (switch level, memory harness): {r0_rate:.0} half-cycles/s over {n_r0}");

    let mut best = 0.0f64;
    for _ in 0..3 {
        let mut m = v2a03_micro::core(&trace.header.loads, trace.header.reset_vector, s);
        let t = Instant::now();
        for _ in 0..n_rung {
            m.half_step();
        }
        best = best.max(n_rung as f64 / t.elapsed().as_secs_f64());
    }
    println!(
        "core rung: {best:.0} half-cycles/s over {n_rung} (best of 3), {:.0}x rung 0, {:.1}x real time ({real_time:.0} half-cycles/s)",
        best / r0_rate,
        best / real_time
    );

    // The whole rung: the core with the APU fed from its writes, as the
    // gate runs it, over a program that plays every channel.
    let mut prog = Vec::new();
    for (r, v) in [(0x17u8, 0x00u8), (0x15, 0x0f), (0x00, 0xa6), (0x01, 0xa9), (0x02, 0xab), (0x03, 0x09), (0x04, 0x7f), (0x05, 0x91), (0x06, 0x00), (0x07, 0x3a), (0x08, 0xc0), (0x0a, 0x50), (0x0b, 0x48), (0x0c, 0x04), (0x0e, 0x04), (0x0f, 0x10), (0x10, 0x4f), (0x11, 0x20), (0x12, 0x00), (0x13, 0x02), (0x15, 0x1f)] {
        prog.extend([0xa9, v, 0x8d, r, 0x40]);
    }
    let spin = 0x8000 + prog.len() as u16;
    prog.extend([0x4c, spin as u8, (spin >> 8) as u8]);
    let loads = vec![v6502_pins::Load { org: 0x8000, bytes: prog }, v6502_pins::Load { org: 0xc000, bytes: vec![0xa5; 33] }];
    let mut best_apu = 0.0f64;
    for _ in 0..3 {
        let mut m = v2a03_micro::core(&loads, 0x8000, s);
        let mut apu = v2a03_micro::apu::Apu::new();
        let t = Instant::now();
        for _ in 0..n_rung {
            m.half_step();
            let f = m.pins();
            if !f.rw && f.clk0 && (0x4000..=0x4017).contains(&f.ab) {
                apu.write((f.ab & 0x1f) as u8, f.db);
            }
            apu.half_step(&mut |a| m.mem[a as usize]);
        }
        best_apu = best_apu.max(n_rung as f64 / t.elapsed().as_secs_f64());
        std::hint::black_box(apu.codes());
    }
    println!(
        "core rung with the APU: {best_apu:.0} half-cycles/s over {n_rung} (best of 3), {:.0}x rung 0, {:.1}x real time",
        best_apu / r0_rate,
        best_apu / real_time
    );
}
