//! The cross-chip comparison: a recorded 6502 pin trace beside this
//! chip's core run through the same script, every differing field
//! classified by a named rule before anything is asserted. The rules are
//! the ones `examples/lockstep-probe.rs` measured on 2026-09-05 over all
//! 274 recorded traces; a difference no rule names is LOUD and a gate
//! fails on it.
//!
//! - `stack`: the address differs, both in page 1, by exactly the two
//!   dies' stack-pointer offset in the low byte. Silicon leaves S
//!   undefined at power-on; the 6502's simulation settles it at $00 and
//!   the 2A03's at $C0, so after the reset's three decrements one core
//!   pushes at $01FD and the other at $01BD, and every stack address
//!   differs by $40 for the rest of time. The offset is derived from the
//!   traces, not typed (`stack_offset`).
//! - `s-leak`: the same offset in page 0, allowed only where the caller
//!   says the program copies S into an index register (TSX, whose trace
//!   then runs a `NOP zp,X` indexed by it).
//! - `write-phi1`: the data byte differs on the phi1 half of a write
//!   cycle. No bus service happens there (a write is serviced as clk0
//!   rises, a read as it falls) and the world drives nothing: the 6502's
//!   pins show the last byte read, the 2A03's show a value that is
//!   neither that nor reliably the byte about to be written. Nothing
//!   crosses the pins in that half; the write's own phi2 byte is
//!   compared as `data` and must agree.
//! - `data`: a serviced data byte differs. Loud, unless the caller's
//!   expected list names it (the decimal fixtures).
//! - any other field by its name: loud.

use std::collections::BTreeMap;

use v6502_pins::PinFrame;

pub const STACK: &str = "stack";
pub const S_LEAK: &str = "s-leak";
pub const WRITE_PHI1: &str = "write-phi1";
pub const DATA: &str = "data";

/// A frame carrying at least one loud class, in full.
#[derive(Clone, Debug)]
pub struct Loud {
    pub h: usize,
    pub classes: Vec<&'static str>,
    pub expected: PinFrame,
    pub got: PinFrame,
}

#[derive(Default, Debug)]
pub struct Report {
    pub counts: BTreeMap<&'static str, usize>,
    pub loud: Vec<Loud>,
    pub frames: usize,
}

impl Report {
    pub fn count(&self, class: &str) -> usize {
        self.counts.get(class).copied().unwrap_or(0)
    }
}

/// The two streams' stack-pointer offset, read off their first stack
/// access: the recording's low byte minus this chip's, at the same h.
/// None if the recording never touches page 1, or the chip is elsewhere
/// at that moment (which the caller then sees as a loud `ab`).
pub fn stack_offset(expected: &[PinFrame], got: &[PinFrame]) -> Option<u8> {
    let i = expected.iter().position(|f| f.ab >> 8 == 1)?;
    let g = got.get(i)?;
    (g.ab >> 8 == 1).then(|| (expected[i].ab as u8).wrapping_sub(g.ab as u8))
}

pub fn classify(expected: &[PinFrame], got: &[PinFrame], stack_offset: u8, s_leak_ok: bool) -> Report {
    let mut r = Report { frames: expected.len().min(got.len()), ..Report::default() };
    let same_page_offset = |e: &PinFrame, g: &PinFrame| e.ab >> 8 == g.ab >> 8 && (e.ab as u8).wrapping_sub(g.ab as u8) == stack_offset;
    for (i, (e, g)) in expected.iter().zip(got).enumerate() {
        let mut classes: Vec<&'static str> = Vec::new();
        if e.ab != g.ab {
            classes.push(if e.ab >> 8 == 1 && same_page_offset(e, g) {
                STACK
            } else if s_leak_ok && e.ab >> 8 == 0 && same_page_offset(e, g) {
                S_LEAK
            } else {
                "ab"
            });
        }
        if e.db != g.db {
            classes.push(if !e.rw && !e.clk0 { WRITE_PHI1 } else { DATA });
        }
        for (name, a, b) in [
            ("h", e.h == g.h, true),
            ("clk0", e.clk0, g.clk0),
            ("rw", e.rw, g.rw),
            ("sync", e.sync, g.sync),
            ("res", e.res, g.res),
            ("irq", e.irq, g.irq),
            ("nmi", e.nmi, g.nmi),
            ("rdy", e.rdy, g.rdy),
            ("so", e.so, g.so),
        ] {
            if a != b {
                classes.push(name);
            }
        }
        if classes.is_empty() {
            continue;
        }
        for c in &classes {
            *r.counts.entry(c).or_default() += 1;
        }
        if classes.iter().any(|c| !matches!(*c, STACK | S_LEAK | WRITE_PHI1)) {
            r.loud.push(Loud { h: i, classes, expected: *e, got: *g });
        }
    }
    if expected.len() != got.len() {
        *r.counts.entry("length").or_default() += 1;
        let h = r.frames;
        r.loud.push(Loud {
            h,
            classes: vec!["length"],
            expected: expected.get(h).copied().unwrap_or_default(),
            got: got.get(h).copied().unwrap_or_default(),
        });
    }
    r
}
