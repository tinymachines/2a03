//! N3 step 4's gate: the authored APU against rung 0, every CPU
//! half-step, on the five output codes (`sq0_out`, `sq1_out`, `tri_out`,
//! `noi_out`, `pcm_out`) and the frame IRQ flag, over a register program
//! that exercises every channel (both squares with envelope and sweep,
//! the triangle's linear counter, the noise, the DMC looping a sample)
//! in each frame sequencer mode. Rung 0 runs the program through
//! `CorePins`; the rung runs it on the core with the APU fed from the
//! core's own phi2 writes. The DMC is enabled last so its fetch stalls
//! (step 5, not modelled) fall inside the spin loop and shift no write.
//!
//! `APU_DUMP=1` prints both code streams around the first divergence,
//! which is how `apu::fit`'s constants were measured. SKIPS by name
//! without the die data; REQUIRE_NETLIST=1 insists. `MUTATE=1` reverses
//! the duty table at build time and must go red.

use v2a03_micro::apu::Apu;
use v2a03_sim::pins::CorePins;
use v6502_pins::{Load, PinEngine};

fn mutate() -> bool {
    std::env::var("MUTATE").is_ok_and(|v| v == "1")
}

/// LDA #v ; STA $40xx
fn w(reg: u8, v: u8) -> [u8; 5] {
    [0xa9, v, 0x8d, reg, 0x40]
}

/// Every channel, then the DMC last, then spin. `short` picks the second
/// world: lengths of one and three half frames, envelopes without loop,
/// the triangle's control clear with a linear counter of five, so the
/// length and linear expiries and the envelope's floor all land inside
/// the window; the first world holds long notes so the sweeps run to
/// their mutes and the DMC loops its sample.
fn program(five_step: bool, short: bool) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend(w(0x17, if five_step { 0x80 } else { 0x00 }));
    p.extend(w(0x15, 0x0f));
    if short {
        // Square 0: duty 3, envelope period 1, no loop, length index 3.
        p.extend(w(0x00, 0xc1));
        p.extend(w(0x01, 0x08));
        p.extend(w(0x02, 0x40));
        p.extend(w(0x03, 0x19));
        // Square 1: duty 0, constant 9, length index 5.
        p.extend(w(0x04, 0x19));
        p.extend(w(0x05, 0x00));
        p.extend(w(0x06, 0xf0));
        p.extend(w(0x07, 0x28));
        // Triangle: control clear, linear 5, length index 9.
        p.extend(w(0x08, 0x05));
        p.extend(w(0x0a, 0x30));
        p.extend(w(0x0b, 0x49));
        // Noise: constant 12, mode 1, period index 7, length index 3.
        p.extend(w(0x0c, 0x1c));
        p.extend(w(0x0e, 0x87));
        p.extend(w(0x0f, 0x18));
        // DMC: no loop, rate 12, level $60, sample $C000 x 17 bytes.
        p.extend(w(0x10, 0x0c));
        p.extend(w(0x11, 0x60));
        p.extend(w(0x12, 0x00));
        p.extend(w(0x13, 0x01));
        p.extend(w(0x15, 0x1f));
        let spin = 0x8000 + p.len() as u16;
        p.extend([0x4c, spin as u8, (spin >> 8) as u8]);
        return p;
    }
    // Square 0: duty 2, envelope looping at period 6, sweep negating.
    p.extend(w(0x00, 0xa6));
    p.extend(w(0x01, 0xa9));
    p.extend(w(0x02, 0xab));
    p.extend(w(0x03, 0x09));
    // Square 1: duty 1, halt, constant 15, sweep growing.
    p.extend(w(0x04, 0x7f));
    p.extend(w(0x05, 0x91));
    p.extend(w(0x06, 0x00));
    p.extend(w(0x07, 0x3a));
    // Triangle: control set, linear 64, timer $050.
    p.extend(w(0x08, 0xc0));
    p.extend(w(0x0a, 0x50));
    p.extend(w(0x0b, 0x48));
    // Noise: envelope period 4, mode 0, period index 4.
    p.extend(w(0x0c, 0x04));
    p.extend(w(0x0e, 0x04));
    p.extend(w(0x0f, 0x10));
    // DMC: loop, rate 15, level $20, sample $C000 x 33 bytes; enabled last.
    p.extend(w(0x10, 0x4f));
    p.extend(w(0x11, 0x20));
    p.extend(w(0x12, 0x00));
    p.extend(w(0x13, 0x02));
    p.extend(w(0x15, 0x1f));
    let spin = 0x8000 + p.len() as u16;
    p.extend([0x4c, spin as u8, (spin >> 8) as u8]);
    p
}

fn sample() -> Vec<u8> {
    (0..33u8).map(|i| i.wrapping_mul(0x5b) ^ 0xa5).collect()
}

