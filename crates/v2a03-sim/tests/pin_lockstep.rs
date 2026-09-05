//! The pin-lockstep gate: the 2A03's 6502 core presented at the pins as
//! a `v6502-pins` `PinFrame`, held first to what a 6502 must do there
//! (the chip side, below) and then, chip against chip, to the 6502's own
//! recorded pin golden: all 274 traces the 6502 repository records from
//! its rung 0 (seven programs, the reference's program, the scripted
//! interrupt and reset runs, the three decimal chains, every opcode),
//! replayed through this core by the contract's own `run`, with every
//! differing field classified by a named rule (`v2a03_sim::lockstep`)
//! and anything unnamed failing the gate.
//!
//! The 2A03's native unit is the MASTER half-step (`clk_in`); the 6502's
//! `PinFrame` is one per `clk0` phase, so `CorePins` samples on every
//! `clk0` transition, which is exactly one 6502 half-cycle. The crate
//! depends only on `v6502-pins` (the contract, MIT, no die data), never
//! on a 6502 engine: the other chip enters as recorded text, which is
//! what a pin golden is for. The recordings are the 6502 repository's own
//! gitignored files (NC-SA-derived, never committed), read from a sibling
//! checkout or `PIN_GOLDEN=<dir>`; without them the cross-chip test
//! SKIPS by name and `REQUIRE_PINS=1` insists.
//!
//! What the cross-chip half proves, and names (docs/n3-report.md):
//! every trace agrees pin for pin except (1) the stack page, offset by
//! the two dies' simulated power-on stack pointers and by nothing else,
//! (2) the data byte in a write's phi1, where nothing is serviced, (3)
//! the three decimal chains, where the 2A03 stores the binary sums and
//! binary flags the 6502 adjusts, listed byte by byte, and (4) the
//! reset-mid-run script, where the 2A03 holds its core still under RES
//! while the 6502 runs its hijacked BRK on, both reading the vector at
//! the same half-cycle. Two scripts drive RDY or SO, pins this chip does
//! not have, and are refused by name.
//!
//! `MUTATE=1` flips R/W's polarity in the chip-side extractor (every read
//! presents as a write, so the vector fetch fails its read check) and, in
//! the cross-chip half, one bit of one serviced byte in the first
//! compared trace, which must surface as a loud `data` difference.

use std::path::PathBuf;

use v2a03_sim::harness::Harness;
use v2a03_sim::lockstep::{classify, stack_offset, DATA, S_LEAK, STACK, WRITE_PHI1};
use v2a03_sim::pins::CorePins;
use v2a03_sim::Cpu;
use v6502_pins::{line, parse_stim, parse_trace, run, PinEngine, PinFrame};

fn mutate() -> bool {
    std::env::var("MUTATE").is_ok_and(|v| v == "1")
}

/// Run `prog` at `load` (reset vector pointed there) and return one
/// `PinFrame` per 6502 half-cycle (per `clk0` phase), for `frames`
/// frames after power-on. `h` counts clk0 phases from the first one
/// seen, the extractor's own origin (the cross-chip half aligns two
/// such streams; here it is just a frame index).
fn extract(prog: &[u8], load: u16, frames: usize) -> Vec<PinFrame> {
    let mut har = Harness::new(Cpu::power_on());
    har.load(load, prog, load);
    let nl = har.cpu.engine.netlist().clone();
    let n = |name: &str| nl.node(name).unwrap_or_else(|| panic!("node {name}"));
    let clk0 = n("clk0");
    let sync = n("sync");
    let rw = n("rw");
    let (res, irq, nmi, rdy, so) = (n("res"), n("irq"), n("nmi"), n("rdy"), n("so"));
    let ab: Vec<_> = (0..16).map(|i| n(&format!("ab{i}"))).collect();
    let db: Vec<_> = (0..8).map(|i| n(&format!("db{i}"))).collect();
    let bits = |har: &Harness, ns: &[halfphi::NodeId]| -> u32 {
        ns.iter()
            .enumerate()
            .map(|(i, &nd)| (har.cpu.engine.is_high(nd) as u32) << i)
            .sum()
    };

    let mut out = Vec::with_capacity(frames);
    let mut prev = har.cpu.engine.is_high(clk0);
    let mut h = 0u64;
    // A generous master-half-step budget: clk0 divides clk_in by 12, so
    // a phase is 12 master steps; `frames` phases need about 12x, plus
    // the reset run-in.
    let budget = (frames as u64 + 40) * 14 + 4000;
    for _ in 0..budget {
        har.half_step();
        let c = har.cpu.engine.is_high(clk0);
        if c != prev {
            prev = c;
            out.push(PinFrame {
                h,
                clk0: c,
                ab: bits(&har, &ab) as u16,
                db: bits(&har, &db) as u8,
                rw: har.cpu.engine.is_high(rw) != mutate(),
                sync: har.cpu.engine.is_high(sync),
                res: har.cpu.engine.is_high(res),
                irq: har.cpu.engine.is_high(irq),
                nmi: har.cpu.engine.is_high(nmi),
                rdy: har.cpu.engine.is_high(rdy),
                so: har.cpu.engine.is_high(so),
            });
            h += 1;
            if out.len() == frames {
                break;
            }
        }
    }
    out
}

