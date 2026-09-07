//! The joypad clock under a DMC fetch, on the switch-level chip. The
//! controller port's clock is the 2A03's /OE1 pin, the die's `/r4016`,
//! which pulses low on a read of $4016. When the DMC's fetch halts the
//! core in the middle of such a read, the halt and the fetch's cycles
//! re-present the core's read on the bus, and the question the bench's
//! B0 gate asks of the part is whether /OE1 pulses again on each of
//! those cycles, so a controller's 4021 shifts an extra bit. This probe
//! asks the die first.
//!
//! The program starts a looping DMC sample at the fastest rate, strobes
//! the pad, then reads $4016 in a tight loop. For every /r4016 pulse it
//! prints the half-step, the address bus and whether the DMC's RDY was
//! low; at the end, the count of $4016 read instructions against the
//! count of pulses, and the pulses that fell while RDY was low.
//!
//!   cargo run --release -p v2a03-sim --example joy-clock-probe -- [half-steps]
//!
//! NOPS=n pads the loop so the fetch cadence walks other cycles;
//! DUMP=a..b prints every half-step in the range; DECODE=1 reads a list
//! of aliases plainly and reports which pull the strobes low; FETCH16=1
//! runs the DMC over $C016 with the core on NOPs. Measured 2026-09-06:
//! the re-run pulses /OE1 again, the halt cycles do not, and a sample
//! address with the port's low five bits keeps it low through the fetch
//! (the strobe's high bits are the core's held address, its low five the
//! pins'). docs/n3-report.md.

use v2a03_sim::harness::Harness;
use v2a03_sim::Cpu;

/// DECODE=1: which addresses pull /r4016 low on a plain read, so a DMC
/// sample fetch from an alias is understood. The program reads each of
/// a list of addresses once (LDA abs) and the probe reports the pulses
/// by the address on the bus.
fn decode() {
    let addrs: [u16; 12] = [0x4016, 0x4017, 0xc016, 0x8016, 0x0016, 0x4116, 0x4216, 0x4416, 0x4816, 0x5016, 0x6016, 0x4036];
    let mut prog = Vec::new();
    for &a in &addrs {
        prog.extend([0xadu8, a as u8, (a >> 8) as u8]);
    }
    let spin = 0x8000 + prog.len() as u16;
    prog.extend([0x4c, spin as u8, (spin >> 8) as u8]);
    let mut h = Harness::new(Cpu::power_on());
    h.load(0x8000, &prog, 0x8000);
    let nl = h.cpu.engine.netlist().clone();
    let n = |name: &str| nl.node(name).unwrap_or_else(|| panic!("node {name}"));
    let (r4016, r4017) = (n("/r4016"), n("/r4017"));
    let ab: Vec<_> = (0..16).map(|i| n(&format!("ab{i}"))).collect();
    let bits = |h: &Harness, ns: &[halfphi::NodeId]| -> u32 { ns.iter().enumerate().map(|(i, &nd)| (h.cpu.engine.is_high(nd) as u32) << i).sum() };
    let mut low16 = std::collections::BTreeSet::new();
    let mut low17 = std::collections::BTreeSet::new();
    for _ in 0..400 {
        h.half_step();
        let a = bits(&h, &ab) as u16;
        if !h.cpu.engine.is_high(r4016) {
            low16.insert(a);
        }
        if !h.cpu.engine.is_high(r4017) {
            low17.insert(a);
        }
    }
    println!("addresses read: {:?}", addrs.iter().map(|a| format!("{a:04x}")).collect::<Vec<_>>());
    println!("/r4016 low while the bus showed: {:?}", low16.iter().map(|a| format!("{a:04x}")).collect::<Vec<_>>());
    println!("/r4017 low while the bus showed: {:?}", low17.iter().map(|a| format!("{a:04x}")).collect::<Vec<_>>());
}

