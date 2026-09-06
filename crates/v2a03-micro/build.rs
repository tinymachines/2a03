//! The APU tables, measured out of rung 0 at build time and written to
//! OUT_DIR (NC-SA-derived like everything read off the die data; never
//! committed). Each measurement is the corresponding step 3 probe
//! restated (`v2a03-sim/examples/apu-*-probe.rs`), so a number here can
//! be checked against a probe's printout by hand:
//!
//!   LENGTH[32]        sq0_len as loaded by $4003 with each index
//!   NOISE_PERIOD[16]  half-steps between LFSR shifts per $400E index
//!   DMC_RATE[16]      half-steps between sample-bit shifts per $4010 index
//!   DUTY[4]           bit c set where sq0_out is high on sequencer step c
//!   FRAME_4 / FRAME_5 (half-steps after the $4017 write strobe, phase)
//!                     over one period, and the period, per mode
//!   FRAME_POWER_ON    half-steps from the pin contract's h=0 to the first
//!                     phase_a with no $4017 write
//!   NOISE_TIMER, DMC_TIMER   the two LFSR-shaped timers (`noi_t`, 11 bits;
//!                     `pcm_t`, 9 bits): the value at h=0, the feedback
//!                     taps fitted over their free run, the terminal state
//!                     at which they reload, and the reload value per
//!                     $400E index / $4010 rate
//!
//! Without the die data the tables are written empty with AVAILABLE =
//! false and every consumer SKIPS by name. About two to three minutes
//! with it (the noise table waits out the timer's power-on state sixteen
//! times).

use std::fmt::Write as _;
use std::path::PathBuf;

use v2a03_sim::harness::Harness;
use v2a03_sim::Cpu;

struct P {
    h: Harness,
}

impl P {
    fn new(prog: &[u8]) -> P {
        let mut h = Harness::new(Cpu::power_on());
        let mut p = prog.to_vec();
        let spin = 0x8000 + p.len() as u16;
        p.extend([0x4c, spin as u8, (spin >> 8) as u8]);
        h.load(0x8000, &p, 0x8000);
        P { h }
    }
    fn hi(&self, name: &str) -> bool {
        let n = self.h.cpu.engine.netlist().node(name).unwrap_or_else(|| panic!("node {name}"));
        self.h.cpu.engine.is_high(n)
    }
    fn bits(&self, prefix: &str, n: usize) -> u32 {
        (0..n).map(|i| (self.hi(&format!("{prefix}{i}")) as u32) << i).sum()
    }
    fn step(&mut self) {
        self.h.half_step();
    }
    fn until_write(&mut self, strobe: &str, limit: usize) -> usize {
        let mut prev = self.hi(strobe);
        for i in 0..limit {
            self.step();
            let v = self.hi(strobe);
            if v && !prev {
                return i;
            }
            prev = v;
        }
        panic!("{strobe} never rose");
    }
}

fn w(reg: u8, v: u8) -> [u8; 5] {
    [0xa9, v, 0x8d, reg, 0x40]
}

fn length() -> [u8; 32] {
    let mut prog = Vec::new();
    prog.extend(w(0x15, 0x01));
    prog.extend(w(0x00, 0x30));
    for i in 0..32u8 {
        prog.extend(w(0x03, i << 3));
    }
    let mut p = P::new(&prog);
    let mut out = [0u8; 32];
    for slot in out.iter_mut() {
        p.until_write("w4003", 4000);
        for _ in 0..6 {
            p.step();
        }
        *slot = p.bits("sq0_len", 8) as u8;
    }
    out
}

/// A unit and its LFSR timer, by node name: the unit's nodes (`prefix`,
/// `n` bits) shift once per period; the timer (`timer`, `tbits`) reloads
/// after standing at `terminal`.
struct Watch<'a> {
    strobe: &'a str,
    prefix: &'a str,
    n: usize,
    timer: &'a str,
    tbits: usize,
    terminal: u32,
}

/// Half-steps between the first two changes of the unit's nodes after
/// the write, and the timer's reload: the first value it takes after
/// standing at its terminal.
fn shift_period(prog: &[u8], w: &Watch, limit: usize) -> (u32, u32) {
    let (strobe, prefix, n, timer, tbits, terminal) = (w.strobe, w.prefix, w.n, w.timer, w.tbits, w.terminal);
    let mut p = P::new(prog);
    p.until_write(strobe, 6000);
    let mut prev = p.bits(prefix, n);
    let mut last = None;
    let mut t_prev = p.bits(timer, tbits);
    let mut reload = None;
    let mut period = None;
    for i in 0..limit {
        p.step();
        let t = p.bits(timer, tbits);
        if t != t_prev {
            if t_prev == terminal {
                reload.get_or_insert(t);
            }
            t_prev = t;
        }
        let v = p.bits(prefix, n);
        if v != prev {
            if let Some(l) = last {
                period.get_or_insert((i - l) as u32);
            }
            last = Some(i);
            prev = v;
        }
        if let (Some(a), Some(b)) = (period, reload) {
            return (a, b);
        }
    }
    panic!("{prefix}: fewer than two shifts or no reload in {limit} half-steps");
}

