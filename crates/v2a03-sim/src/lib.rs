//! The 2A03 as a running machine: halfphi's engine plus the chip's own
//! clock, reset and input pins, initialized exactly the way the
//! reference simulator's initChip does it, so the golden comparison is
//! a comparison and not a coincidence.
//!
//! The unit here is the MASTER half-step: one toggle of `clk_in`, the
//! 21.477272 MHz pin. The divided CPU clock (`clk0`, the ÷12) is an
//! output the divider produces, never something this layer drives; the
//! reference's own halfStep spins `clk_in` until `clk0` moves, and that
//! loop belongs to a later milestone's harness, not to the chip.
//!
//! The same deliberate state fix-up as the 2C02, measured before this
//! file was written: the 2A03 has transistors gated by the supply rail
//! (`build.rs` counts are in docs/a0-report.md). In silicon they conduct
//! permanently; the reference's init turns them on and nothing ever
//! turns them off; halfphi's power-on state starts every transistor off
//! and would leave them off forever. `power_on` sets exactly those
//! conducting once. Ground-gated transistors are permanently off in
//! both models and need nothing.

pub mod harness;
pub mod lockstep;
pub mod mixer;
pub mod pins;

use std::sync::Arc;

use halfphi::{Engine, Netlist, NodeId};

pub struct Sig {
    /// The master clock pin, this layer's half-step unit.
    pub clk_in: NodeId,
    /// The divided CPU clock, an output of the on-die ÷12.
    pub clk0: NodeId,
    pub res: NodeId,
    pub so: NodeId,
    pub irq: NodeId,
    pub nmi: NodeId,
}

pub struct Cpu {
    pub engine: Engine,
    pub sig: Sig,
}

impl Cpu {
    /// Power on and run the reference's reset recipe (macros.js
    /// initChip, statement for statement): everything to the power-on
    /// state, supply-gated transistors conducting, layout pulls
    /// restored, the master clock low and pulsed six times, reset and
    /// SO low, IRQ and NMI high, a full settle, ninety-six master
    /// pulses under reset, reset released.
    pub fn power_on() -> Cpu {
        Cpu::power_on_with(v2a03_netlist::netlist())
    }

    pub fn power_on_with(nl: Arc<Netlist>) -> Cpu {
        let sig = Sig {
            clk_in: nl.node("clk_in").expect("clk_in"),
            clk0: nl.node("clk0").expect("clk0"),
            res: nl.node("res").expect("res"),
            so: nl.node("so").expect("so"),
            irq: nl.node("irq").expect("irq"),
            nmi: nl.node("nmi").expect("nmi"),
        };
        let supply_gated: Vec<_> = nl.gates_of(nl.vcc()).to_vec();
        let mut engine = Engine::new(nl);
        engine.force_power_on_state();
        for t in supply_gated {
            engine.state_mut().trans_on.set(t as usize);
        }
        engine.restore_layout_pulls();
        engine.drive_low(sig.clk_in);
        for _ in 0..6 {
            engine.drive_high(sig.clk_in);
            engine.drive_low(sig.clk_in);
        }
        engine.drive_low(sig.res);
        engine.drive_low(sig.so);
        engine.drive_high(sig.irq);
        engine.drive_high(sig.nmi);
        engine.settle_all();
        for _ in 0..12 * 8 {
            engine.drive_high(sig.clk_in);
            engine.drive_low(sig.clk_in);
        }
        engine.drive_high(sig.res);
        Cpu { engine, sig }
    }

    /// One master half-step: toggle `clk_in`, settle.
    pub fn half_step(&mut self) {
        if self.engine.is_high(self.sig.clk_in) {
            self.engine.drive_low(self.sig.clk_in);
        } else {
            self.engine.drive_high(self.sig.clk_in);
        }
    }

    /// Every node's level as a '0'/'1' line over node ids 0..node_count,
    /// nonexistent ids as '0': byte for byte what the golden generator
    /// writes.
    pub fn state_line(&self) -> String {
        let nl = self.engine.netlist();
        (0..nl.node_count() as NodeId)
            .map(|n| {
                if nl.exists(n) && self.engine.is_high(n) {
                    '1'
                } else {
                    '0'
                }
            })
            .collect()
    }
}