#[test]
fn the_core_presents_a_conformant_6502_at_the_pins() {
    if !v2a03_netlist::available() {
        eprintln!("SKIP: extern/visual2a03 not fetched");
        return;
    }
    // LDA #$42; STA $0200; JMP $8000.
    let prog = [0xa9u8, 0x42, 0x8d, 0x00, 0x02, 0x4c, 0x00, 0x80];
    let frames = extract(&prog, 0x8000, 260);

    // The reset vector is fetched from $FFFC/$FFFD and reads $00/$80.
    let vlo = frames.iter().find(|f| f.ab == 0xfffc).expect("no $FFFC fetch");
    let vhi = frames.iter().find(|f| f.ab == 0xfffd).expect("no $FFFD fetch");
    assert!(vlo.rw && vhi.rw, "the vector fetch must be a read");
    assert_eq!((vlo.db, vhi.db), (0x00, 0x80), "reset vector bytes");

    // Execution enters at the vector: the first sync-high fetch after
    // the vector read is $8000, reading the first opcode $A9.
    let vpos = frames.iter().position(|f| f.ab == 0xfffd).unwrap();
    let first_fetch = frames[vpos..]
        .iter()
        .find(|f| f.sync && f.rw)
        .expect("no opcode fetch after the vector");
    assert_eq!(first_fetch.ab, 0x8000, "execution enters at the reset vector");
    assert_eq!(first_fetch.db, 0xa9, "first opcode is LDA #imm");

    // Every sync-high fetch in the loop body sits at one of the three
    // opcode addresses ($8000 LDA, $8002 STA, $8005 JMP): the core is
    // not fetching opcodes from operand or data addresses.
    let opcode_addrs = [0x8000u16, 0x8002, 0x8005];
    for f in frames[vpos..].iter().filter(|f| f.sync && f.rw) {
        assert!(
            opcode_addrs.contains(&f.ab),
            "an opcode fetch landed at {:04x}, not an instruction boundary",
            f.ab
        );
    }

    // The STA lands as a write of $42 to $0200. The write data is valid
    // on the phi2 (clk0 high) half-cycle; the phi1 half still carries
    // the address low byte on the bus, so the check is per write cycle,
    // not per frame.
    assert!(
        frames.iter().any(|f| f.ab == 0x0200 && !f.rw && f.clk0 && f.db == 0x42),
        "no phi2 write of $42 to $0200"
    );

    // The interrupt and ready inputs stand inactive (high) across the
    // captured window: the reset that drove `res` low ran inside
    // `power_on`, before capture, and nothing asserts irq/nmi/rdy in a
    // plain run. SO is the exception, and deliberately: the reference's
    // own initChip does `setLow('so')` and never releases it, so the
    // 2A03 harness holds SO low throughout. It is inert (SO acts only on
    // a high-to-low edge; held low from the start, it never fires), and
    // the cross-chip half drives both engines' SO alike rather than
    // relying on a rest level.
    assert!(
        frames.iter().all(|f| f.res && f.irq && f.nmi && f.rdy),
        "res/irq/nmi/rdy must stand inactive in a plain run"
    );
    assert!(
        frames.iter().all(|f| !f.so),
        "SO is held low by the reference reset recipe; a high SO means power_on changed"
    );

    eprintln!(
        "{} frames: reset vector $8000, LDA/STA/JMP fetched at boundaries, STA wrote $42 to $0200",
        frames.len()
    );
}

// ---------------------------------------------------------------------------
// The cross-chip half.
// ---------------------------------------------------------------------------

/// Scripts that drive pins the 2A03 does not have. RDY is an internal
/// node its DMA units own (three scripts drive it: the stall, the fall
/// inside a write cycle, the phi1 release); SO is an unbonded pad the
/// reference holds low.
const ABSENT_PIN_TRACES: &[&str] = &["fixture-rdy-stall", "fixture-rdy-in-write", "fixture-rdy-release-phi1", "fixture-so-pulse"];

