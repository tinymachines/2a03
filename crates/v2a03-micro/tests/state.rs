//! Saved states: the chip stopped anywhere and started again there runs
//! on exactly as if it had never stopped. A program keeps every unit
//! busy (the squares with a sweep, the triangle, the noise, the frame
//! counter's IRQ, the DMC fetching at its fastest rate and a sprite DMA
//! it collides with, then the loop reading $4015), and a state is taken
//! every few dozen half-steps through all of it. Each one goes through
//! the byte form (postcard) into a fresh chip on a copy of the world,
//! and the two run side by side: every pin, every half-step, and the
//! whole state again at the end. States are taken where a CPU cycle
//! ends, as `Rung::save_state` requires.
//!
//! MUTATE_STATE=1 leaves the DMC fetch in flight out of a restore and
//! must go red.

use std::cell::RefCell;
use std::rc::Rc;

use v2a03_micro::rung::Rung;
use v2a03_micro::state::RungState;
use v6502_micro::machine::MicroBus;
use v6502_pins::{Load, PinEngine};

fn w(reg: u8, v: u8) -> [u8; 5] {
    [0xa9, v, 0x8d, reg, 0x40]
}

fn busy_program() -> Vec<Load> {
    let mut prog = Vec::new();
    for (r, v) in [
        (0x17u8, 0x00u8), // the four-step sequence, its IRQ allowed
        (0x00, 0xbf), (0x01, 0x99), (0x02, 0x40), (0x03, 0x08),
        (0x04, 0x7f), (0x05, 0x8a), (0x06, 0x80), (0x07, 0x09),
        (0x08, 0x81), (0x0a, 0x30), (0x0b, 0x08),
        (0x0c, 0x3a), (0x0e, 0x84), (0x0f, 0x08),
        (0x10, 0x8f), (0x11, 0x20), (0x12, 0x00), (0x13, 0x02), // DMC: fastest, IRQ on, sample at $C000
        (0x15, 0x1f),
    ] {
        prog.extend(w(r, v));
    }
    prog.extend(w(0x14, 0x02));
    // The loop: read the status (the frame IRQ flag clears on a read),
    // store it, write the noise period again, and go round.
    let top = 0x8000 + prog.len() as u16;
    prog.extend([0xad, 0x15, 0x40, 0x85, 0x10, 0xa5, 0x10, 0x8d, 0x0e, 0x40]);
    prog.extend([0x4c, top as u8, (top >> 8) as u8]);
    let sample: Vec<u8> = (0..33u8).map(|i| i.wrapping_mul(0x5b) ^ 0xa5).collect();
    let page: Vec<u8> = (0..=255u8).map(|i| i.wrapping_mul(3)).collect();
    vec![Load { org: 0x8000, bytes: prog }, Load { org: 0xc000, bytes: sample }, Load { org: 0x0200, bytes: page }]
}

/// Flat memory the test can copy: the world's half of a console's state.
#[derive(Clone)]
struct Flat(Rc<RefCell<Vec<u8>>>);

impl MicroBus for Flat {
    fn read(&mut self, a: u16) -> u8 {
        self.0.borrow()[a as usize]
    }
    fn write(&mut self, a: u16, v: u8) {
        self.0.borrow_mut()[a as usize] = v;
    }
}

fn world(loads: &[Load]) -> Flat {
    let mut m = vec![0u8; 0x10000];
    for l in loads {
        m[l.org as usize..l.org as usize + l.bytes.len()].copy_from_slice(&l.bytes);
    }
    m[0xfffc] = 0x00;
    m[0xfffd] = 0x80;
    Flat(Rc::new(RefCell::new(m)))
}

fn bytes(r: &Rung) -> Vec<u8> {
    postcard::to_allocvec(&r.save_state().expect("at a cycle's end")).expect("a state encodes")
}

/// A fresh chip on a copy of `world`, restored from `saved`.
fn restored(world: &Flat, saved: &[u8]) -> (Rung, Flat) {
    let copy = Flat(Rc::new(RefCell::new(world.0.borrow().clone())));
    let mut r = Rung::with_bus(Box::new(copy.clone()), v2a03_micro::STACK_AT_H0_MEASURED);
    let st: RungState = postcard::from_bytes(saved).expect("a state decodes");
    r.load_state(&st).expect("a state restores");
    (r, copy)
}

#[test]
fn a_chip_restored_anywhere_runs_on_as_if_it_had_never_stopped() {
    let loads = busy_program();
    let a_world = world(&loads);
    let mut a = Rung::with_bus(Box::new(a_world.clone()), v2a03_micro::STACK_AT_H0_MEASURED);
    a.set_inputs(true, true, true, true, true);
    const END: u64 = 12_000;
    const RUN: u64 = 2_400;
    // Each restored chip runs beside the one that never stopped, for RUN
    // half-steps, and must show the same pins at every one of them.
    let mut live: Vec<(u64, Rung, Flat)> = Vec::new();
    let (mut splits, mut held) = (0, 0);
    for h in 0..END + RUN {
        // Every 17 half-steps that end a CPU cycle (see `save_state`):
        // odd, so the splits walk across every DMA and APU grain.
        if h < END && h % 17 == 0 && a.at_cycle_end() {
            let saved = bytes(&a);
            let (b, b_world) = restored(&a_world, &saved);
            assert_eq!(b.pins(), a.pins(), "at {h}: the pins as restored");
            assert_eq!(bytes(&b), saved, "at {h}: a restored chip saves the same state");
            if !a.pins().rdy {
                held += 1;
            }
            live.push((h, b, b_world));
            splits += 1;
        }
        a.half_step();
        for (at, b, _) in live.iter_mut() {
            b.half_step();
            assert_eq!(b.pins(), a.pins(), "split at {at}, {} half-steps on", h + 1 - *at);
        }
        // Compared whole at the first cycle's end RUN half-steps on.
        if !a.at_cycle_end() {
            continue;
        }
        let now = bytes(&a);
        live.retain(|(at, b, b_world)| {
            if h + 1 - at < RUN {
                return true;
            }
            assert!(bytes(b) == now, "split at {at}: the whole state after {RUN} half-steps");
            assert!(*b_world.0.borrow() == *a_world.0.borrow(), "split at {at}: the world as written");
            false
        });
    }
    assert!(live.is_empty());
    assert!(splits > 300, "{splits} splits");
    assert!(held > 20, "only {held} splits landed inside a stall");
}
