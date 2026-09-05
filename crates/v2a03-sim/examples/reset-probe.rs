//! How the 2A03's reset pin reaches its core, measured: the interrupt
//! fixture's program (the recorded 6502 `fixture-reset-mid-run` case:
//! reset asserted at h=20 for eight phases) is run with the reset held
//! for a series of durations, and the frames from assertion to the first
//! reset-vector read are printed, so the divergence from the 6502's
//! recording is stated in the chip's own numbers before the gate names
//! it.
//!
//!   cargo run --release -p v2a03-sim --example reset-probe -- [file.pins]

use v2a03_sim::pins::CorePins;
use v6502_pins::{line, parse_trace, run, Stim, IDLE_INPUTS};

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../../6502/tools/pin-golden/fixture-reset-mid-run.pins").into()
    });
    let trace = parse_trace(&std::fs::read_to_string(&path).unwrap()).unwrap();
    println!("recorded 6502, h=18..44:");
    for f in &trace.frames[18..44] {
        println!("  {}", line(f));
    }
    let (r, i, n, y, s) = IDLE_INPUTS;
    for hold in [8u64, 12, 16, 24, 48, 96] {
        let stim = vec![
            Stim { h: 20, res: false, irq: i, nmi: n, rdy: y, so: s },
            Stim { h: 20 + hold, res: r, irq: i, nmi: n, rdy: y, so: s },
        ];
        let mut core = CorePins::new(&trace.header.loads, trace.header.reset_vector);
        let frames = run(&mut core, 20 + hold + 80, &stim);
        let vec = frames.iter().position(|f| f.ab == 0xfffc && f.rw);
        println!(
            "2a03, reset held {hold} phases from h=20: first $FFFC read at h={vec:?}{}",
            if core.clock_stopped { " (clk0 STOPPED)" } else { "" }
        );
        let upto = vec.map(|v| v + 6).unwrap_or(20 + hold as usize + 20).min(frames.len());
        for f in &frames[18..upto] {
            println!("  {}", line(f));
        }
    }
}
