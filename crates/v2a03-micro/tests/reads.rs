//! The $4015 read at the pins, across the frame IRQ flag's rise and a
//! length counter's expiry, which the fourth step brings together: the
//! whole chip (`Rung`, on a bus, as the console has it) against rung 0
//! through `CorePins`, the way the stall gate compares, on programs
//! that write $4017 and read $4015 a chosen number of cycles later,
//! stepping the read one cycle at a time across the event on both APU
//! parities. blargg's apu_test 4, 5 and 6 read the status register a
//! cycle either side of such events, and through the console they had
//! read a cycle late.
//!
//! What it found: the chip latches the status at the end of the read's
//! phi2, a half-step after the core asks its bus, so a flag rising in
//! that half-step is read as set; and an internal register read leaves
//! the external data bus to whatever drives it (the harness's memory
//! here), so the pin frames of such a read show that byte, not the
//! status. The Rung answers the pins from its outer bus and the latch
//! through `MicroBus::read_late`, one half-step on.

use v2a03_micro::rung::Rung;
use v2a03_sim::lockstep::classify;
use v2a03_sim::pins::CorePins;
use v6502_micro::machine::MicroBus;
use v6502_pins::{line, Load, PinEngine};

/// A flat bus of the loads: the Rung on a bus answers $4015 itself
/// (`Rung::new`'s flat image would hand back the program's own write).
struct Flat(Vec<u8>);

impl MicroBus for Flat {
    fn read(&mut self, a: u16) -> u8 {
        self.0[a as usize]
    }
    fn write(&mut self, a: u16, v: u8) {
        self.0[a as usize] = v;
    }
}

fn flat(loads: &[Load]) -> Flat {
    let mut m = vec![0u8; 0x10000];
    for l in loads {
        m[l.org as usize..l.org as usize + l.bytes.len()].copy_from_slice(&l.bytes);
    }
    m[0xfffc] = 0x00;
    m[0xfffd] = 0x80;
    Flat(m)
}

fn w(reg: u8, v: u8) -> [u8; 5] {
    [0xa9, v, 0x8d, reg, 0x40]
}

/// $4017 <- $40 (flag clear), the notes, $4017 <- mode, `nops` NOPs,
/// LDA $4015, STA $00, spin. `odd` adds a three-cycle instruction before
/// the mode write.
fn program(mode: u8, odd: bool, nops: usize, clock_first: bool) -> Vec<Load> {
    let mut p = Vec::new();
    p.extend(w(0x17, 0x40));
    p.extend(w(0x15, 0x0f));
    // Every length-bearing channel with a two-half-frame length (index
    // 3) and its halt clear: the squares at constant volume, the
    // triangle with a long linear counter, the noise at constant volume.
    p.extend(w(0x00, 0x10));
    p.extend(w(0x03, 0x18));
    p.extend(w(0x04, 0x10));
    p.extend(w(0x07, 0x18));
    p.extend(w(0x08, 0x7f));
    p.extend(w(0x0b, 0x18));
    p.extend(w(0x0c, 0x10));
    p.extend(w(0x0f, 0x18));
    if clock_first {
        // blargg's 5-len_timing: a mode-1 write clocks every length
        // once at once, so the mode write's first half frame expires
        // them (the second step, not the fourth).
        p.extend(w(0x17, 0xc0));
    }
    if odd {
        p.extend([0x85, 0x01]);
    }
    p.extend(w(0x17, mode));
    p.extend(std::iter::repeat_n(0xea, nops));
    // Two reads four cycles apart, each stored: the second shows whether
    // the first's clear held (the fourth step sets the flag on three
    // consecutive cycles, so an early clear is undone).
    p.extend([0xad, 0x15, 0x40, 0x85, 0x00, 0xad, 0x15, 0x40, 0x85, 0x02]);
    let spin = 0x8000 + p.len() as u16;
    p.extend([0x4c, spin as u8, (spin >> 8) as u8]);
    vec![Load { org: 0x8000, bytes: p }]
}

