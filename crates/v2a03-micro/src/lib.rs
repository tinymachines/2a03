//! The 2A03's ladder rung (the console sketch's N3): no nodes. The core
//! is the 6502's rung 3 (`v6502-micro`, a git dependency pinned by
//! revision) configured as this die's core by the two knobs N3 step 1's
//! divergence list justified (`docs/n3-report.md`): the decimal adjust
//! disconnected, and the stack pointer at h=0 seeded with the value this
//! chip's rung 0 measures. Nothing about the core is authored here.
//!
//! The APU follows as tables measured out of rung 0 (`docs/n3-plan.md`,
//! steps 3 to 5).

#![forbid(unsafe_code)]

pub mod apu;
pub mod rung;

/// The tables measured out of rung 0 at build time (`build.rs`).
pub mod tables {
    /// An LFSR-shaped timer as the die builds two of them: it free-runs
    /// from `at_h0`, each tick shifting left with the XOR of the two
    /// `taps` fed in, and when it stands at `terminal` the next tick
    /// reloads it with `reload[rate]` and clocks its unit.
    #[derive(Debug)]
    pub struct LfsrTimer {
        pub at_h0: u32,
        pub taps: (u8, u8),
        pub terminal: u32,
        pub reload: [u16; 16],
    }
    include!(concat!(env!("OUT_DIR"), "/tables.rs"));
}

use v6502_micro::machine::{MicroBus, MicroCpu};
use v6502_pins::Load;

/// The core as this die presents it: rung 3 with the adjust disconnected
/// and S seeded. `stack_at_h0` is measured off rung 0's register nodes
/// (`v2a03_sim::pins::CorePins::stack_pointer` after `power_cycle`), never
/// typed; passing a typed number is a bug by name.
/// The stack pointer at the pin contract's h=0, read off rung 0's s0..s7
/// register nodes (`tests/core.rs` reads it every run and holds this
/// constant to it, so a console that has no rung 0 of its own can seed
/// the core with a measurement rather than a number).
pub const STACK_AT_H0_MEASURED: u8 = 0xbd;

/// The same core on a bus a console provides (`MicroBus`): no flat
/// image, the reset vector read through the bus like everything else.
pub fn core_on_bus(bus: Box<dyn MicroBus>, stack_at_h0: u8) -> MicroCpu {
    let mut m = MicroCpu::new();
    m.set_decimal_adjust(false);
    m.set_stack_at_h0(Some(stack_at_h0));
    m.bus = Some(bus);
    v6502_pins::PinEngine::power_cycle(&mut m);
    m
}

pub fn core(loads: &[Load], reset_vector: u16, stack_at_h0: u8) -> MicroCpu {
    let mut m = MicroCpu::new();
    m.set_decimal_adjust(false);
    m.set_stack_at_h0(Some(stack_at_h0));
    for l in loads {
        let o = l.org as usize;
        m.mem[o..o + l.bytes.len()].copy_from_slice(&l.bytes);
    }
    m.mem[0xfffc] = reset_vector as u8;
    m.mem[0xfffd] = (reset_vector >> 8) as u8;
    v6502_pins::PinEngine::power_cycle(&mut m);
    m
}