/// FETCH16=1: the DMC fetching from $C010.. while the core spins on
/// NOPs, never reading the pad: does /r4016 fall when the sample
/// fetch's address is $C016?
fn fetch16() {
    let mut prog: Vec<u8> = Vec::new();
    for (r, v) in [(0x10u8, 0x4fu8), (0x12, 0x00), (0x13, 0xff), (0x15, 0x10)] {
        prog.extend([0xa9, v, 0x8d, r, 0x40]);
    }
    let spin = 0x8000 + prog.len() as u16;
    prog.extend([0xea, 0xea, 0x4c, spin as u8, (spin >> 8) as u8]);
    let mut h = Harness::new(Cpu::power_on());
    h.load(0x8000, &prog, 0x8000);
    for i in 0..0x2000usize {
        h.memory[0xc000 + i] = (i as u8).wrapping_mul(7);
    }
    let nl = h.cpu.engine.netlist().clone();
    let n = |name: &str| nl.node(name).unwrap_or_else(|| panic!("node {name}"));
    let (r4016, pcm_rdy) = (n("/r4016"), n("pcm_dma_/rdy"));
    let ab: Vec<_> = (0..16).map(|i| n(&format!("ab{i}"))).collect();
    let bits = |h: &Harness, ns: &[halfphi::NodeId]| -> u32 { ns.iter().enumerate().map(|(i, &nd)| (h.cpu.engine.is_high(nd) as u32) << i).sum() };
    let (mut fetches, mut lows) = (0u32, Vec::new());
    let mut prev_rdy = true;
    for step in 0..24_000usize {
        h.half_step();
        let pr = h.cpu.engine.is_high(pcm_rdy);
        if !pr && prev_rdy {
            fetches += 1;
        }
        prev_rdy = pr;
        if !h.cpu.engine.is_high(r4016) {
            lows.push((step, bits(&h, &ab) as u16, pr));
        }
    }
    println!("{fetches} DMC fetches with the core on NOPs; /r4016 low at {} half-steps: {:?}", lows.len(), lows.iter().take(8).map(|(s, a, pr)| format!("h={s} ab={a:04x} pcm_dma_/rdy={}", *pr as u8)).collect::<Vec<_>>());
}

