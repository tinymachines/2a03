//! N3 step 5's gate: the stalls. The whole chip at the pins (`Rung`: the
//! core, the APU and the DMA units) against rung 0 through `CorePins`,
//! every frame, every field, over programs that stall: a $4014 sprite
//! DMA at each of the two write alignments, and the DMC fetching its
//! sample. The one class allowed is step 1's write-phi1 byte
//! (`v2a03_sim::lockstep`); RDY is compared like any other field, so a
//! stall one half-step short or long, or a DMA byte on the wrong frame,
//! fails by name.
//!
//! SKIPS by name without the die data; REQUIRE_NETLIST=1 insists.
//! `MUTATE=1` drops the DMA's last pair and must go red.

use v2a03_micro::rung::Rung;
use v2a03_sim::lockstep::classify;
use v2a03_sim::pins::CorePins;
use v6502_pins::{line, run, Load, PinEngine};

fn w(reg: u8, v: u8) -> [u8; 5] {
    [0xa9, v, 0x8d, reg, 0x40]
}

fn dma_program(shift: bool) -> Vec<Load> {
    let mut prog = Vec::new();
    if shift {
        prog.extend([0xa5u8, 0x00]);
    }
    prog.extend(w(0x14, 0x02));
    prog.extend([0xea, 0xea, 0xea, 0xea]);
    let spin = 0x8000 + prog.len() as u16;
    prog.extend([0x4c, spin as u8, (spin >> 8) as u8]);
    let page: Vec<u8> = (0..=255u8).map(|i| i.wrapping_mul(3)).collect();
    vec![Load { org: 0x8000, bytes: prog }, Load { org: 0x0200, bytes: page }]
}

fn dmc_program(odd: bool) -> Vec<Load> {
    let mut prog = Vec::new();
    if odd {
        // A three-cycle instruction first: the enable lands on the other
        // APU cycle parity (an APU cycle is two CPU cycles). BIT, not a
        // store: a store of the power-on A would compare two dies'
        // undefined registers.
        prog.extend([0x24, 0x00]);
    }
    for (r, v) in [(0x10u8, 0x4fu8), (0x11, 0x20), (0x12, 0x00), (0x13, 0x02), (0x15, 0x1f)] {
        prog.extend(w(r, v));
    }
    let spin = 0x8000 + prog.len() as u16;
    prog.extend([0x4c, spin as u8, (spin >> 8) as u8]);
    let sample: Vec<u8> = (0..33u8).map(|i| i.wrapping_mul(0x5b) ^ 0xa5).collect();
    vec![Load { org: 0x8000, bytes: prog }, Load { org: 0xc000, bytes: sample }]
}

fn compare(name: &str, loads: &[Load], steps: u64) {
    let mut r0 = CorePins::new(loads, 0x8000);
    r0.power_cycle();
    let s = r0.stack_pointer();
    let want = run(&mut r0, steps, &[]);
    let mut rung = Rung::new(loads, 0x8000, s);
    if std::env::var("MUTATE").is_ok_and(|v| v == "1") {
        rung.dma_pairs = 255;
    }
    let got = run(&mut rung, steps, &[]);
    let rep = classify(&want, &got, 0, false);
    let stalled = want.iter().filter(|f| !f.rdy).count();
    if let Some(l) = rep.loud.first() {
        let lo = l.h.saturating_sub(6);
        let hi = (l.h + 6).min(want.len() - 1);
        let mut s = String::new();
        for i in lo..=hi {
            s.push_str(&format!("    h={i:>5} rung0 {} | rung {}{}\n", line(&want[i]), line(&got[i]), if i == l.h { "  <-- first" } else { "" }));
        }
        panic!("{name}: {} unnamed difference(s), first at h={} {:?}\n{s}", rep.loud.len(), l.h, l.classes);
    }
    eprintln!("{name}: {} frames identical in every field but {} write-phi1 bytes; RDY low on {stalled} of them", want.len(), rep.count("write-phi1"));
}

#[test]
fn the_sprite_dma_and_the_dmc_fetch_stall_the_core_as_rung_0_does() {
    if !v2a03_netlist::available() || !v2a03_micro::tables::AVAILABLE {
        if std::env::var_os("REQUIRE_NETLIST").is_some() {
            panic!("REQUIRE_NETLIST=1 but extern/visual2a03 is not fetched");
        }
        eprintln!("SKIP: extern/visual2a03 not fetched");
        return;
    }
    compare("sprite DMA, write on an even cycle", &dma_program(false), 1200);
    compare("sprite DMA, write on an odd cycle", &dma_program(true), 1200);
    compare("DMC fetches", &dmc_program(false), 4200);
    compare("DMC fetches, enabled on an odd cycle", &dmc_program(true), 4200);
}
