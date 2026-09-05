//! The 2A03's 6502 core at the pin contract: `v6502_pins::PinEngine` for
//! the memory harness, one `PinFrame` per clk0 phase (the 6502's own
//! half-cycle; the 2A03's native unit is the master half-step, twelve of
//! which make one clk0 phase). This is what the cross-chip pin-lockstep
//! gate drives: `v6502_pins::run` applies a recorded `.stim` and collects
//! frames exactly as it does for every rung of the 6502's ladder, so the
//! two chips cannot be run through a script differently.
//!
//! The contract's h=0 is "the frame the reset sequence leaves behind",
//! which on the 6502's rung 0 is the first opcode fetch: its power_cycle
//! runs the vector reads before anyone looks. The 2A03's power_on
//! (the reference's initChip recipe) leaves the core earlier in its reset
//! sequence, and `ALIGN_PHASES` is the MEASURED distance from that frame
//! to the first opcode fetch (`examples/lockstep-probe.rs`, 2026-09-05:
//! 17 phases on every trace tried). `power_cycle` walks there and asserts
//! the frame it lands on is a sync-high fetch of the reset vector, so a
//! changed reset recipe fails by name instead of shifting every
//! comparison by a phase.
//!
//! Two of the contract's five inputs are not 2A03 pins: RDY is an
//! internal node the DMA units drive (`spr_dma_/rdy`, `pcm_dma_/rdy`) and
//! SO is an unbonded pad the reference holds low. A script that asks for
//! either non-idle sets `absent_pin_driven`, and a caller comparing
//! against such a trace must refuse by name rather than read a
//! coincidence. The frame reports both from their internal nodes.

use halfphi::NodeId;
use v6502_pins::{Load, PinEngine, PinFrame};

use crate::harness::Harness;
use crate::Cpu;

/// clk0 phases from the frame `Cpu::power_on` leaves to the first opcode
/// fetch after the reset vector reads. Measured, then asserted.
pub const ALIGN_PHASES: u64 = 17;

pub struct CorePins {
    pub har: Harness,
    loads: Vec<Load>,
    reset_vector: u16,
    h: u64,
    clk0: NodeId,
    sync: NodeId,
    rw: NodeId,
    res: NodeId,
    irq: NodeId,
    nmi: NodeId,
    rdy: NodeId,
    so: NodeId,
    ab: [NodeId; 16],
    db: [NodeId; 8],
    /// A script asked for RDY low or SO high, pins this chip does not have.
    pub absent_pin_driven: bool,
    /// clk0 stopped under a driven input (the divider held), reported
    /// rather than spun on; the frames after it repeat the last state.
    pub clock_stopped: bool,
}

impl CorePins {
    /// The 2A03 core built from what a `.pins` header says, the mirror of
    /// `v6502_sim::pins::rung0`: the loads placed, the reset vector set,
    /// nothing run yet (`run` calls `power_cycle`).
    pub fn new(loads: &[Load], reset_vector: u16) -> CorePins {
        let har = Harness::new(Cpu::power_on());
        let nl = har.cpu.engine.netlist().clone();
        let n = |name: &str| nl.node(name).unwrap_or_else(|| panic!("node {name}"));
        let mut cp = CorePins {
            har,
            loads: loads.to_vec(),
            reset_vector,
            h: 0,
            clk0: n("clk0"),
            sync: n("sync"),
            rw: n("rw"),
            res: n("res"),
            irq: n("irq"),
            nmi: n("nmi"),
            rdy: n("rdy"),
            so: n("so"),
            ab: std::array::from_fn(|i| n(&format!("ab{i}"))),
            db: std::array::from_fn(|i| n(&format!("db{i}"))),
            absent_pin_driven: false,
            clock_stopped: false,
        };
        cp.place();
        cp
    }

    fn place(&mut self) {
        for l in &self.loads {
            let o = l.org as usize;
            self.har.memory[o..o + l.bytes.len()].copy_from_slice(&l.bytes);
        }
        self.har.memory[0xfffc] = self.reset_vector as u8;
        self.har.memory[0xfffd] = (self.reset_vector >> 8) as u8;
    }

    fn bits(&self, ns: &[NodeId]) -> u32 {
        ns.iter().enumerate().map(|(i, &nd)| (self.har.cpu.engine.is_high(nd) as u32) << i).sum()
    }

    /// The core's stack pointer as the register nodes `s0..s7` hold it:
    /// the measurement a core rung seeds its own from.
    pub fn stack_pointer(&self) -> u8 {
        let nl = self.har.cpu.engine.netlist().clone();
        (0..8)
            .map(|i| {
                let n = nl.node(&format!("s{i}")).unwrap_or_else(|| panic!("node s{i}"));
                (self.har.cpu.engine.is_high(n) as u8) << i
            })
            .sum()
    }
}

impl PinEngine for CorePins {
    fn power_cycle(&mut self) {
        self.har = Harness::new(Cpu::power_on());
        self.place();
        self.absent_pin_driven = false;
        self.clock_stopped = false;
        for _ in 0..ALIGN_PHASES {
            self.har.half_step();
        }
        let f = self.pins();
        assert!(
            f.sync && f.rw && f.ab == self.reset_vector,
            "after {ALIGN_PHASES} phases the core is not fetching from the reset vector: {} (the reset recipe changed; re-measure with lockstep-probe)",
            v6502_pins::line(&f)
        );
        self.h = 0;
    }

    fn set_inputs(&mut self, res: bool, irq: bool, nmi: bool, rdy: bool, so: bool) {
        let e = &mut self.har.cpu.engine;
        for (node, level) in [(self.res, res), (self.irq, irq), (self.nmi, nmi)] {
            if level {
                e.drive_high(node);
            } else {
                e.drive_low(node);
            }
        }
        if !rdy || so {
            self.absent_pin_driven = true;
        }
    }

    fn half_step(&mut self) {
        if !self.har.half_step_bounded(1 << 12) {
            self.clock_stopped = true;
        }
        self.h += 1;
    }

    fn pins(&self) -> PinFrame {
        let e = &self.har.cpu.engine;
        PinFrame {
            h: self.h,
            clk0: e.is_high(self.clk0),
            ab: self.bits(&self.ab) as u16,
            db: self.bits(&self.db) as u8,
            rw: e.is_high(self.rw),
            sync: e.is_high(self.sync),
            res: e.is_high(self.res),
            irq: e.is_high(self.irq),
            nmi: e.is_high(self.nmi),
            rdy: e.is_high(self.rdy),
            so: e.is_high(self.so),
        }
    }

    fn h(&self) -> u64 {
        self.h
    }
}
