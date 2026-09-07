//! The joypad port under a DMC fetch: how many times the world is asked
//! for $4016 per `LDA $4016`, on the rung against the die. On the die
//! (`v2a03-sim`'s `joy-clock-probe`, 2026-09-06) a fetch that lands on
//! the read holds /OE1 low through the halt cycles, one continuous
//! pulse, lets it rise for the fetch's own read, and pulses it again
//! when the held read re-runs with RDY high: two pulses for that
//! instruction, one for every other. A 4021 on the port shifts on each
//! rise, so the pad is clocked twice and the core takes the bit after
//! the one it asked for; a console's board has to be asked exactly that
//! often, no more (the halt cycles are silent) and no less (the re-run
//! is real). This gate counts /r4016's falls per instruction on rung 0
//! and the bus's asks per instruction on the rung, over the same
//! program, and holds the two sequences equal.
//!
//! SKIPS by name without the die data; REQUIRE_NETLIST=1 insists.
//! MUTATE_QUIET=1 lets every held re-ask through (four per collision)
//! and MUTATE_HELD=1 (rung 3's own) keeps the first byte and asks once;
//! both must go red.

use std::cell::Cell;
use std::rc::Rc;

use v2a03_micro::rung::Rung;
use v2a03_sim::pins::CorePins;
use v6502_micro::machine::MicroBus;
use v6502_pins::{Load, PinEngine};

/// LDA #$4F; STA $4010 (loop, rate 15); $4012 = $00; $4013 = $FF;
/// $4015 = $10; strobe the pad; then LDA $4016 / `nops` NOPs / JMP
/// forever. The sample sits at $C000..$DFFF, clear of the vectors. With
/// no NOPs the loop is 7 cycles and every seventh fetch (432 cycles
/// apart) lands on the read, the 22nd from $C016; with two it is 11 and
/// the 54th, from $C036, lands there: the two addresses whose low five
/// bits are the port's, where the die keeps the strobe low.
fn program(nops: usize) -> (Vec<Load>, u16) {
    let mut prog = Vec::new();
    for (r, v) in [(0x10u8, 0x4fu8), (0x12, 0x00), (0x13, 0xff), (0x15, 0x10), (0x16, 0x01), (0x16, 0x00)] {
        prog.extend([0xa9, v, 0x8d, r, 0x40]);
    }
    let loop_at = 0x8000 + prog.len() as u16;
    prog.extend([0xad, 0x16, 0x40]);
    prog.extend(std::iter::repeat_n(0xea, nops));
    prog.extend([0x4c, loop_at as u8, (loop_at >> 8) as u8]);
    let sample: Vec<u8> = (0..0x2000usize).map(|i| (i as u8).wrapping_mul(7)).collect();
    (vec![Load { org: 0x8000, bytes: prog }, Load { org: 0xc000, bytes: sample }], loop_at)
}

struct Counting {
    mem: Vec<u8>,
    asks: Rc<Cell<u32>>,
}

impl MicroBus for Counting {
    fn read(&mut self, a: u16) -> u8 {
        if a == 0x4016 {
            self.asks.set(self.asks.get() + 1);
        }
        self.mem[a as usize]
    }
    fn write(&mut self, a: u16, v: u8) {
        self.mem[a as usize] = v;
    }
}

/// Per `LDA $4016` fetched (sync at its address), the count the closure
/// accumulates between one fetch and the next.
fn per_instruction<E: PinEngine>(e: &mut E, loop_at: u16, frames: u64, mut count: impl FnMut(&E) -> u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut prev_sync = false;
    let mut acc = 0u32;
    let mut open = false;
    for _ in 0..frames {
        PinEngine::half_step(e);
        let f = PinEngine::pins(e);
        if f.sync && !prev_sync && f.ab == loop_at {
            if open {
                out.push(acc);
            }
            open = true;
            acc = 0;
        }
        acc += count(e);
        prev_sync = f.sync;
    }
    out
}

#[test]
fn the_world_is_asked_for_4016_as_often_as_the_die_pulses_oe1() {
    if !v2a03_netlist::available() || !v2a03_micro::tables::AVAILABLE {
        if std::env::var_os("REQUIRE_NETLIST").is_some() {
            panic!("REQUIRE_NETLIST=1 but no die data");
        }
        eprintln!("SKIP: no die data");
        return;
    }
    // Both cadences: the aliased sample address ($C016, then $C036) is
    // what the rung's strobe rule is for, and each run must also collide
    // plainly a few times or the gate holds nothing.
    compare(0, 40_000, 5);
    compare(2, 50_000, 4);
}

fn compare(nops: usize, frames: u64, min_doubles: usize) {
    let (loads, loop_at) = program(nops);

    // Rung 0: /r4016's falls, per instruction.
    let mut r0 = CorePins::new(&loads, 0x8000);
    r0.power_cycle();
    let s = r0.stack_pointer();
    let r4016 = r0.har.cpu.engine.netlist().node("/r4016").unwrap();
    let mut prev = true;
    let die = per_instruction(&mut r0, loop_at, frames, |e| {
        let now = e.har.cpu.engine.is_high(r4016);
        let fell = prev && !now;
        prev = now;
        fell as u32
    });

    // The rung: the bus's asks, per instruction.
    let mut mem = vec![0u8; 0x10000];
    for l in &loads {
        mem[l.org as usize..l.org as usize + l.bytes.len()].copy_from_slice(&l.bytes);
    }
    mem[0xfffc] = 0x00;
    mem[0xfffd] = 0x80;
    let asks = Rc::new(Cell::new(0u32));
    let mut rung = Rung::with_bus(Box::new(Counting { mem, asks: asks.clone() }), s);
    let mut last = 0u32;
    let model = per_instruction(&mut rung, loop_at, frames, |_| {
        let n = asks.get();
        let d = n - last;
        last = n;
        d
    });

    let hist = |v: &[u32]| {
        let mut h = std::collections::BTreeMap::new();
        for &k in v {
            *h.entry(k).or_insert(0u32) += 1;
        }
        h
    };
    let doubles = die.iter().filter(|&&k| k == 2).count();
    eprintln!("{nops} NOPs, {frames} frames: die {} instructions, pulses per instruction {:?}; rung {} instructions, asks per instruction {:?}", die.len(), hist(&die), model.len(), hist(&model));
    assert!(doubles >= min_doubles, "the program must collide a fetch with the read at least {min_doubles} times over {frames} frames, or the gate holds nothing: {doubles}");
    assert_eq!(die.len(), model.len(), "the two ran the same number of instructions");
    if let Some(i) = (0..die.len()).find(|&i| die[i] != model[i]) {
        panic!("instruction {i}: the die pulsed /OE1 {} time(s), the rung asked the world {} time(s) for $4016", die[i], model[i]);
    }
}