const STEPS: u64 = 80_000;
const NODES: [(&str, usize); 5] = [("sq0_out", 4), ("sq1_out", 4), ("tri_out", 4), ("noi_out", 4), ("pcm_out", 7)];

fn rung0_codes(loads: &[Load]) -> Vec<([u8; 5], bool)> {
    let mut r0 = CorePins::new(loads, 0x8000);
    r0.power_cycle();
    let nl = r0.har.cpu.engine.netlist().clone();
    let ids: Vec<Vec<_>> = NODES.iter().map(|(p, n)| (0..*n).map(|i| nl.node(&format!("{p}{i}")).expect("out node")).collect()).collect();
    let irq = nl.node("frame_irq").unwrap();
    let read = |r0: &CorePins| {
        let mut c = [0u8; 5];
        for (k, id) in ids.iter().enumerate() {
            c[k] = id.iter().enumerate().map(|(i, &n)| (r0.har.cpu.engine.is_high(n) as u8) << i).sum();
        }
        (c, r0.har.cpu.engine.is_high(irq))
    };
    let mut out = vec![read(&r0)];
    for _ in 0..STEPS {
        r0.half_step();
        out.push(read(&r0));
    }
    out
}

fn rung_codes(loads: &[Load]) -> Vec<([u8; 5], bool)> {
    let mut core = v2a03_micro::core(loads, 0x8000, 0xbd);
    let mut apu = Apu::new();
    let mut out = vec![(apu.codes(), apu.frame_irq)];
    for _ in 0..STEPS {
        core.half_step();
        let f = core.pins();
        if !f.rw && f.clk0 && (0x4000..=0x4017).contains(&f.ab) {
            apu.write((f.ab & 0x1f) as u8, f.db);
        }
        apu.half_step(&core.mem);
        out.push((apu.codes(), apu.frame_irq));
    }
    out
}

#[test]
fn the_five_output_codes_match_rung_0_every_half_step_in_both_frame_modes() {
    if !v2a03_netlist::available() || !v2a03_micro::tables::AVAILABLE {
        if std::env::var_os("REQUIRE_NETLIST").is_some() {
            panic!("REQUIRE_NETLIST=1 but extern/visual2a03 is not fetched (or the tables were built without it)");
        }
        eprintln!("SKIP: extern/visual2a03 not fetched");
        return;
    }
    let dump = std::env::var_os("APU_DUMP").is_some();
    for (five, short) in [(false, false), (true, false), (false, true), (true, true)] {
        let loads = vec![Load { org: 0x8000, bytes: program(five, short) }, Load { org: 0xc000, bytes: sample() }];
        let want = rung0_codes(&loads);
        let got = rung_codes(&loads);
        // MUTATE=1 acts at build time (build.rs reverses the duty table)
        // so the mutation is in the table the rung reads, not in this
        // test's bookkeeping.
        assert!(!mutate() || v2a03_micro::tables::DUTY[0] > v2a03_micro::tables::DUTY[3], "MUTATE=1 but the table is not the mutant; rebuild (cargo rebuilds on the env var)");
        let first = want.iter().zip(&got).position(|(a, b)| a != b);
        if let Some(h) = first {
            let lo = h.saturating_sub(8);
            let hi = (h + 8).min(want.len() - 1);
            let mut s = String::new();
            for i in lo..=hi {
                s.push_str(&format!("    h={i:>6} rung0 {:?} irq={} | apu {:?} irq={}{}\n", want[i].0, want[i].1 as u8, got[i].0, got[i].1 as u8, if i == h { "  <-- first" } else { "" }));
            }
            let diverging = want.iter().zip(&got).filter(|(a, b)| a != b).count();
            if dump {
                // The change events of the first differing channel on
                // both engines, 3000 half-steps either side.
                let ch = (0..5).find(|&k| want[h].0[k] != got[h].0[k]).unwrap_or(0);
                let lo = h.saturating_sub(3000);
                let hi = (h + 3000).min(want.len() - 1);
                let events = |v: &[([u8; 5], bool)]| -> Vec<String> {
                    let mut out = Vec::new();
                    for i in lo.max(1)..=hi {
                        if v[i].0[ch] != v[i - 1].0[ch] {
                            out.push(format!("{i}:{}", v[i].0[ch]));
                        }
                    }
                    out
                };
                eprintln!("channel {ch} changes, rung 0: {}", events(&want).join(" "));
                eprintln!("channel {ch} changes, apu:    {}", events(&got).join(" "));
                eprintln!("{}", s);
            }
            panic!(
                "{}-step mode, {} world: codes diverge at h={h} ({diverging} of {} half-steps differ):\n{s}",
                if five { 5 } else { 4 },
                if short { "short-note" } else { "long-note" },
                want.len()
            );
        }
        eprintln!(
            "{}-step mode, {} world: {} half-steps, five codes and the frame IRQ flag identical to rung 0",
            if five { 5 } else { 4 },
            if short { "short-note" } else { "long-note" },
            want.len()
        );
    }
}