/// An LFSR-shaped timer, measured: its value at the contract's h=0, the
/// feedback taps that explain every free-running step, the one state
/// whose successor breaks the rule (the terminal), and where it goes from
/// there (the reload for whatever rate stands). Observed over the first
/// `ticks` changes with no register written.
struct LfsrTimer {
    at_h0: u32,
    taps: (u8, u8),
    terminal: u32,
}

fn lfsr_timer(prefix: &str, bits: usize, ticks: usize) -> LfsrTimer {
    let mut p = P::new(&[]);
    for _ in 0..v2a03_sim::pins::ALIGN_PHASES {
        p.step();
    }
    let at_h0 = p.bits(prefix, bits);
    let mask = (1u32 << bits) - 1;
    let mut pairs = Vec::new();
    let mut prev = at_h0;
    while pairs.len() < ticks {
        p.step();
        let v = p.bits(prefix, bits);
        if v != prev {
            pairs.push((prev, v));
            prev = v;
        }
    }
    type Fit = ((u8, u8), Vec<(u32, u32)>);
    let mut best: Option<Fit> = None;
    for i in 0..bits as u8 {
        for j in 0..i {
            let bad: Vec<(u32, u32)> = pairs
                .iter()
                .copied()
                .filter(|&(a, b)| ((a << 1) & mask) | ((a >> i & 1) ^ (a >> j & 1)) != b)
                .collect();
            if best.as_ref().is_none_or(|(_, b)| bad.len() < b.len()) {
                best = Some(((i, j), bad));
            }
        }
    }
    let ((i, j), bad) = best.unwrap();
    let terminals: std::collections::BTreeSet<u32> = bad.iter().map(|&(a, _)| a).collect();
    assert_eq!(terminals.len(), 1, "{prefix}: taps ({i},{j}) leave {} unexplained states {terminals:?}, not one terminal", terminals.len());
    LfsrTimer { at_h0, taps: (i, j), terminal: *terminals.iter().next().unwrap() }
}

fn noise_period(terminal: u32) -> ([u16; 16], [u16; 16]) {
    let mut out = [0u16; 16];
    let mut reload = [0u16; 16];
    for f in 0..16usize {
        let mut prog = Vec::new();
        prog.extend(w(0x15, 0x08));
        prog.extend(w(0x0c, 0x3f));
        prog.extend(w(0x0e, f as u8));
        prog.extend(w(0x0f, 0x00));
        let (a, b) = shift_period(&prog, &Watch { strobe: "w400f", prefix: "noi_c", n: 15, timer: "noi_t", tbits: 11, terminal }, 40_000);
        out[f] = a as u16;
        reload[f] = b as u16;
    }
    (out, reload)
}

fn dmc_rate(terminal: u32) -> ([u16; 16], [u16; 16]) {
    let mut out = [0u16; 16];
    let mut reload = [0u16; 16];
    for r in 0..16usize {
        let mut prog = Vec::new();
        prog.extend(w(0x10, r as u8));
        prog.extend(w(0x11, 0x40));
        prog.extend(w(0x12, 0x00));
        prog.extend(w(0x13, 0x01));
        prog.extend(w(0x15, 0x10));
        let (a, b) = shift_period(&prog, &Watch { strobe: "w4015", prefix: "pcm_bits", n: 3, timer: "pcm_t", tbits: 9, terminal }, 8000);
        out[r] = a as u16;
        reload[r] = b as u16;
    }
    (out, reload)
}

fn duty() -> [u8; 4] {
    let mut out = [0u8; 4];
    for (d, slot) in out.iter_mut().enumerate() {
        let mut prog = Vec::new();
        prog.extend(w(0x15, 0x01));
        prog.extend(w(0x00, ((d as u8) << 6) | 0x3f));
        prog.extend(w(0x02, 0x08));
        prog.extend(w(0x03, 0x00));
        let mut p = P::new(&prog);
        p.until_write("w4003", 4000);
        let mut prev_c = p.bits("sq0_c", 3);
        let mut seen = 0u8;
        for _ in 0..1200 {
            p.step();
            let c = p.bits("sq0_c", 3);
            if c != prev_c {
                // Sample mid-step so the output has settled on this step.
                for _ in 0..4 {
                    p.step();
                }
                if p.bits("sq0_out", 4) == 15 {
                    *slot |= 1 << c;
                }
                seen |= 1 << c;
                prev_c = p.bits("sq0_c", 3);
                if seen == 0xff {
                    break;
                }
            }
        }
        assert_eq!(seen, 0xff, "duty {d}: not every sequencer step was seen");
    }
    out
}