fn main() {
    if std::env::var_os("FETCH16").is_some() {
        fetch16();
        return;
    }
    if std::env::var_os("DECODE").is_some() {
        decode();
        return;
    }
    let steps: usize = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(24_000);
    // LDA #$4F; STA $4010 (loop, rate 15)  LDA #$00; STA $4012 ($C000)
    // LDA #$FF; STA $4013                   LDA #$10; STA $4015 (DMC on)
    // LDA #$01; STA $4016; LDA #$00; STA $4016 (strobe)
    // loop: LDA $4016; JMP loop
    let prog: Vec<u8> = vec![
        0xa9, 0x4f, 0x8d, 0x10, 0x40, 0xa9, 0x00, 0x8d, 0x12, 0x40, 0xa9, 0xff, 0x8d, 0x13, 0x40, 0xa9, 0x10, 0x8d, 0x15, 0x40, 0xa9, 0x01, 0x8d, 0x16, 0x40, 0xa9, 0x00, 0x8d, 0x16, 0x40,
    ];
    let loop_at = 0x8000 + prog.len() as u16;
    let mut prog = prog;
    // NOPS=n pads the loop so the fetch cadence walks other cycles.
    let nops: usize = std::env::var("NOPS").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    prog.extend([0xad, 0x16, 0x40]);
    prog.extend(std::iter::repeat_n(0xea, nops));
    prog.extend([0x4c, loop_at as u8, (loop_at >> 8) as u8]);
    let mut h = Harness::new(Cpu::power_on());
    h.load(0x8000, &prog, 0x8000);
    // The sample: $C000..$DFFF, clear of the vectors.
    for i in 0..0x2000usize {
        h.memory[0xc000 + i] = (i as u8).wrapping_mul(7);
    }
    let nl = h.cpu.engine.netlist().clone();
    let n = |name: &str| nl.node(name).unwrap_or_else(|| panic!("node {name}"));
    let (r4016, pcm_rdy, rdy, sync, clk0) = (n("/r4016"), n("pcm_dma_/rdy"), n("rdy"), n("sync"), n("clk0"));
    let ab: Vec<_> = (0..16).map(|i| n(&format!("ab{i}"))).collect();
    let bits = |h: &Harness, ns: &[halfphi::NodeId]| -> u32 { ns.iter().enumerate().map(|(i, &nd)| (h.cpu.engine.is_high(nd) as u32) << i).sum() };
    // DUMP=a..b prints every half-step in the range.
    let dump: Option<(usize, usize)> = std::env::var("DUMP").ok().and_then(|v| {
        let (a, b) = v.split_once("..")?;
        Some((a.parse().ok()?, b.parse().ok()?))
    });
    let rw = n("rw");
    let (mut prev_r, mut prev_sync, mut prev_clk) = (true, false, false);
    let (mut reads, mut pulses, mut stalled_pulses) = (0u32, 0u32, 0u32);
    let mut fetches_seen = 0u32;
    let mut per_read = Vec::new();
    let mut in_stall = false;
    // Per read: the sample address a fetch took while the core held
    // $4016, if one did.
    let mut collided: Vec<Option<u16>> = Vec::new();
    for step in 0..steps {
        h.half_step();
        let a = bits(&h, &ab) as u16;
        let s = h.cpu.engine.is_high(sync);
        let c = h.cpu.engine.is_high(clk0);
        if s && !prev_sync && std::env::var_os("SYNCS").is_some() {
            println!("  sync at h={step} ab={a:04x}");
        }
        if s && !prev_sync && a == loop_at {
            reads += 1;
            per_read.push(0u32);
            collided.push(None);
        }
        let r = h.cpu.engine.is_high(r4016);
        let pr = h.cpu.engine.is_high(pcm_rdy);
        if dump.is_some_and(|(a0, b0)| step >= a0 && step <= b0) {
            println!("  h={step} clk0={} ab={a:04x} rw={} sync={} rdy={} pcm_dma_/rdy={} /r4016={}", c as u8, h.cpu.engine.is_high(rw) as u8, s as u8, h.cpu.engine.is_high(rdy) as u8, pr as u8, r as u8);
        }
        if !pr && !in_stall {
            in_stall = true;
            fetches_seen += 1;
        }
        if pr {
            in_stall = false;
        }
        if !pr && a >= 0xc000 && h.cpu.engine.is_high(rw) {
            if let Some(last) = collided.last_mut() {
                *last = Some(a);
            }
        }
        if !r && prev_r {
            pulses += 1;
            if let Some(last) = per_read.last_mut() {
                *last += 1;
            }
            if !pr {
                stalled_pulses += 1;
            }
            if !pr || per_read.last().is_some_and(|&k| k > 1) {
                println!("  h={step} clk0={} ab={a:04x} /r4016 fell, pcm_dma_/rdy={} rdy={} (read #{reads}, pulse {} of it)", c as u8, pr as u8, h.cpu.engine.is_high(rdy) as u8, per_read.last().copied().unwrap_or(0));
            }
        }
        prev_r = r;
        prev_sync = s;
        prev_clk = c;
    }
    let _ = prev_clk;
    let multi = per_read.iter().filter(|&&k| k > 1).count();
    for (i, c) in collided.iter().enumerate() {
        if let Some(addr) = c {
            if per_read[i] > 1 || addr & 0x1f == 0x16 {
                println!("  read #{}: a fetch from ${addr:04x} landed in it, {} /r4016 pulse(s)", i + 1, per_read[i]);
            }
        }
    }
    let mut hist = std::collections::BTreeMap::new();
    for &k in &per_read {
        *hist.entry(k).or_insert(0u32) += 1;
    }
    println!("{reads} LDA $4016 fetched, {pulses} /r4016 pulses, {fetches_seen} DMC fetches (RDY low spans), {stalled_pulses} pulses while RDY low, {multi} reads with more than one pulse; pulses per read: {hist:?}");
}
