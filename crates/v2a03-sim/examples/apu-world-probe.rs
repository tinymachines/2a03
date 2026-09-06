//! A register program (hex in PROG, placed at $8000 and run from there)
//! on rung 0, printing every change of a square's nodes inside a window
//! of half-steps from the pin contract's h=0: the length counter, the
//! envelope's counter and divider, the sequencer step, the timer, the
//! sweep's divider, and the output code, plus the frame strobes. What a
//! code the fast rung gets a half-step wrong is made of.
//!
//!   PROG=<hex> FROM=<h> TO=<h> cargo run --release -p v2a03-sim --example apu-world-probe [-- sq1]

use v2a03_sim::harness::Harness;
use v2a03_sim::pins::ALIGN_PHASES;
use v2a03_sim::Cpu;

fn main() {
    let hex = std::env::var("PROG").expect("PROG=<hex bytes>");
    let prog: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
    let from: i64 = std::env::var("FROM").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let to: i64 = std::env::var("TO").ok().and_then(|v| v.parse().ok()).unwrap_or(1000);
    let sq = if std::env::args().nth(1).as_deref() == Some("sq1") { "sq1" } else { "sq0" };
    // PINS=1 sets the chip up through the pin engine, the way the APU
    // gate does (CorePins::new and power_cycle); otherwise the bare
    // harness from power_on.
    let sample: Vec<u8> = (0..33u8).map(|i| i.wrapping_mul(0x5b) ^ 0xa5).collect();
    let mut cp;
    let h: &mut Harness = if std::env::var_os("PINS").is_some() {
        let loads = vec![v6502_pins::Load { org: 0x8000, bytes: prog.clone() }, v6502_pins::Load { org: 0xc000, bytes: sample.clone() }];
        cp = v2a03_sim::pins::CorePins::new(&loads, 0x8000);
        v6502_pins::PinEngine::power_cycle(&mut cp);
        &mut cp.har
    } else {
        cp = v2a03_sim::pins::CorePins::new(&[], 0x8000);
        let mut hh = Harness::new(Cpu::power_on());
        hh.load(0x8000, &prog, 0x8000);
        if std::env::var_os("SAMPLE").is_some() {
            hh.memory[0xc000..0xc000 + 33].copy_from_slice(&sample);
        }
        cp.har = hh;
        &mut cp.har
    };
    let pins = std::env::var_os("PINS").is_some();
    let nl = h.cpu.engine.netlist().clone();
    let n = |name: &str| nl.node(name).unwrap_or_else(|| panic!("node {name}"));
    let group = |prefix: &str, count: usize| -> Vec<halfphi::NodeId> { (0..count).map(|i| n(&format!("{prefix}{i}"))).collect() };
    let buses: Vec<(&str, Vec<halfphi::NodeId>)> = vec![
        ("len", group(&format!("{sq}_len"), 8)),
        ("envc", group(&format!("{sq}_envc"), 4)),
        ("envt", group(&format!("{sq}_envt"), 4)),
        ("envp", group(&format!("{sq}_envp"), 4)),
        ("c", group(&format!("{sq}_c"), 3)),
        ("swpdiv", group(&format!("{sq}_swpt"), 3)),
        ("out", group(&format!("{sq}_out"), 4)),
    ];
    let singles: Vec<(&str, halfphi::NodeId)> = vec![
        ("quarter(low)", n("frm_/quarter")), ("half(low)", n("frm_/half")), ("envmode", n(&format!("{sq}_envmode"))), ("silence", n(&format!("{sq}_silence"))),
    ];
    let bits = |h: &Harness, ns: &[halfphi::NodeId]| -> u32 { ns.iter().enumerate().map(|(i, &nd)| (h.cpu.engine.is_high(nd) as u32) << i).sum() };
    let mut prev_b: Vec<u32> = buses.iter().map(|(_, b)| bits(h, b)).collect();
    let mut prev_s: Vec<bool> = singles.iter().map(|(_, s)| h.cpu.engine.is_high(*s)).collect();
    let already = if pins { ALIGN_PHASES as i64 } else { 0 };
    for step in 0..(to + ALIGN_PHASES as i64 + 1 - already) as usize {
        h.half_step();
        let hh = step as i64 + 1 + already - ALIGN_PHASES as i64;
        let mut line = String::new();
        for (i, (name, b)) in buses.iter().enumerate() {
            let v = bits(h, b);
            if v != prev_b[i] {
                line.push_str(&format!(" {name} {}->{}", prev_b[i], v));
                prev_b[i] = v;
            }
        }
        for (i, (name, s)) in singles.iter().enumerate() {
            let v = h.cpu.engine.is_high(*s);
            if v != prev_s[i] {
                line.push_str(&format!(" {name}->{}", v as u8));
                prev_s[i] = v;
            }
        }
        if !line.is_empty() && hh >= from {
            println!("h={hh}:{line}");
        }
    }
}
