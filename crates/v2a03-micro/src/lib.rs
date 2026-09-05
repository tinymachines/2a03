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

use v6502_micro::machine::MicroCpu;
use v6502_pins::Load;

/// The core as this die presents it: rung 3 with the adjust disconnected
/// and S seeded. `stack_at_h0` is measured off rung 0's register nodes
/// (`v2a03_sim::pins::CorePins::stack_pointer` after `power_cycle`), never
/// typed; passing a typed number is a bug by name.
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
