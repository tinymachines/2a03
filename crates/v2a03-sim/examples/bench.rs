//! Quiescent throughput: power on, then time a free run of master
//! half-steps with the buses undriven, best of three.

use std::time::Instant;
use v2a03_sim::Cpu;

fn main() {
    let n = 50_000u64;
    let mut best = f64::MAX;
    for _ in 0..3 {
        let mut cpu = Cpu::power_on();
        let t = Instant::now();
        for _ in 0..n {
            cpu.half_step();
        }
        best = best.min(t.elapsed().as_secs_f64());
    }
    println!(
        "quiescent: {n} master half-steps in {best:.2}s = {:.0} half-steps/s",
        n as f64 / best
    );
}
