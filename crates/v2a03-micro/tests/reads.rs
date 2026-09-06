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
fn program(mode: u8, odd: bool, nops: usize) -> Vec<Load> {
    let mut p = Vec::new();
    p.extend(w(0x17, 0x40));
    p.extend(w(0x15, 0x01));
    p.extend(w(0x00, 0x10)); // constant volume, the length counting
    p.extend(w(0x03, 0x18)); // length index 3: two half frames
    if odd {
        p.extend([0x85, 0x01]);
    }
    p.extend(w(0x17, mode));
    p.extend(std::iter::repeat_n(0xea, nops));
    p.extend([0xad, 0x15, 0x40, 0x85, 0x00]);
    let spin = 0x8000 + p.len() as u16;
    p.extend([0x4c, spin as u8, (spin >> 8) as u8]);
    vec![Load { org: 0x8000, bytes: p }]
}

fn compare(name: &str, loads: &[Load], steps: u64) -> (u8, u8) {
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
        return (stored(&want).unwrap_or(0), stored(&got).unwrap_or(0));
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
    // The byte each core latched from $4015: what its STA $00 wrote.
    let byte = |fs: &[v6502_pins::PinFrame]| fs.iter().find(|f| !f.rw && f.clk0 && f.ab == 0x0000).map(|f| f.db).expect("the store is in the trace");
    (byte(&want), byte(&got))
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
        for delta in -1i64..=2 {
            let nops = ((centre as i64 - 4) / 2 + delta) as usize;
            let loads = program(0x00, odd, nops);
            let (want, got) = compare(&format!("{} parity, {nops} NOPs", if odd { "odd" } else { "even" }), &loads, (centre as u64 + 40) * 2);
            seen.push((odd, want, got));
            eprintln!("{} parity, {nops} NOPs: rung 0 latched {want:02x}, the Rung {got:02x}", if odd { "odd" } else { "even" });
        }
    }
    // Each parity's sweep must cross the event on rung 0 in both bits
    // (or the sweep is not where it thinks); the Rung agreed at every
    // read, which the classifier above held field for field, the stored
    // byte included.
    for odd in [false, true] {
        for (bit, name) in [(0x40u8, "IRQ flag"), (0x01, "length")] {
            let vals: Vec<u8> = seen.iter().filter(|s| s.0 == odd).map(|s| s.1 & bit).collect();
            assert!(vals.iter().any(|&v| v != vals[0]), "{name} ({} parity): rung 0 latched the same bit at every offset {vals:?}; the sweep misses the event", if odd { "odd" } else { "even" });
        }
    }
    eprintln!("status reads: {} sweep points, both parities, the Rung latched what rung 0 latched at every one", seen.len());
}