const PHASES: [&str; 5] = ["frm_phase_a", "frm_phase_b", "frm_phase_c", "frm_phase_d", "frm_phase_e"];

/// Half-steps the other write parity adds to every frame event
/// (measured by `apu-write-probe`; asserted on every phase below).
const FRAME_JITTER: u32 = 2;

/// A mode-1 write clocks the quarter and half frames at once: the
/// half-step after the strobe on which `frm_/half` falls (the even
/// parity; the other is FRAME_JITTER later, asserted).
fn mode1_clock() -> u32 {
    let mut at = [0u32; 2];
    for (k, odd) in [false, true].into_iter().enumerate() {
        let mut prog: Vec<u8> = if odd { vec![0x85, 0x00] } else { Vec::new() };
        prog.extend(w(0x17, 0x80));
        let mut p = P::new(&prog);
        p.until_write("w4017", 4000);
        let mut i = 0u32;
        loop {
            p.step();
            i += 1;
            if !p.hi("frm_/half") {
                at[k] = i;
                break;
            }
            assert!(i < 100, "mode 1: the write did not clock the half frame within a hundred half-steps");
        }
    }
    assert_eq!(at[1], at[0] + FRAME_JITTER, "mode 1: the immediate clock falls at {} and {} by parity, not {} apart", at[0], at[1], FRAME_JITTER);
    at[0]
}

