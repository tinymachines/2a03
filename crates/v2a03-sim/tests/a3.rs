//! A3's gates: first sound. The authored square-note program
//! (tools/golden-trace/program-a3.json, the one copy) runs on the chip
//! through the memory harness, and three things must hold:
//!
//! 1. The reference's own run of the same program replays node for
//!    node, CPU half-step for CPU half-step, with no exemptions
//!    (tools/golden-trace/gen-a3.js): the APU, the core and the bus
//!    glue under one comparison.
//! 2. The square channel's output code, read off sq0_out every half
//!    step, is a 50 percent duty square whose plateau length in half
//!    steps derives from the program's own timer byte: the sequencer
//!    steps every 2*(t+1) CPU cycles and the duty pattern is 8 steps,
//!    so each plateau is 2 * 2*(t+1) * 4 half-steps. The netlist
//!    proposes; the measurement disposes.
//! 3. Mixed through the authored nesdev table (src/mixer.rs), the run
//!    emits a two-level nes-bus AudioSamples whose levels are exactly
//!    mixer::ad1(15, 0) and 0.
//!
//! SKIPS by name without the die data or the golden; REQUIRE_NETLIST=1
//! and REQUIRE_GOLDEN=1 insist. MUTATE=1 makes the harness serve the
//! timer byte XOR 1 (the chip plays a note the program did not write):
//! the replay AND the period measurement must both go red.

use v2a03_sim::harness::Harness;
use v2a03_sim::{mixer, Cpu};

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

struct Program {
    load: u16,
    reset_vector: u16,
    bytes: Vec<u8>,
    half_steps: usize,
}

/// The authored program, read from the same file gen-a3.js reads. The
/// parse is deliberately dumb: the file is ours and small, and a parse
/// that stops matching is a loud failure.
fn program() -> Program {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tools/golden-trace/program-a3.json"
    ))
    .expect("program-a3.json");
    let field = |k: &str| -> u64 {
        let at = text.find(&format!("\"{k}\":")).unwrap_or_else(|| panic!("{k} missing"));
        text[at..]
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap()
    };
    let b0 = text.find("\"bytes\": [").expect("bytes");
    let b1 = b0 + text[b0..].find(']').expect("bytes close");
    let bytes: Vec<u8> = text[b0 + 10..b1]
        .split(',')
        .map(|s| s.trim().parse().expect("byte"))
        .collect();
    Program {
        load: field("load") as u16,
        reset_vector: field("reset_vector") as u16,
        bytes,
        half_steps: field("half_steps") as usize,
    }
}

/// The subject: the program loaded, and under MUTATE=1 the timer byte
/// served wrong.
fn subject(p: &Program) -> Harness {
    let mut h = Harness::new(Cpu::power_on());
    h.load(p.load, &p.bytes, p.reset_vector);
    if mutate() {
        // The address the program stores the timer low byte from: the
        // operand of "LDA #$08" sits in memory, and the harness serves
        // it XOR 1, so the chip assembles a different note.
        let timer_operand_addr = p.load + 11;
        assert_eq!(p.bytes[11], 8, "the program moved; re-aim the mutation");
        h.mutate_read = Some((timer_operand_addr, 1));
    }
    h
}

#[test]
fn the_reference_replays_through_the_harness() {
    if !v2a03_netlist::available() && skip("extern/visual2a03 not fetched", "REQUIRE_NETLIST") {
        return;
    }
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tools/golden-trace/golden-2a03-a3.txt"
    );
    let Ok(golden) = std::fs::read_to_string(path) else {
        if skip("no A3 golden (node tools/golden-trace/gen-a3.js)", "REQUIRE_GOLDEN") {
            return;
        }
        unreachable!()
    };
    let mut lines = golden.lines();
    let header = lines.next().expect("golden header");
    assert!(header.starts_with("2a03 a3 golden:"), "not an A3 golden: {header}");

    let p = program();
    let mut h = subject(&p);
    let nl = h.cpu.engine.netlist().clone();
    let mut compared = 0usize;
    for (step, want) in lines.enumerate() {
        if step > 0 {
            h.half_step();
        }
        let got = h.cpu.state_line();
        assert_eq!(got.len(), want.len(), "node count differs at step {step}");
        for (i, (a, b)) in got.bytes().zip(want.bytes()).enumerate() {
            if a != b {
                let name = nl.name_of(i as halfphi::NodeId).unwrap_or("(unnamed)");
                panic!("half-step {step}: divergence at node {i} ({name})");
            }
        }
        compared += 1;
    }
    assert!(compared > 1000, "golden too short to mean anything: {compared}");
    eprintln!("replayed {compared} states bit-exact through the harness, no exemptions");
}

