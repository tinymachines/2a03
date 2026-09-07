//! The M2 pin against the core's clock, in master half-steps: what the
//! bench's alignment classifier (nes-bench/tools/b2-align.py) reads off
//! the scope is M2, and the console's `Alignment` is defined on the
//! core's clk0 (the console's cpu_phase is the half-step of a phi1's
//! start, where clk0 falls). This prints, over the
//! first few hundred half-steps after power-on, every edge of clk0 and
//! of the M2 output node (`phi2`), so the offset between them is a
//! measured number and not an assumption.
//!
//!   cargo run --release -p v2a03-sim --example m2-phase
//!
//! Measured 2026-09-07: M2 falls on the half-step clk0 falls (offset 0)
//! and rises three half-steps before clk0 rises, so it is high for 15
//! of 24: the 62.5 percent duty the part is documented with.
use v2a03_sim::Cpu;

fn main() {
    let mut cpu = Cpu::power_on();
    let nl = cpu.engine.netlist().clone();
    let m2 = nl.node("phi2").expect("phi2 (the M2 output)");
    let (mut p0, mut pm) = (cpu.engine.is_high(cpu.sig.clk0), cpu.engine.is_high(m2));
    let mut clk_edges = Vec::new();
    let mut m2_edges = Vec::new();
    for m in 1..=240u32 {
        cpu.half_step();
        let (c, x) = (cpu.engine.is_high(cpu.sig.clk0), cpu.engine.is_high(m2));
        if c != p0 {
            clk_edges.push((m, c as u8));
            p0 = c;
        }
        if x != pm {
            m2_edges.push((m, x as u8));
            pm = x;
        }
    }
    println!("clk0 edges (half-step, level): {clk_edges:?}");
    println!("M2 edges   (half-step, level): {m2_edges:?}");
    // The offset of each M2 edge from the nearest clk0 edge of the
    // same direction, once the divider has settled.
    let mut offsets = std::collections::BTreeMap::new();
    for &(m, lv) in m2_edges.iter().skip(4) {
        if let Some(&(c, _)) = clk_edges.iter().filter(|&&(c, l)| l == lv && c <= m).last() {
            *offsets.entry((lv, m as i64 - c as i64)).or_insert(0u32) += 1;
        }
    }
    println!("M2 edge minus the last clk0 edge of the same level, half-steps: {offsets:?}");
}