/// The trace whose program copies S into X (TSX at $0207) and then runs
/// the `$34 $12` operand bytes as `NOP zp,X`: the power-on stack pointer
/// reaches a page-0 address there, the one place it leaks off page 1.
const S_LEAK_TRACE: &str = "op-ba";

/// The decimal chains: every differing serviced byte must be one of
/// these (address, the 6502's adjusted byte, the 2A03's binary byte),
/// in this order. The arithmetic is the program's own operands
/// (`pin-golden.rs`, `decimal_cases`), so the expectation is derived,
/// not remembered: 19+28, 09+01, 99+99+1 (=$133), 50+50, then the PHP
/// after the last add (binary $a0 carries nothing, so C clears; V stays);
/// 42-13, 10-05, 00-01 under SEC; 1f+01 (the invalid-BCD add; the
/// 9a-00, the CMP under D and the post-CLD add already agreed).
type Store = (u16, u8, u8);
const DECIMAL_STORES: &[(&str, &[Store])] = &[
    ("decimal-adc", &[(0x0080, 0x47, 0x41), (0x0081, 0x10, 0x0a), (0x0082, 0x99, 0x33), (0x0083, 0x00, 0xa0), (0x01fa, 0xfd, 0xfc)]),
    ("decimal-sbc", &[(0x0080, 0x29, 0x2f), (0x0081, 0x05, 0x0b), (0x0082, 0x99, 0xff)]),
    ("decimal-mixed", &[(0x0080, 0x26, 0x20)]),
];

const RESET_TRACE: &str = "fixture-reset-mid-run";

/// Traces whose program writes S (TXS, and SHS/TAS) before its first
/// stack access: from there both cores hold the same S, so their own
/// stack offset reads 0 and the stack page agrees outright.
const S_SETTERS: &[&str] = &["op-9a", "op-9b"];

fn pin_golden_dir() -> PathBuf {
    match std::env::var_os("PIN_GOLDEN") {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../6502/tools/pin-golden"),
    }
}

