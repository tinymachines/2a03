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

/// The DMC playing at its fastest rate (a byte every 432 CPU cycles)
/// while a $4014 sprite DMA runs (512 cycles): at least one sample fetch
/// lands inside the DMA. `gap` NOPs between the enable and the $4014
/// write move where.
fn dmc_in_dma_program(gap: usize, odd: bool) -> Vec<Load> {
    dmc_in_dma_program_at(gap, odd, 0x00)
}

fn dmc_in_dma_program_at(gap: usize, odd: bool, addr: u8) -> Vec<Load> {
    let mut prog = Vec::new();
    if odd {
        prog.extend([0x24, 0x00]);
    }
    for (r, v) in [(0x10u8, 0x0fu8), (0x11, 0x20), (0x12, addr), (0x13, 0x02), (0x15, 0x1f)] {
        prog.extend(w(r, v));
    }
    prog.extend(std::iter::repeat_n(0xea, gap));
    prog.extend(w(0x14, 0x02));
    let spin = 0x8000 + prog.len() as u16;
    prog.extend([0x4c, spin as u8, (spin >> 8) as u8]);
    let sample: Vec<u8> = (0..33u8).map(|i| i.wrapping_mul(0x5b) ^ 0xa5).collect();
    let page: Vec<u8> = (0..=255u8).map(|i| i.wrapping_mul(3)).collect();
    vec![Load { org: 0x8000, bytes: prog }, Load { org: 0xc000 + 64 * addr as u16, bytes: sample }, Load { org: 0x0200, bytes: page }]
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
    // Its own variable: the APU reads MUTATE itself (a fitted phase), and
    // an aimed mutation must be red for one reason.
    if std::env::var("MUTATE_COLLISION").is_ok_and(|v| v == "1") {
        rung.collision_pause = false;
    }
    let mut got = run(&mut rung, steps, &[]);
    // A DMC read inside a sprite DMA: the rung reads the DMC's documented
    // address; rung 0 reads elsewhere (its own finding, docs/n3-report.md,
    // for the bench). Those two frames are the one named class here:
    // counted, printed, and the comparison holds everything else on them.
    let mut collision_reads = Vec::new();
    for i in 0..want.len().min(got.len()) {
        let (e, g) = (want[i], got[i]);
        if !e.rdy && !g.rdy && e.rw && g.rw && e.ab != g.ab && (0xc000..=0xffff).contains(&g.ab) && e.ab >> 12 == 8 {
            collision_reads.push(format!("h={i} rung reads {:04x}, rung 0 read {:04x}", g.ab, e.ab));
            got[i].ab = e.ab;
            got[i].db = e.db;
        }
    }
    if !collision_reads.is_empty() {
        eprintln!("  DMC reads inside the DMA, the address rung 0's own finding: {}", collision_reads.join("; "));
    }
    let rep = classify(&want, &got, 0, false);
    let stalled = want.iter().filter(|f| !f.rdy).count();
    if let Ok(r) = std::env::var("DUMP") {
        let (a, b) = r.split_once("..").unwrap();
        let (a, b): (usize, usize) = (a.parse().unwrap(), b.parse().unwrap());
        for i in a..b.min(want.len()) {
            eprintln!("    h={i:>5} rung0 {} | rung {}", line(&want[i]), line(&got[i]));
        }
    }
    if std::env::var_os("SAMPLE_READS").is_some() {
        // Reads on a get frame while RDY is low that are neither the DMA's
        // page nor the core's own program: the DMC's, wherever it went.
        let odd_reads = |v: &[v6502_pins::PinFrame]| -> Vec<String> {
            v.iter().filter(|f| !f.rdy && f.rw && !f.clk0 && f.ab != 0x2004 && (f.ab >> 8) != 0x02 && !(0x81c0..0x8300).contains(&f.ab)).map(|f| format!("{}:{:04x}={:02x}", f.h, f.ab, f.db)).collect()
        };
        eprintln!("  rung 0 stalled reads off the page (frame:addr=byte): {}", odd_reads(&want).join(" "));
        eprintln!("  rung   stalled reads off the page (frame:addr=byte): {}", odd_reads(&got).join(" "));
    }
    if std::env::var_os("RDY_RUNS").is_some() {
        // The runs of RDY low on rung 0: (first frame, length).
        let mut runs = Vec::new();
        let mut i = 0;
        while i < want.len() {
            if !want[i].rdy {
                let s = i;
                while i < want.len() && !want[i].rdy {
                    i += 1;
                }
                runs.push((s, i - s));
            } else {
                i += 1;
            }
        }
        eprintln!("  RDY low runs (frame, length): {runs:?}");
    }
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

/// A DMC fetch landing inside a sprite DMA, at five places against the
/// DMA and two sample addresses: rung 0 pauses the DMA for two cycles
/// (the sample read on the get cycle the DMA was about to use, the
/// core's held cycle after it) and resumes with the pair that was due
/// on the next get frame, RDY low throughout, and the rung does the
/// same frame for frame. The one named class is the address the sample
/// is read from: rung 0's DMC address register is disturbed by the
/// collision (the read lands in $8000.. and stays there), which the
/// documented part and every game that plays samples across its sprite
/// DMA say the part does not do; recorded in docs/n3-report.md for the
/// bench, the rung keeping the documented address. `MUTATE_COLLISION=1`
/// lets the fetch take no cycles and must go red (its own variable,
/// since the APU reads `MUTATE` for a fitted phase of its own).
#[test]
fn a_dmc_fetch_inside_the_sprite_dma_pauses_it_as_rung_0_does() {
    if !v2a03_netlist::available() || !v2a03_micro::tables::AVAILABLE {
        if std::env::var_os("REQUIRE_NETLIST").is_some() {
            panic!("REQUIRE_NETLIST=1 but extern/visual2a03 is not fetched");
        }
        eprintln!("SKIP: extern/visual2a03 not fetched");
        return;
    }
    for (gap, odd, addr) in [(440usize, false, 0u8), (440, true, 0), (600, false, 3), (660, true, 0), (676, true, 0)] {
        let name = format!("DMC fetch inside sprite DMA, gap {gap}, odd {odd}, sample at {:04x}", 0xc000 + 64 * addr as u16);
        compare(&name, &dmc_in_dma_program_at(gap, odd, addr), 4200);
    }
}
