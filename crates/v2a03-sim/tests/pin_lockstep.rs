//! The pin-lockstep gate, chip side: the 2A03's 6502 core presented at
//! the pins as a `v6502-pins` `PinFrame`, and held to what a 6502 must
//! do there.
//!
//! This is the first half of the console sketch's "new kind of gate"
//! (N1): chip versus chip through the contract. The 2A03's native unit
//! is the MASTER half-step (`clk_in`); the 6502's `PinFrame` is one per
//! `clk0` phase, so the extractor below samples the pins on every
//! `clk0` transition, which is exactly one 6502 half-cycle. It depends
//! only on `v6502-pins` (the contract, MIT, no die data), never on a
//! 6502 engine: a chip crate does not know what is on the other side of
//! its pins. The cross-chip comparison against a recorded 6502 `.pins`
//! trace, and the decimal-mode divergence, are the second half and live
//! where both chips are reachable (the console layer).
//!
//! What is proven here: run a known program on the core and the pin
//! stream is a conformant 6502's, the reset vector fetched from
//! $FFFC/$FFFD, execution entered at the vector, opcode fetches marked
//! by `sync`, and a store landing as a write on the addressed cell.

use v2a03_sim::harness::Harness;
use v2a03_sim::Cpu;
use v6502_pins::PinFrame;

/// `MUTATE=1` flips R/W's polarity in the extracted frame, the same
/// shape as the 2c02's `MUTATE=rd` on its pin frame: every read presents
/// as a write, so the vector fetch fails its read check and the test
/// must go red. Proof that the pin checks can tell.
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
