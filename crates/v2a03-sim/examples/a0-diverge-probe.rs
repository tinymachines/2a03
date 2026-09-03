//! The measurement the A0 exemption (if any) is written from: replay
//! the golden and report every node that EVER diverges, with its first
//! and last divergent state and the count. Run before authoring
//! anything about what agrees.

use v2a03_sim::Cpu;

fn main() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tools/golden-trace/golden-2a03.txt"
    );
    let golden = std::fs::read_to_string(path).expect("golden (tools/golden-trace/gen.js)");
    let mut lines = golden.lines();
    let header = lines.next().expect("header");
    assert!(header.starts_with("2a03 golden:"), "not a 2a03 golden: {header}");

    let mut cpu = Cpu::power_on();
    let nl = cpu.engine.netlist().clone();
    struct Div {
        first: usize,
        last: usize,
        count: usize,
    }
    let mut divs: std::collections::BTreeMap<usize, Div> = Default::default();
    let mut states = 0usize;
    for (step, want) in lines.enumerate() {
        if step > 0 {
            cpu.half_step();
        }
        let got = cpu.state_line();
        assert_eq!(got.len(), want.len(), "node count differs at step {step}");
        for (i, (a, b)) in got.bytes().zip(want.bytes()).enumerate() {
            if a != b {
                let d = divs.entry(i).or_insert(Div { first: step, last: step, count: 0 });
                d.last = step;
                d.count += 1;
            }
        }
        states += 1;
    }
    println!("{states} states compared; {} nodes ever diverge", divs.len());
    for (i, d) in &divs {
        println!(
            "node {i} ({}): first {} last {} count {}",
            nl.name_of(*i as halfphi::NodeId).unwrap_or("(unnamed)"),
            d.first,
            d.last,
            d.count
        );
    }
}
