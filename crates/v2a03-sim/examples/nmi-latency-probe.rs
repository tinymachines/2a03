//! The 2A03 core's NMI pad, timed at the master clock: the pad driven
//! low at every master pulse across a NOP, and the pulse at which the
//! NMI vector is first read, against the fetch that follows. The 6502's
//! rung 0 takes an edge present as a cycle's phi1 begins at the next
//! fetch (tinymachines/6502, brk-nmi-probe); this asks whether the
//! 2A03's own pad path adds anything at the grain the console runs at.
//!
//!     cargo run --release -p v2a03-sim --example nmi-latency-probe
//!
//! Twelve master pulses make one clk0 phase here (the reference's
//! divider), so a CPU cycle is twenty-four; a PPU dot would be four.

use v2a03_sim::pins::CorePins;
use v6502_pins::{Load, PinEngine};

fn main() {
    let mut prog = vec![0xea; 8];
    prog.extend([0xea; 8]);
    let here = 0x0200 + prog.len() as u16;
    prog.extend([0x4c, here as u8, (here >> 8) as u8]);
    let loads = vec![
        Load { org: 0x0200, bytes: prog },
        Load { org: 0x0340, bytes: vec![0xea, 0x40] },
        Load { org: 0xfffa, bytes: vec![0x40, 0x03, 0x00, 0x02, 0x40, 0x03] },
    ];
    // Where the fetch of $0204 begins, in master pulses from power_cycle.
    let mut cp = CorePins::new(&loads, 0x0200);
    cp.power_cycle();
    let nl = cp.har.cpu.engine.netlist().clone();
    let n = |name: &str| nl.node(name).unwrap_or_else(|| panic!("node {name}"));
    let (sync, nmi, clk0) = (n("sync"), n("nmi"), n("clk0"));
    let ab: Vec<_> = (0..16).map(|i| n(&format!("ab{i}"))).collect();
    let bits = |cp: &CorePins| -> u16 { ab.iter().enumerate().map(|(i, &b)| (cp.har.cpu.engine.is_high(b) as u16) << i).sum() };
    let mut pulses = 0u64;
    let fetch = loop {
        let flipped = cp.har.master_pulse();
        pulses += 1;
        if flipped && cp.har.cpu.engine.is_high(sync) && bits(&cp) == 0x0204 && !cp.har.cpu.engine.is_high(clk0) {
            break pulses;
        }
    };
    println!("fetch of $0204 announced at master pulse {fetch} (clk0 low, sync high)");
    println!("  nmi low from   vector read at   (pulses after the fetch)");
    for off in -30i64..=30 {
        let mut cp = CorePins::new(&loads, 0x0200);
        cp.power_cycle();
        let at = (fetch as i64 + off) as u64;
        let mut pulses = 0u64;
        let mut vec_at = None;
        while pulses < at + 400 {
            if pulses == at {
                cp.har.cpu.engine.drive_low(nmi);
            }
            let flipped = cp.har.master_pulse();
            pulses += 1;
            if flipped && bits(&cp) == 0xfffa && cp.har.cpu.engine.is_high(n("rw")) {
                vec_at = Some(pulses as i64 - fetch as i64);
                break;
            }
        }
        println!("  {off:>5}          {}", vec_at.map_or("none".to_string(), |v| format!("{v:+}")));
    }
}
