//! N3 step 2's gate: the core rung against this chip's rung 0, program
//! for program, over every recorded 6502 pin trace's program and script.
//! Rung 0 is run through `CorePins` (the same adapter the cross-chip
//! gate proved), the core rung through `v2a03_micro::core` with S read
//! off rung 0's register nodes, and the two must agree in every field at
//! every half-cycle except the write-phi1 byte, the one class step 1's
//! list left to this die's pads (`v2a03_sim::lockstep`): the core rung
//! shows the 6502's last-read byte there, rung 0 shows the 2A03's, and
//! nothing is serviced in that half. The decimal chains must agree
//! outright, which is the divergence list's whole point: the binary
//! bytes are this chip's.
//!
//! One program is listed as diverging, by name and bounded
//! (`EXPECTED_DIVERGENCE`): the reset-mid-run script. Rung 3 plays the
//! 6502's own response to RES (the in-flight BRK run on, then a
//! freewheel), and this die holds its core still under RES instead
//! (`docs/n3-report.md`, step 1); until that hold is measured and
//! authored as a knob, the two must differ only inside the reset window
//! and read the vector at the same half-cycle, and a clean replay there
//! fails too, so the list cannot rot.
//!
//! The scripts that drive RDY or SO are refused as in step 1. SKIPS by
//! name without the die data or the recordings; REQUIRE_NETLIST=1 and
//! REQUIRE_PINS=1 insist. `MUTATE=1` leaves the decimal adjust connected
//! on the core rung: the three decimal traces must go red at their
//! stores, by name.

use std::path::PathBuf;

use v2a03_sim::lockstep::{classify, WRITE_PHI1};
use v2a03_sim::pins::CorePins;
use v6502_pins::{line, parse_stim, parse_trace, run, PinEngine};

const ABSENT_PIN_TRACES: &[&str] = &["fixture-rdy-stall", "fixture-rdy-in-write", "fixture-rdy-release-phi1", "fixture-so-pulse"];
const DECIMAL_TRACES: &[&str] = &["decimal-adc", "decimal-sbc", "decimal-mixed"];

/// Programs the core rung is known not to reproduce on this die, each
/// with the measured reason. A trace here must still diverge, inside the
/// stated bounds.
const EXPECTED_DIVERGENCE: &[(&str, &str)] = &[(
    "fixture-reset-mid-run",
    "the 2A03 holds its core still while RES is low where the 6502 runs its hijacked BRK on and freewheels; rung 3 plays the 6502's. Both read the vector at the same half-cycle. The hold is step 2's carried item: measure it, then author it as a knob.",
)];

fn mutate() -> bool {
    std::env::var("MUTATE").is_ok_and(|v| v == "1")
}

fn pin_golden_dir() -> PathBuf {
    match std::env::var_os("PIN_GOLDEN") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../6502/tools/pin-golden"),
    }
}

