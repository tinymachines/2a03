//! A0's gates: the counts held to the reference simulator's own loading
//! of the same files, power-on convergence, and the reference's trace
//! replayed node for node, master half-step for master half-step,
//! bit-exact on ALL nodes with no exemption list at all: the first chip
//! in the family to replay with none. What bought that is halfphi
//! 0.1.3's drive order (PullDown outranks PullUp), decided FROM this
//! chip's measurement: the SO input chain forms three contested groups
//! at power-on (a layout pullup in the same group as the init-driven-low
//! pin), the reference resolves them low, and the old order resolved
//! them high, eight nodes wrong for the whole trace
//! (examples/a0-diverge-probe.rs, 2026-09-02).
//!
//! SKIPS by name without the fetched die data (tools/fetch-netlist.sh)
//! or without the golden file (tools/golden-trace/gen.js);
//! REQUIRE_NETLIST=1 / REQUIRE_GOLDEN=1 make absence a failure. MUTATE=1
//! switches off the supply-gated transistor fix-up in the subject, the
//! one piece of chip knowledge power_on adds; the golden replay must
//! diverge, which is the proof those 100 conducting transistors are
//! load-bearing.

use v2a03_sim::Cpu;

fn skip(reason: &str, require_var: &str) -> bool {
    if std::env::var(require_var).map(|v| v == "1").unwrap_or(false) {
        panic!("{require_var}=1 but {reason}");
    }
    eprintln!("SKIP: {reason}");
    true
}

fn mutate() -> bool {
    std::env::var("MUTATE").map(|v| v == "1").unwrap_or(false)
}

/// The subject: the honest power-on, or under MUTATE=1 one whose 100
/// supply-gated transistors are switched back off, halfphi's default
/// and the wrong physics.
fn subject() -> Cpu {
    let mut cpu = Cpu::power_on();
    if mutate() {
        let vcc = cpu.engine.netlist().vcc();
        let gated: Vec<_> = cpu.engine.netlist().gates_of(vcc).to_vec();
        for t in gated {
            cpu.engine.state_mut().trans_on.clear(t as usize);
        }
        cpu.engine.settle_all();
    }
    cpu
}

#[test]
fn the_counts_match_the_references_own_loading() {
    if !v2a03_netlist::available() && skip("extern/visual2a03 not fetched", "REQUIRE_NETLIST") {
        return;
    }
    // Both real parsers, halfphi and the reference JS engine itself
    // (gen.js prints its own counts to stderr), agree: 10,946
    // conducting transistors over 5,577 defined nodes in a 20,001-id
    // space, 2,021 names.
    assert_eq!(v2a03_netlist::TRANSISTOR_COUNT, 10_946);
    assert_eq!(v2a03_netlist::NODE_COUNT, 20_001);
    assert_eq!(v2a03_netlist::NAME_COUNT, 2_021);
    let nl = v2a03_netlist::netlist();
    assert_eq!(nl.transistor_count(), v2a03_netlist::TRANSISTOR_COUNT);
    let defined = (0..nl.node_count() as halfphi::NodeId)
        .filter(|&n| nl.exists(n))
        .count();
    assert_eq!(defined, 5_577);
    // The two permanently-decided transistor families: 100 supply-gated
    // (conducting in silicon; power_on sets them so) and 67
    // ground-gated (off in silicon and in the model). The 2C02 had 38
    // and 46; the recurrence is what promoted the question to a family
    // pattern rather than a 2C02 quirk.
    assert_eq!(nl.gates_of(nl.vcc()).len(), 100);
    assert_eq!(nl.gates_of(nl.vss()).len(), 67);
    for name in ["clk_in", "clk0", "res", "so", "irq", "nmi", "rw", "out0", "dbe", "rdy"] {
        assert!(nl.node(name).is_some(), "node {name} missing");
    }
    eprintln!(
        "2a03: {} node ids, {defined} defined, {} transistors, {} names",
        v2a03_netlist::NODE_COUNT,
        v2a03_netlist::TRANSISTOR_COUNT,
        v2a03_netlist::NAME_COUNT
    );
}

#[test]
fn power_on_converges_with_no_chip_specific_help() {
    if !v2a03_netlist::available() && skip("extern/visual2a03 not fetched", "REQUIRE_NETLIST") {
        return;
    }
    let cpu = Cpu::power_on();
    let cold = cpu.engine.stats().nonconvergent_settles;
    assert_eq!(cold, 0, "nonconvergent settles during power-on: {cold}");
    // The three contested groups the SO chain forms are expected and
    // stay three; a fourth means a new group is exercising the drive
    // order somewhere unmeasured, and the diverge probe is the next
    // thing to run.
    assert_eq!(cpu.engine.stats().contested_groups, 3);
}

#[test]
fn the_reference_trace_replays_node_for_node_with_no_exemptions() {
    if !v2a03_netlist::available() && skip("extern/visual2a03 not fetched", "REQUIRE_NETLIST") {
        return;
    }
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tools/golden-trace/golden-2a03.txt"
    );
    let Ok(golden) = std::fs::read_to_string(path) else {
        if skip("no golden trace (node tools/golden-trace/gen.js)", "REQUIRE_GOLDEN") {
            return;
        }
        unreachable!()
    };
    let mut lines = golden.lines();
    let header = lines.next().expect("golden header");
    assert!(header.starts_with("2a03 golden:"), "not a 2a03 golden: {header}");

    let mut cpu = subject();
    let nl = cpu.engine.netlist().clone();
    let mut compared = 0usize;
    for (step, want) in lines.enumerate() {
        if step > 0 {
            cpu.half_step();
        }
        let got = cpu.state_line();
        assert_eq!(got.len(), want.len(), "node count differs at step {step}");
        for (i, (a, b)) in got.bytes().zip(want.bytes()).enumerate() {
            if a != b {
                let name = nl.name_of(i as halfphi::NodeId).unwrap_or("(unnamed)");
                panic!("step {step}: divergence at node {i} ({name})");
            }
        }
        compared += 1;
    }
    assert!(compared > 100, "golden too short to mean anything: {compared}");
    eprintln!("replayed {compared} states bit-exact on all nodes, no exemptions");
}
