//! N3 step 5's measurement: the two stalls as the pin contract sees them.
//! Rung 0 runs through `CorePins` (so h is the contract's h and `rdy` the
//! internal RDY node), and the frames around each stall are printed with
//! the DMA units' own nodes beside them: `spr_dma_/rdy`, `pcm_dma_/rdy`,
//! `spr_addr`, `pcm_dma_active`, `pcm_rd_active`.
//!
//!   cargo run --release -p v2a03-sim --example stall-probe -- [dma|dmc]

use v2a03_sim::pins::CorePins;
use v6502_pins::{line, Load, PinEngine};

fn w(reg: u8, v: u8) -> [u8; 5] {
    [0xa9, v, 0x8d, reg, 0x40]
}

fn show(core: &CorePins, h: u64, extra: &[(&str, usize)]) -> String {
    let mut s = line(&core.pins());
    let nl = core.har.cpu.engine.netlist().clone();
    for (name, n) in extra {
        let v: u32 = if *n == 1 {
            core.har.cpu.engine.is_high(nl.node(name).unwrap()) as u32
        } else {
            (0..*n).map(|i| (core.har.cpu.engine.is_high(nl.node(&format!("{name}{i}")).unwrap()) as u32) << i).sum()
        };
        s.push_str(&format!(" {name}={v:x}"));
    }
    let _ = h;
    s
}

fn dma() {
    for shift in [false, true] {
        let mut prog = Vec::new();
        if shift {
            prog.extend([0xa5u8, 0x00]);
        }
        prog.extend(w(0x14, 0x02));
        prog.extend([0xea, 0xea, 0xea, 0xea]);
        let spin = 0x8000 + prog.len() as u16;
        prog.extend([0x4c, spin as u8, (spin >> 8) as u8]);
        let mut mem = vec![0u8; 256];
        for (i, b) in mem.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(3);
        }
        let loads = vec![Load { org: 0x8000, bytes: prog }, Load { org: 0x0200, bytes: mem }];
        let mut core = CorePins::new(&loads, 0x8000);
        core.power_cycle();
        let extra = [("spr_dma_/rdy", 1usize), ("spr_addr", 16), ("RnWstretched", 1)];
        println!("## sprite DMA, shift={shift}: frames from the $4014 write to the stall's end (contract h)");
        let mut strobe = None;
        let mut low_from = None;
        let mut low_to = None;
        let mut lines = Vec::new();
        for h in 1..=1120u64 {
            core.half_step();
            let f = core.pins();
            if f.ab == 0x4014 && !f.rw && f.clk0 {
                strobe = Some(h);
            }
            if !f.rdy && low_from.is_none() {
                low_from = Some(h);
            }
            if f.rdy && low_from.is_some() && low_to.is_none() {
                low_to = Some(h);
            }
            let near_start = strobe.is_some_and(|s| h >= s.saturating_sub(4) && h <= s + 14);
            let near_end = low_to.is_some_and(|t| h >= t - 8 && h <= t + 6) || (low_from.is_some() && low_to.is_none() && h > 1000);
            if near_start || near_end {
                lines.push(format!("  h={h:>5} {}", show(&core, h, &extra)));
            }
        }
        for l in lines {
            println!("{l}");
        }
        println!("  strobe h={strobe:?}; rdy low from h={low_from:?} to h={low_to:?} ({} half-steps)", low_to.unwrap() - low_from.unwrap());
    }
}

fn dmc() {
    let mut prog = Vec::new();
    for (r, v) in [(0x10u8, 0x4fu8), (0x11, 0x20), (0x12, 0x00), (0x13, 0x02), (0x15, 0x1f)] {
        prog.extend(w(r, v));
    }
    let spin = 0x8000 + prog.len() as u16;
    prog.extend([0x4c, spin as u8, (spin >> 8) as u8]);
    let sample: Vec<u8> = (0..33u8).map(|i| i.wrapping_mul(0x5b) ^ 0xa5).collect();
    let loads = vec![Load { org: 0x8000, bytes: prog }, Load { org: 0xc000, bytes: sample }];
    let mut core = CorePins::new(&loads, 0x8000);
    core.power_cycle();
    let extra = [("pcm_dma_/rdy", 1usize), ("pcm_dma_active", 1), ("pcm_rd_active", 1), ("pcm_bits", 3)];
    println!("## DMC: frames around the first three fetches (contract h)");
    let mut prev_rdy = true;
    let mut stalls = 0;
    let mut window = 0;
    for h in 1..=4200u64 {
        core.half_step();
        let f = core.pins();
        if !f.rdy && prev_rdy {
            stalls += 1;
            window = 10;
            println!("  stall {stalls} begins:");
            // print the two frames before as well
        }
        if window > 0 || (!f.rdy) {
            println!("  h={h:>5} {}", show(&core, h, &extra));
            if f.rdy {
                window -= 1;
            }
        }
        prev_rdy = f.rdy;
        if stalls == 3 && window == 0 && f.rdy {
            break;
        }
    }
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_default();
    if which.is_empty() || which == "dma" {
        dma();
    }
    if which.is_empty() || which == "dmc" {
        dmc();
    }
}