#[test]
fn the_core_rung_matches_rung_0_on_every_program_but_the_write_phi1_byte() {
    if !v2a03_netlist::available() {
        if std::env::var_os("REQUIRE_NETLIST").is_some() {
            panic!("REQUIRE_NETLIST=1 but extern/visual2a03 is not fetched");
        }
        eprintln!("SKIP: extern/visual2a03 not fetched");
        return;
    }
    let dir = pin_golden_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        if std::env::var_os("REQUIRE_PINS").is_some() {
            panic!("REQUIRE_PINS=1 but no pin golden at {}", dir.display());
        }
        eprintln!("SKIP: no 6502 pin golden at {} (the programs and scripts come from there)", dir.display());
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "pins"))
        .collect();
    paths.sort();
    assert!(paths.len() >= 270, "{} traces; the 6502 records 274", paths.len());

    let (mut compared, mut exact, mut refused, mut frames) = (0usize, 0usize, 0usize, 0usize);
    let mut wphi1 = 0usize;
    let mut decimal_red: Vec<String> = Vec::new();
    let mut s_seen: Option<u8> = None;
    for path in &paths {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let trace = parse_trace(&std::fs::read_to_string(path).unwrap()).unwrap();
        let stim = if trace.header.stim.is_empty() {
            Vec::new()
        } else {
            parse_stim(&std::fs::read_to_string(path.with_extension("stim")).unwrap()).unwrap()
        };
        if ABSENT_PIN_TRACES.contains(&name.as_str()) {
            refused += 1;
            continue;
        }
        let steps = trace.frames.len() as u64 - 1;
        // Rung 0, and S off its register nodes at h=0.
        let mut r0 = CorePins::new(&trace.header.loads, trace.header.reset_vector);
        r0.power_cycle();
        let s = r0.stack_pointer();
        match s_seen {
            None => s_seen = Some(s),
            Some(x) => assert_eq!(x, s, "{name}: rung 0's S at h=0 changed between programs"),
        }
        let want = run(&mut r0, steps, &stim);
        assert!(!r0.absent_pin_driven && !r0.clock_stopped, "{name}: rung 0 refused or stopped");
        // The core rung, seeded from that measurement.
        let mut rung = v2a03_micro::core(&trace.header.loads, trace.header.reset_vector, s);
        if mutate() {
            rung.set_decimal_adjust(true);
        }
        let got = run(&mut rung, steps, &stim);
        // Offset 0: the seed makes the stack pages agree, so any page-1
        // difference is loud. No S leak is allowed either.
        let rep = classify(&want, &got, 0, false);
        compared += 1;
        frames += want.len();
        wphi1 += rep.count(WRITE_PHI1);
        if rep.counts.is_empty() {
            exact += 1;
        }
        if let Some((_, why)) = EXPECTED_DIVERGENCE.iter().find(|(n, _)| *n == name) {
            assert!(!rep.loud.is_empty(), "{name} is listed as diverging but replayed clean; take it off the list");
            let res_low = want.iter().position(|f| !f.res).expect("RES asserted in the script");
            let vec_want = want.iter().skip(res_low).position(|f| f.ab == 0xfffc && f.rw).map(|i| i + res_low).expect("rung 0 vector read");
            let vec_got = got.iter().skip(res_low).position(|f| f.ab == 0xfffc && f.rw).map(|i| i + res_low).expect("core rung vector read");
            assert_eq!(vec_want, vec_got, "{name}: the vector is read at different half-cycles");
            for l in &rep.loud {
                assert!(l.h > res_low && l.h < vec_want - 6, "{name} h={}: a difference outside the reset window", l.h);
            }
            eprintln!("  {name}: listed divergence, {} frames inside the reset window h={}..{}, vector at h={vec_want} on both: {why}", rep.loud.len(), res_low + 1, vec_want - 7);
            continue;
        }
        if !rep.loud.is_empty() {
            let first = &rep.loud[0];
            let msg = format!(
                "{name}: {} unnamed difference(s); first h={} {:?}\n    rung 0    {}\n    core rung {}",
                rep.loud.len(),
                first.h,
                first.classes,
                line(&first.expected),
                line(&first.got)
            );
            if mutate() && DECIMAL_TRACES.contains(&name.as_str()) {
                decimal_red.push(msg);
                continue;
            }
            panic!("{msg}");
        }
    }
    assert_eq!(refused, ABSENT_PIN_TRACES.len());
    if mutate() {
        assert_eq!(decimal_red.len(), DECIMAL_TRACES.len(), "MUTATE=1: every decimal chain must go red with the adjust connected");
        eprintln!("MUTATE=1: the three decimal chains diverged with the adjust connected:\n{}", decimal_red.join("\n"));
        panic!("MUTATE=1: the core rung with the 6502's decimal adjust is not this chip's core (as it must not be)");
    }
    assert!(wphi1 > 0, "no write-phi1 frame differed; the class is gone, re-measure");
    eprintln!(
        "core rung vs rung 0: {compared} programs, {frames} half-cycles, {exact} exact in every field, S seeded ${:02x} from rung 0's nodes; {wphi1} write-phi1 bytes differ and nothing else does",
        s_seen.unwrap()
    );
}