/// (offset, phase) rises after the write strobe until the sequence repeats
/// (phase_a seen twice), plus the period between the two phase_a rises.
/// `odd` puts a three-cycle instruction before the write, so the write
/// lands on the other APU cycle parity (a two-cycle NOP would not move
/// it: an APU cycle is two CPU cycles).
fn frame(mode: u8, odd: bool) -> (Vec<(u32, u8)>, u32) {
    let mut prog: Vec<u8> = if odd { vec![0x85, 0x00] } else { Vec::new() };
    if mode != 2 {
        prog.extend(w(0x17, mode << 7));
    }
    let mut p = P::new(&prog);
    let origin = if mode == 2 {
        // No write: measured from the pin contract's h=0, which is
        // ALIGN_PHASES after the frame power_on leaves.
        for _ in 0..v2a03_sim::pins::ALIGN_PHASES {
            p.step();
        }
        0usize
    } else {
        p.until_write("w4017", 4000);
        0
    };
    let mut prev = [false; 5];
    let mut rises: Vec<(u32, u8)> = Vec::new();
    let mut a_seen = 0;
    let mut period = 0;
    for i in 0..90_000usize {
        p.step();
        for (k, name) in PHASES.iter().enumerate() {
            let v = p.hi(name);
            if v && !prev[k] {
                if k == 0 {
                    a_seen += 1;
                    if a_seen == 2 {
                        period = (i - origin) as u32 - rises[0].0;
                        return (rises, period);
                    }
                }
                rises.push(((i - origin) as u32, k as u8));
            }
            prev[k] = v;
        }
    }
    let _ = period;
    panic!("mode {mode}: phase_a did not repeat in 90,000 half-steps");
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=MUTATE");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("tables.rs");
    let mut s = String::new();
    if !v2a03_netlist::available() {
        println!("cargo:warning=v2a03-micro: extern/visual2a03 not fetched; APU tables written EMPTY, tests will SKIP");
        s.push_str("pub const AVAILABLE: bool = false;\npub static LENGTH: [u8; 32] = [0; 32];\npub static NOISE_PERIOD: [u16; 16] = [0; 16];\npub static DMC_RATE: [u16; 16] = [0; 16];\npub static DUTY: [u8; 4] = [0; 4];\npub static FRAME_4: &[(u32, u8)] = &[];\npub static FRAME_4_PERIOD: u32 = 0;\npub static FRAME_5: &[(u32, u8)] = &[];\npub static FRAME_5_PERIOD: u32 = 0;\npub static FRAME_POWER_ON: u32 = 0;\npub static FRAME_JITTER: u32 = 0;\npub static MODE1_CLOCK: u32 = 0;\npub static NOISE_TIMER: LfsrTimer = LfsrTimer { at_h0: 0, taps: (0, 0), terminal: 0, reload: [0; 16] };\npub static DMC_TIMER: LfsrTimer = LfsrTimer { at_h0: 0, taps: (0, 0), terminal: 0, reload: [0; 16] };\n");
        std::fs::write(&out, s).unwrap();
        return;
    }
    let length = length();
    let mut duty = duty();
    if std::env::var("MUTATE").is_ok_and(|v| v == "1") {
        // The proof the gate can tell: the duty table swapped, so every
        // square plays the wrong sequence and the code comparison must
        // go red. Never shipped; the env var rebuilds the table.
        duty.reverse();
        println!("cargo:warning=v2a03-micro: MUTATE=1, the duty table is REVERSED; the APU gate must go red");
    }
    let (f4, p4) = frame(0, false);
    let (f5, p5) = frame(1, false);
    let (f0, _) = frame(2, false);
    // The other parity: every rise two half-steps later, both modes (the
    // jitter blargg's apu_test 4 asks about), held here so a table that
    // stopped saying so fails the build by name.
    let (f4o, p4o) = frame(0, true);
    let (f5o, p5o) = frame(1, true);
    for (name, a, b, pa, pb) in [("mode 0", &f4, &f4o, p4, p4o), ("mode 1", &f5, &f5o, p5, p5o)] {
        assert_eq!(pa, pb, "{name}: the period differs by write parity");
        assert_eq!(a.len(), b.len(), "{name}: the phase list differs by write parity");
        for (x, y) in a.iter().zip(b) {
            assert_eq!(x.1, y.1, "{name}: the phase order differs by write parity");
            assert_eq!(y.0, x.0 + FRAME_JITTER, "{name}: phase {} rises at {} on one parity and {} on the other, not {} apart", x.1, x.0, y.0, FRAME_JITTER);
        }
    }
    let m1 = mode1_clock();
    let nt = lfsr_timer("noi_t", 11, 2100);
    let dt = lfsr_timer("pcm_t", 9, 560);
    let (dmc, dmc_reload) = dmc_rate(dt.terminal);
    let (noise, noise_reload) = noise_period(nt.terminal);
    s.push_str("pub const AVAILABLE: bool = true;\n");
    writeln!(
        s,
        "pub static NOISE_TIMER: LfsrTimer = LfsrTimer {{ at_h0: {}, taps: ({}, {}), terminal: {}, reload: {:?} }};",
        nt.at_h0, nt.taps.0, nt.taps.1, nt.terminal, noise_reload
    )
    .unwrap();
    writeln!(
        s,
        "pub static DMC_TIMER: LfsrTimer = LfsrTimer {{ at_h0: {}, taps: ({}, {}), terminal: {}, reload: {:?} }};",
        dt.at_h0, dt.taps.0, dt.taps.1, dt.terminal, dmc_reload
    )
    .unwrap();
    writeln!(s, "pub static LENGTH: [u8; 32] = {length:?};").unwrap();
    writeln!(s, "pub static NOISE_PERIOD: [u16; 16] = {noise:?};").unwrap();
    writeln!(s, "pub static DMC_RATE: [u16; 16] = {dmc:?};").unwrap();
    writeln!(s, "pub static DUTY: [u8; 4] = {duty:?};").unwrap();
    writeln!(s, "pub static FRAME_4: &[(u32, u8)] = &{f4:?};").unwrap();
    writeln!(s, "pub static FRAME_4_PERIOD: u32 = {p4};").unwrap();
    writeln!(s, "pub static FRAME_5: &[(u32, u8)] = &{f5:?};").unwrap();
    writeln!(s, "pub static FRAME_5_PERIOD: u32 = {p5};").unwrap();
    writeln!(s, "pub static FRAME_POWER_ON: u32 = {};", f0[0].0).unwrap();
    writeln!(s, "pub static FRAME_JITTER: u32 = {FRAME_JITTER};").unwrap();
    writeln!(s, "pub static MODE1_CLOCK: u32 = {m1};").unwrap();
    std::fs::write(&out, s).unwrap();
    println!(
        "cargo:warning=v2a03-micro: APU tables recorded: length {:?}, noise {:?}, dmc {:?}, duty {:?}, frame4 {:?} period {p4}, frame5 {:?} period {p5}, power-on phase_a at {}; noise timer at h0 {} taps {:?} terminal {} reload {:?}; dmc timer at h0 {} taps {:?} terminal {} reload {:?}",
        length, noise, dmc, duty, f4, f5, f0[0].0, nt.at_h0, nt.taps, nt.terminal, noise_reload, dt.at_h0, dt.taps, dt.terminal, dmc_reload
    );
}