#[test]
fn the_square_plays_the_note_the_program_wrote() {
    if !v2a03_netlist::available() && skip("extern/visual2a03 not fetched", "REQUIRE_NETLIST") {
        return;
    }
    let p = program();
    let mut h = subject(&p);
    let nl = h.cpu.engine.netlist().clone();
    let sq0: Vec<_> = (0..4)
        .map(|i| nl.node(&format!("sq0_out{i}")).expect("sq0_out"))
        .collect();

    // The expectation derives from the program's own timer byte, not
    // from this run: plateau = 2 * 2*(t+1) * 4 half-steps at 50 percent
    // duty, and the code must swing exactly 0 to volume 15.
    let t = p.bytes[11] as usize;
    let expected_plateau = 2 * 2 * (t + 1) * 4;

    let mut codes = Vec::with_capacity(p.half_steps);
    for _ in 0..p.half_steps {
        h.half_step();
        let code: u8 = sq0
            .iter()
            .enumerate()
            .map(|(i, &n)| (h.cpu.engine.is_high(n) as u8) << i)
            .sum();
        codes.push(code);
    }
    // Skip the setup: measure plateaus from the first transition after
    // a quarter of the run.
    let start = p.half_steps / 4;
    let tail = &codes[start..];
    let levels: std::collections::BTreeSet<u8> = tail.iter().copied().collect();
    assert_eq!(
        levels.iter().copied().collect::<Vec<_>>(),
        vec![0, 15],
        "the square swings between silence and constant volume 15"
    );
    let mut runs = Vec::new();
    let mut run_len = 1usize;
    for w in tail.windows(2) {
        if w[0] == w[1] {
            run_len += 1;
        } else {
            runs.push(run_len);
            run_len = 1;
        }
    }
    // Interior plateaus only: the first and last are clipped by the
    // window.
    assert!(runs.len() >= 4, "too few plateaus measured: {}", runs.len());
    for (i, r) in runs[1..].iter().enumerate() {
        assert_eq!(
            *r, expected_plateau,
            "plateau {i} is {r} half-steps where the program's timer byte says {expected_plateau}"
        );
    }
    eprintln!(
        "sq0_out: {} plateaus of exactly {expected_plateau} half-steps, swinging 0 to 15",
        runs.len() - 1
    );

    // First sound as a value: the run mixed through the authored table
    // into the contract's AudioSamples, one sample per CPU half-step
    // (rate = the exact master clock over 12, times 2 half-steps).
    let mut audio = nes_bus::audio::AudioSamples::new(2 * 236_250_000, 11 * 12);
    for &c in &codes {
        audio.push(mixer::ad1(c, 0), mixer::ad2(0, 0, 0));
    }
    let hi = mixer::ad1(15, 0);
    for (i, &s) in audio.ad1[start..].iter().enumerate() {
        assert!(
            s == 0.0 || s == hi,
            "sample {i} is {s}, neither silence nor mixer::ad1(15, 0)"
        );
    }
    assert!(audio.ad2.iter().all(|&s| s == 0.0), "AD2 must stay silent");
    // A coarse band around the formula's own value (0.1494), so a
    // transcription typo in the constants fails loudly without the
    // test re-typing the formula it is checking.
    assert!(hi > 0.14 && hi < 0.16, "ad1(15,0) = {hi} left the table's plausible band");
}