#[test]
fn the_core_replays_the_6502s_pin_golden_up_to_four_named_divergences() {
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
        eprintln!("SKIP: no 6502 pin golden at {} (record it there with `cargo run --release -p v6502-pins --example pin-golden`, or point PIN_GOLDEN at it)", dir.display());
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "pins"))
        .collect();
    paths.sort();
    assert!(paths.len() >= 270, "the pin golden is {} traces; the 6502 records 274", paths.len());

    let mut compared = 0usize;
    let mut exact = 0usize;
    let mut refused = Vec::new();
    let mut offset_seen: Option<u8> = None;
    let mut s_at_h0: Option<(u8, u8)> = None;
    let mut totals = std::collections::BTreeMap::<&str, usize>::new();
    let mut decimal_seen = 0usize;
    let mut reset_seen = false;
    let mut leak_seen = false;
    for path in &paths {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let trace = parse_trace(&std::fs::read_to_string(path).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let stim = if trace.header.stim.is_empty() {
            Vec::new()
        } else {
            parse_stim(&std::fs::read_to_string(path.with_extension("stim")).unwrap()).unwrap()
        };
        let mut core = CorePins::new(&trace.header.loads, trace.header.reset_vector);
        // The stack pointer at h=0 on this die, read off the register
        // nodes, so the offset the classifier allows is a measurement
        // and not a number.
        core.power_cycle();
        let s_core = core.stack_pointer();
        let mut got = run(&mut core, trace.frames.len() as u64 - 1, &stim);
        if core.absent_pin_driven {
            assert!(ABSENT_PIN_TRACES.contains(&name.as_str()), "{name} drives RDY or SO and is not on the absent-pin list");
            refused.push(name);
            continue;
        }
        assert!(!ABSENT_PIN_TRACES.contains(&name.as_str()), "{name} is listed as driving an absent pin but did not");
        assert!(!core.clock_stopped, "{name}: clk0 stopped under the script");
        if mutate() && compared == 0 {
            // One bit of one SERVICED byte (a read's phi1), so the flip
            // cannot hide in the write-phi1 class.
            let f = got.iter_mut().skip(30).find(|f| f.rw && !f.clk0).expect("a read to mutate");
            f.db ^= 1;
        }
        compared += 1;

        let own = stack_offset(&trace.frames, &got);
        let offset = match offset_seen {
            None => {
                // The recording's first stack access is a push AT S, so
                // its low byte is the 6502's S at h=0; the difference must
                // be exactly what the two dies' registers say.
                let o = own.expect("the first trace must touch the stack");
                let s_rec = trace.frames.iter().find(|f| f.ab >> 8 == 1).unwrap().ab as u8;
                assert_eq!(o, s_rec.wrapping_sub(s_core), "{name}: the stack offset is not the two S registers' difference");
                offset_seen = Some(o);
                s_at_h0 = Some((s_rec, s_core));
                o
            }
            Some(o) => {
                match own {
                    Some(x) if x == o => {}
                    Some(0) => assert!(S_SETTERS.contains(&name.as_str()), "{name}: the stack page agrees outright, but the program is not listed as writing S"),
                    Some(x) => panic!("{name}: stack offset {x:#04x} where every other trace shows {o:#04x}"),
                    None => {}
                }
                o
            }
        };
        let rep = classify(&trace.frames, &got, offset, name == S_LEAK_TRACE);
        for (k, v) in &rep.counts {
            *totals.entry(k).or_default() += v;
        }
        if rep.counts.is_empty() {
            exact += 1;
        }
        if name == S_LEAK_TRACE {
            assert_eq!(rep.count(S_LEAK), 2, "{name}: TSX leaks S into exactly the two phases of one NOP zp,X read");
            leak_seen = true;
        }

        let loud_lines = || rep.loud.iter().take(6).map(|l| format!("  h={} {:?}\n    6502 {}\n    2a03 {}", l.h, l.classes, line(&l.expected), line(&l.got))).collect::<Vec<_>>().join("\n");
        if let Some((_, stores)) = DECIMAL_STORES.iter().find(|(n, _)| *n == name) {
            decimal_seen += 1;
            let got_stores: Vec<Store> = rep
                .loud
                .iter()
                .map(|l| {
                    assert_eq!(l.classes.iter().filter(|c| **c != STACK).collect::<Vec<_>>(), vec![&DATA], "{name} h={}: a decimal chain may differ only in serviced data\n{}", l.h, loud_lines());
                    assert!(!l.expected.rw && l.expected.clk0, "{name} h={}: the differing byte is not a write's phi2", l.h);
                    (l.expected.ab, l.expected.db, l.got.db)
                })
                .collect();
            assert_eq!(got_stores, stores.to_vec(), "{name}: the binary stores are not the listed ones");
            continue;
        }
        if name == RESET_TRACE {
            reset_seen = true;
            // RES low from the script's first assertion; the 6502 runs
            // its hijacked BRK to the vector select and freewheels, the
            // 2A03 holds still and freewheels after release; both must
            // read $FFFC at the same h and agree from the reset's three
            // stack reads (six phases before it) onward.
            let res_low = trace.frames.iter().position(|f| !f.res).expect("RES asserted");
            let vec_rec = trace.frames.iter().skip(res_low).position(|f| f.ab == 0xfffc && f.rw).map(|i| i + res_low).expect("6502 vector read");
            let vec_got = got.iter().skip(res_low).position(|f| f.ab == 0xfffc && f.rw).map(|i| i + res_low).expect("2a03 vector read");
            assert_eq!(vec_rec, vec_got, "{name}: the two cores read the reset vector at different half-cycles");
            for l in &rep.loud {
                assert!(l.h > res_low && l.h < vec_rec - 6, "{name} h={}: a difference outside the reset window ({}..{})\n{}", l.h, res_low + 1, vec_rec - 6, loud_lines());
            }
            assert!(!rep.loud.is_empty(), "{name}: the reset window showed no difference, so the named divergence is gone; re-measure");
            eprintln!("{name}: {} frames differ inside the reset window h={}..{}, vector read at h={vec_rec} on both", rep.loud.len(), res_low + 1, vec_rec - 7);
            continue;
        }
        assert!(rep.loud.is_empty(), "{name}: {} unnamed difference(s)\n{}", rep.loud.len(), loud_lines());
    }
    assert_eq!(refused.len(), ABSENT_PIN_TRACES.len(), "refused {refused:?}");
    assert_eq!(decimal_seen, DECIMAL_STORES.len(), "not every decimal chain was compared");
    assert!(reset_seen && leak_seen, "the named traces must all be present");
    assert!(totals.get(WRITE_PHI1).copied().unwrap_or(0) > 0, "no write-phi1 frame differed; the class is gone, re-measure");
    let (s_rec, s_core) = s_at_h0.unwrap();
    eprintln!(
        "cross-chip: {compared} traces compared, {} refused by name ({}), {exact} exact in every field; stack pointer at h=0 ${s_rec:02x} on the 6502 and ${s_core:02x} on the 2A03; per class {totals:?}",
        refused.len(),
        refused.join(", ")
    );
}
