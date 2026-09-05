//! The measurement the cross-chip pin-lockstep gate is written from: every
//! recorded 6502 `.pins` trace replayed through the 2A03's core (its
//! loads, reset vector and `.stim` applied by the contract's own `run`),
//! and every differing frame CLASSIFIED before any of it is asserted:
//!
//!   stack    ab differs, both in page 1, by exactly the two dies'
//!            power-on stack pointer difference
//!   wphi1    db differs on the phi1 half of a write cycle (the bus is
//!            undriven by the world there)
//!   data     db differs where the bus is serviced (a read's phi1 or a
//!            write's phi2): a different byte crossed the pins
//!   other    anything else, printed in full
//!
//! Prints a table per trace and a total per class, so the gate's
//! exemptions are typed from this output and nothing else.
//!
//!   cargo run --release -p v2a03-sim --example lockstep-probe -- [pin-golden-dir] [max-traces]

use std::collections::BTreeMap;

use v2a03_sim::pins::CorePins;
use v6502_pins::{first_difference, line, parse_stim, parse_trace, run, PinFrame};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| concat!(env!("CARGO_MANIFEST_DIR"), "/../../../6502/tools/pin-golden").into());
    let max: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .expect("pin golden dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "pins"))
        .collect();
    paths.sort();
    let mut totals: BTreeMap<&str, usize> = BTreeMap::new();
    let mut clean = 0usize;
    for path in paths.iter().take(max) {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let trace = parse_trace(&std::fs::read_to_string(path).unwrap()).unwrap();
        let stim = if trace.header.stim.is_empty() {
            Vec::new()
        } else {
            parse_stim(&std::fs::read_to_string(path.with_extension("stim")).unwrap()).unwrap()
        };
        let mut core = CorePins::new(&trace.header.loads, trace.header.reset_vector);
        let got = run(&mut core, trace.frames.len() as u64 - 1, &stim);
        if core.absent_pin_driven {
            println!("{name}: script drives RDY or SO, not 2A03 pins; not compared");
            *totals.entry("absent-pin").or_default() += 1;
            continue;
        }
        let mut classes: BTreeMap<&str, usize> = BTreeMap::new();
        let mut shown = 0;
        let s_low = |e: &PinFrame, g: &PinFrame| (e.ab as u8).wrapping_sub(g.ab as u8) == 0x40 && e.ab >> 8 == g.ab >> 8;
        for (i, (e, g)) in trace.frames.iter().zip(&got).enumerate() {
            // Every differing field is classified, not just the first: a
            // stack push whose address AND byte differ carries two facts.
            let mut fields: Vec<&str> = Vec::new();
            if e.ab != g.ab {
                fields.push(if e.ab >> 8 == 1 && s_low(e, g) {
                    "stack"
                } else if name == "op-ba" && e.ab >> 8 == 0 && s_low(e, g) {
                    "s-leak"
                } else {
                    "other-ab"
                });
            }
            if e.db != g.db {
                fields.push(if !e.rw && !e.clk0 { "wphi1" } else { "data" });
            }
            let g2 = PinFrame { rdy: e.rdy, so: e.so, ab: e.ab, db: e.db, ..*g };
            if let Some(f) = first_difference(e, &g2) {
                fields.push(f);
            }
            let loud = fields.iter().any(|c| *c != "stack" && *c != "s-leak" && *c != "wphi1");
            if loud || (fields.contains(&"wphi1") && shown < 8) {
                // For a write's phi1: the byte the last read served and the
                // byte this write lands in phi2, beside what each die shows.
                let prev_read = trace.frames[..i].iter().rev().find(|f| f.rw && !f.clk0).map(|f| f.db);
                let next = trace.frames.get(i + 1).map(|f| f.db);
                println!(
                    "  {name} h={i} {fields:?}: recorded {} | 2a03 {} | last read {prev_read:02x?} phi2 byte {next:02x?}",
                    line(e),
                    line(g)
                );
                shown += 1;
            }
            for c in fields {
                *classes.entry(c).or_default() += 1;
            }
        }
        if core.clock_stopped {
            println!("  {name}: clk0 STOPPED under the script");
            *classes.entry("clock-stopped").or_default() += 1;
        }
        if classes.is_empty() {
            clean += 1;
        } else {
            println!("{name}: {classes:?}");
        }
        for (k, v) in classes {
            *totals.entry(k).or_default() += v;
        }
    }
    println!("clean traces: {clean}; totals per class: {totals:?}");
}
