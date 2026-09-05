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
}