fn compare(name: &str, loads: &[Load], steps: u64) -> (u16, u16) {
    let mut r0 = CorePins::new(loads, 0x8000);
    r0.power_cycle();
    let s = r0.stack_pointer();
    // Beside the pins: the half-step each engine's frame IRQ flag first
    // rises on (the die's `frame_irq` node, the Rung's APU field).
    let irq_node = r0.har.cpu.engine.netlist().node("frame_irq").unwrap();
    let mut want = vec![r0.pins()];
    let mut die_irq_at = None;
    for h in 0..steps {
        r0.half_step();
        want.push(r0.pins());
        if die_irq_at.is_none() && r0.har.cpu.engine.is_high(irq_node) {
            die_irq_at = Some(h + 1);
        }
    }
    let mut rung = Rung::with_bus(Box::new(flat(loads)), s);
    rung.power_cycle();
    let mut got = vec![rung.pins()];
    let mut rung_irq_at = None;
    for h in 0..steps {
        rung.half_step();
        got.push(rung.pins());
        if rung_irq_at.is_none() && rung.apu.borrow().frame_irq {
            rung_irq_at = Some(h + 1);
        }
    }
    eprintln!("{name}: frame IRQ flag first high after half-step {die_irq_at:?} on rung 0, {rung_irq_at:?} on the Rung");
    let rep = classify(&want, &got, 0, false);
    // SURVEY=1: report the read's pin bytes on both engines instead of
    // holding them, to read the chip's output lag off the whole sweep.
    if std::env::var_os("SURVEY").is_some() {
        let frames = |fs: &[v6502_pins::PinFrame]| -> Vec<u8> { fs.iter().filter(|f| f.rw && f.ab == 0x4015).map(|f| f.db).collect() };
        let stored = |fs: &[v6502_pins::PinFrame]| fs.iter().find(|f| !f.rw && f.clk0 && f.ab == 0x0000).map(|f| f.db);
        let at = want.iter().position(|f| f.rw && f.ab == 0x4015).unwrap();
        eprintln!("{name}: read at h={at}: rung 0 pins {:02x?} stored {:02x?} | Rung pins {:02x?} stored {:02x?}", frames(&want), stored(&want), frames(&got), stored(&got));
        return (stored(&want).unwrap_or(0) as u16, stored(&got).unwrap_or(0) as u16);
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
    // The bytes each core latched from $4015: what its STA $00 and STA $02 wrote.
    let byte = |fs: &[v6502_pins::PinFrame], at: u16| fs.iter().find(|f| !f.rw && f.clk0 && f.ab == at).map(|f| f.db).expect("the store is in the trace");
    ((byte(&want, 0) as u16) << 8 | byte(&want, 2) as u16, (byte(&got, 0) as u16) << 8 | byte(&got, 2) as u16)
}

#[test]
fn the_status_read_lands_as_rung_0_does_across_the_irq_flag_and_a_length_expiry() {
    if !v2a03_netlist::available() || !v2a03_micro::tables::AVAILABLE {
        if std::env::var_os("REQUIRE_NETLIST").is_some() {
            panic!("REQUIRE_NETLIST=1 but extern/visual2a03 is not fetched");
        }
        eprintln!("SKIP: extern/visual2a03 not fetched");
        return;
    }
    // Mode 0: the fourth step, 59,661 half-steps after the write's
    // strobe on the short parity (apu-write-probe), raises the frame
    // IRQ flag and clocks the half frame that expires the two-half-frame
    // length (the die holds a length as one less, and plays through the
    // clock that takes it to zero). Counted in cycles from the STA's
    // write cycle, the read's cycle is the NOPs' two each plus the LDA's
    // fourth; each parity's sweep steps a NOP (two cycles) across the
    // event, and the odd parity's three extra cycles put its reads on
    // the other cycle.
    let mut seen = Vec::new();
    let centre = 29830usize;
    for odd in [false, true] {
        for delta in -1i64..=3 {
            let nops = ((centre as i64 - 4) / 2 + delta) as usize;
            let loads = program(0x00, odd, nops, false);
            let (want, got) = compare(&format!("{} parity, {nops} NOPs", if odd { "odd" } else { "even" }), &loads, (centre as u64 + 160) * 2);
            seen.push((odd, want, got));
            eprintln!("{} parity, {nops} NOPs: rung 0 latched {:02x} then {:02x}, the Rung {:02x} then {:02x}", if odd { "odd" } else { "even" }, want >> 8, want & 0xff, got >> 8, got & 0xff);
        }
    }
    // The second step, with the lengths clocked once by a mode-1 write
    // first (blargg's 5-len_timing shape): every channel's length bit
    // must drop where rung 0's does.
    let centre2 = 14915usize;
    let mut second = Vec::new();
    for odd in [false, true] {
        for delta in -1i64..=2 {
            let nops = ((centre2 as i64 - 4) / 2 + delta) as usize;
            let loads = program(0x00, odd, nops, true);
            let (want, got) = compare(&format!("clocked first, {} parity, {nops} NOPs", if odd { "odd" } else { "even" }), &loads, (centre2 as u64 + 160) * 2);
            second.push((odd, want, got));
            eprintln!("clocked first, {} parity, {nops} NOPs: rung 0 latched {:02x} then {:02x}, the Rung {:02x} then {:02x}", if odd { "odd" } else { "even" }, want >> 8, want & 0xff, got >> 8, got & 0xff);
        }
    }
    for odd in [false, true] {
        let vals: Vec<u8> = second.iter().filter(|s| s.0 == odd).map(|s| (s.1 >> 8) as u8 & 0x0f).collect();
        assert!(vals.iter().any(|&v| v != vals[0]), "second step ({} parity): rung 0 latched the same lengths at every offset {vals:?}; the sweep misses the event", if odd { "odd" } else { "even" });
    }
    // Each parity's sweep must cross the event on rung 0 in both bits
    // (or the sweep is not where it thinks); the Rung agreed at every
    // read, which the classifier above held field for field, the stored
    // byte included.
    for odd in [false, true] {
        for (bit, name) in [(0x40u8, "IRQ flag"), (0x01, "square 0 length"), (0x02, "square 1 length"), (0x04, "triangle length"), (0x08, "noise length")] {
            let vals: Vec<u8> = seen.iter().filter(|s| s.0 == odd).map(|s| (s.1 >> 8) as u8 & bit).collect();
            assert!(vals.iter().any(|&v| v != vals[0]), "{name} ({} parity): rung 0 latched the same bit at every offset {vals:?}; the sweep misses the event", if odd { "odd" } else { "even" });
        }
    }
    // The second read must have seen the flag set again after an early
    // clear somewhere in the sweep, and not after a late one.
    let seconds: Vec<u8> = seen.iter().map(|s| (s.1 & 0xff) as u8 & 0x40).collect();
    assert!(seconds.iter().any(|&v| v != 0) && seconds.contains(&0), "the second reads never showed both a re-set flag and a held clear: {seconds:02x?}");
    eprintln!("status reads: {} sweep points, both parities, two reads each, the Rung latched what rung 0 latched at every one", seen.len());
}

/// $4010 <- $8F (IRQ on, fastest rate), $4012 <- 0 (the sample at
/// $C000), $4013 <- 0 (one byte), $4015 <- $10, then `nops` NOPs, LDA
/// $4015, STA $00, LDA $4015, STA $02, spin: blargg's 7-dmc_basics
/// #19, which expects the byte fetched at once, the bytes-remaining bit
/// clear and the DMC IRQ flag set by the first read.
fn dmc_program(nops: usize, odd: bool) -> Vec<Load> {
    let mut p = Vec::new();
    p.extend(w(0x17, 0x40));
    if odd {
        p.extend([0x85, 0x01]);
    }
    p.extend(w(0x10, 0x8f));
    p.extend(w(0x12, 0x00));
    p.extend(w(0x13, 0x00));
    p.extend(w(0x15, 0x10));
    p.extend(std::iter::repeat_n(0xea, nops));
    p.extend([0xad, 0x15, 0x40, 0x85, 0x00, 0xad, 0x15, 0x40, 0x85, 0x02]);
    let spin = 0x8000 + p.len() as u16;
    p.extend([0x4c, spin as u8, (spin >> 8) as u8]);
    vec![Load { org: 0x8000, bytes: p }, Load { org: 0xc000, bytes: vec![0x5a; 64] }]
}

#[test]
fn a_one_byte_sample_started_on_an_empty_buffer_reads_as_rung_0_reads_it() {
    if !v2a03_netlist::available() || !v2a03_micro::tables::AVAILABLE {
        if std::env::var_os("REQUIRE_NETLIST").is_some() {
            panic!("REQUIRE_NETLIST=1 but extern/visual2a03 is not fetched");
        }
        eprintln!("SKIP: extern/visual2a03 not fetched");
        return;
    }
    let mut firsts = Vec::new();
    for odd in [false, true] {
        for nops in [0usize, 1, 2, 4, 8, 16] {
            let (want, got) = compare(&format!("DMC one-byte start, {} parity, {nops} NOPs", if odd { "odd" } else { "even" }), &dmc_program(nops, odd), 800);
            eprintln!("DMC one-byte start, {} parity, {nops} NOPs: rung 0 latched {:02x} then {:02x}, the Rung {:02x} then {:02x}", if odd { "odd" } else { "even" }, want >> 8, want & 0xff, got >> 8, got & 0xff);
            firsts.push((want >> 8) as u8);
        }
    }
    assert!(firsts.contains(&0x10) && firsts.contains(&0x80), "the sweep must see the byte both unfetched and fetched with the IRQ: {firsts:02x?}");
}
