//! Dump the A3 run's sound as CSV for figures: half_step, sq0_out
//! code, mixed AD1 level. Same program, same harness as the test.

use v2a03_sim::harness::Harness;
use v2a03_sim::{mixer, Cpu};

fn main() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tools/golden-trace/program-a3.json"
    ))
    .expect("program-a3.json");
    let b0 = text.find("\"bytes\": [").expect("bytes");
    let b1 = b0 + text[b0..].find(']').expect("close");
    let bytes: Vec<u8> = text[b0 + 10..b1]
        .split(',')
        .map(|s| s.trim().parse().expect("byte"))
        .collect();

    let mut h = Harness::new(Cpu::power_on());
    h.load(32768, &bytes, 32768);
    let nl = h.cpu.engine.netlist().clone();
    let sq0: Vec<_> = (0..4)
        .map(|i| nl.node(&format!("sq0_out{i}")).expect("sq0_out"))
        .collect();
    println!("half_step,sq0,ad1");
    for step in 0..2000u32 {
        h.half_step();
        let code: u8 = sq0
            .iter()
            .enumerate()
            .map(|(i, &n)| (h.cpu.engine.is_high(n) as u8) << i)
            .sum();
        println!("{step},{code},{}", mixer::ad1(code, 0));
    }
}
