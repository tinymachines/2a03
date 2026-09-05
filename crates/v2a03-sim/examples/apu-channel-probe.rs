//! N3 step 3, probes 4 to 9: the channels, measured off rung 0, each a
//! small register program and a sampler on the channel's own nodes.
//! Every number printed here is a measurement step 4 is authored from;
//! nothing is compared against a document.
//!
//!   duty    the four square duty sequences (`sq0_c` step, `sq0_out`)
//!   env     the envelope: decay level per quarter frame, period 0
//!   sweep   the sweep on both squares, positive and negative (the two
//!           squares' negate arithmetic differ), the period per half frame
//!   tri     the triangle's 32-step sequence and its step period
//!   noise   the 16-entry noise period table (half-steps between LFSR
//!           shifts) and the feedback tap in both modes
//!   dmc     the 16-entry DMC rate table (half-steps between shifts), the
//!           sample fetch stall (`pcm_dma_/rdy`) and its address
//!   io      $4016 writes onto OUT0..2, $4016/$4017 reads onto the
//!           controller strobes
//!
//!   cargo run --release -p v2a03-sim --example apu-channel-probe -- [scenario...]

use halfphi::NodeId;
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
    fn node(&self, name: &str) -> NodeId {
        self.h.cpu.engine.netlist().node(name).unwrap_or_else(|| panic!("node {name}"))
    }
    fn hi(&self, name: &str) -> bool {
        self.h.cpu.engine.is_high(self.node(name))
    }
    fn bits(&self, prefix: &str, n: usize) -> u32 {
        (0..n).map(|i| (self.hi(&format!("{prefix}{i}")) as u32) << i).sum()
    }
    fn step(&mut self) {
        self.h.half_step();
    }
    /// Run until the named write strobe rises; returns the half-step.
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

/// LDA #v ; STA $40xx
fn w(reg: u8, v: u8) -> [u8; 5] {
    [0xa9, v, 0x8d, reg, 0x40]
}

fn duty() {
    println!("## duty: sq0_out per sequencer step (sq0_c), timer 8, constant volume 15");
    for d in 0..4u8 {
        let mut prog = Vec::new();
        prog.extend(w(0x15, 0x01));
        prog.extend(w(0x00, (d << 6) | 0x3f));
        prog.extend(w(0x02, 0x08));
        prog.extend(w(0x03, 0x00));
        let mut p = P::new(&prog);
        p.until_write("w4003", 4000);
        let mut pattern = Vec::new();
        let mut prev_c = p.bits("sq0_c", 3);
        let mut last_change = 0usize;
        let mut periods = Vec::new();
        for i in 0..1200 {
            p.step();
            let c = p.bits("sq0_c", 3);
            if c != prev_c {
                pattern.push((c, p.bits("sq0_out", 4)));
                periods.push(i - last_change);
                last_change = i;
                prev_c = c;
            }
            if pattern.len() == 10 {
                break;
            }
        }
        let outs: Vec<String> = pattern.iter().map(|(c, o)| format!("c{c}:{o}")).collect();
        println!("  duty {d}: {} ; step period {:?} half-steps", outs.join(" "), &periods[1..]);
    }
}

fn env() {
    println!("## envelope: sq0 envelope mode, period 0, no loop; decay level (sq0_envc) on each change, in CPU cycles after the $4003 write");
    let mut prog = Vec::new();
    prog.extend(w(0x15, 0x01));
    prog.extend(w(0x00, 0x00)); // duty 0, halt 0, envelope, period 0
    prog.extend(w(0x02, 0x08));
    prog.extend(w(0x03, 0x00));
    let mut p = P::new(&prog);
    let w0 = p.until_write("w4003", 4000);
    let mut prev = p.bits("sq0_envc", 4);
    let mut prev_t = p.bits("sq0_envt", 4);
    println!("  at the write: envc={prev} envt={prev_t} envp={} envmode={}", p.bits("sq0_envp", 4), p.hi("sq0_envmode") as u8);
    let mut n = 0;
    for i in 0..300_000usize {
        p.step();
        let v = p.bits("sq0_envc", 4);
        let t = p.bits("sq0_envt", 4);
        if v != prev || t != prev_t {
            println!("  +{:.1} cycles: envc {prev} -> {v}, envt {prev_t} -> {t}, out={}", (i as f64 - w0 as f64) / 2.0 + 0.5, p.bits("sq0_out", 4));
            prev = v;
            prev_t = t;
            n += 1;
            if n >= 20 {
                break;
            }
        }
    }
}

fn sweep() {
    println!("## sweep: timer $100, shift 1, period 0; sq0_p / sq1_p on each change, CPU cycles after the $4003/$4007 write");
    for (neg, reg_base, name) in [(false, 0x00u8, "sq0"), (true, 0x00, "sq0"), (false, 0x04, "sq1"), (true, 0x04, "sq1")] {
        let mut prog = Vec::new();
        prog.extend(w(0x15, 0x03));
        prog.extend(w(reg_base, 0x3f));
        prog.extend(w(reg_base + 1, 0x81 | if neg { 0x08 } else { 0 }));
        prog.extend(w(reg_base + 2, 0x00));
        prog.extend(w(reg_base + 3, 0x01));
        let mut p = P::new(&prog);
        let strobe = format!("w400{}", reg_base + 3);
        let w0 = p.until_write(&strobe, 4000);
        let pre = format!("{name}_p");
        let mut prev = p.bits(&pre, 11);
        let mut changes = Vec::new();
        for i in 0..70_000usize {
            p.step();
            let v = p.bits(&pre, 11);
            if v != prev {
                changes.push(format!("+{:.1}: ${prev:03x} -> ${v:03x}", (i as f64 - w0 as f64) / 2.0 + 0.5));
                prev = v;
                if changes.len() == 3 {
                    break;
                }
            }
        }
        println!("  {name} negate={}: {}", neg as u8, changes.join(", "));
    }
}

fn tri() {
    println!("## triangle: linear reload 127 with control set, timer 8; tri_out on each change and the step period");
    let mut prog = Vec::new();
    prog.extend(w(0x15, 0x04));
    prog.extend(w(0x08, 0xff));
    prog.extend(w(0x0a, 0x08));
    prog.extend(w(0x0b, 0x00));
    let mut p = P::new(&prog);
    let mut prev = p.bits("tri_out", 4);
    let mut seq = vec![prev];
    let mut last = 0usize;
    let mut periods = Vec::new();
    let w0 = p.until_write("w400b", 4000);
    let mut first_step = None;
    for i in 0..60_000usize {
        p.step();
        let v = p.bits("tri_out", 4);
        if v != prev {
            if first_step.is_none() {
                first_step = Some(i);
                println!("  first step +{:.1} CPU cycles after the $400B write (the linear counter loads at the first quarter-frame clock); tri_lin={} tri_lc={}", (i as f64 - w0 as f64) / 2.0, p.bits("tri_lin", 7), p.bits("tri_lc", 7));
            }
            seq.push(v);
            periods.push(i - last);
            last = i;
            prev = v;
            if seq.len() == 40 {
                break;
            }
        }
    }
    println!("  sequence: {seq:?}");
    if periods.len() > 1 {
        println!("  step periods (half-steps): {:?}", &periods[1..]);
    }
    println!("  tri_lin={} tri_lc={} tri_c={}", p.bits("tri_lin", 7), p.bits("tri_lc", 7), p.bits("tri_c", 5));
}

fn noise() {
    println!("## noise: period table as half-steps between LFSR shifts, per $400E index; then the tap in each mode");
    let mut table = Vec::new();
    for f in 0..16u8 {
        let mut prog = Vec::new();
        prog.extend(w(0x15, 0x08));
        prog.extend(w(0x0c, 0x3f));
        prog.extend(w(0x0e, f));
        prog.extend(w(0x0f, 0x00));
        let mut p = P::new(&prog);
        p.until_write("w400f", 4000);
        let t_at_write = p.bits("noi_t", 11);
        let mut t_max = 0;
        let mut prev = p.bits("noi_c", 15);
        let mut last = None;
        let mut gaps = Vec::new();
        for i in 0..40_000usize {
            p.step();
            if last.is_some() {
                t_max = t_max.max(p.bits("noi_t", 11));
            }
            let v = p.bits("noi_c", 15);
            if v != prev {
                if let Some(l) = last {
                    gaps.push(i - l);
                }
                last = Some(i);
                prev = v;
                if gaps.len() == 3 {
                    break;
                }
            }
        }
        println!(
            "  index {f:>2}: shifts {gaps:?} half-steps apart ({} CPU cycles); noi_t at the write {t_at_write}, highest noi_t between shifts {t_max} (${t_max:03x})",
            gaps.first().map(|g| g / 2).unwrap_or(0)
        );
        table.push(gaps.first().copied().unwrap_or(0) / 2);
    }
    println!("  noise period table (CPU cycles): {table:?}");
    for mode in [0u8, 1] {
        let mut prog = Vec::new();
        prog.extend(w(0x15, 0x08));
        prog.extend(w(0x0c, 0x3f));
        prog.extend(w(0x0e, (mode << 7) | 0x00));
        prog.extend(w(0x0f, 0x00));
        let mut p = P::new(&prog);
        let w0 = p.until_write("w400f", 4000);
        let mut prev = p.bits("noi_c", 15);
        let mut states = vec![prev];
        let mut first = None;
        for i in 0..40_000usize {
            p.step();
            let v = p.bits("noi_c", 15);
            if v != prev {
                first.get_or_insert(i);
                states.push(v);
                prev = v;
                if states.len() == 24 {
                    break;
                }
            }
        }
        // Printed with noi_c0 on the LEFT: the register's own numbering.
        let s: Vec<String> = states.iter().map(|v| format!("{:015b}", v.reverse_bits() >> 17)).collect();
        println!("  mode {mode} (noi_lfsrmode={}): first shift +{:?} half-steps after the $400F write; LFSR states, noi_c0 leftmost:\n    {}", p.hi("noi_lfsrmode") as u8, first.map(|f| f - w0), s.join("\n    "));
    }
}

fn dmc() {
    println!("## dmc: rate table as half-steps between shifts (pcm_bits), per $4010 index; the fetch stall; the output");
    let mut table = Vec::new();
    for r in 0..16u8 {
        let mut prog = Vec::new();
        prog.extend(w(0x15, 0x00));
        prog.extend(w(0x10, r));
        prog.extend(w(0x11, 0x40));
        prog.extend(w(0x12, 0x00)); // sample at $C000
        prog.extend(w(0x13, 0x01)); // 17 bytes
        prog.extend(w(0x15, 0x10));
        let mut p = P::new(&prog);
        for i in 0..17usize {
            p.h.memory[0xc000 + i] = 0b1010_0101;
        }
        let w0 = p.until_write("w4015", 6000);
        // first the fetch: rdy low span and the address read
        let mut stalls: Vec<(usize, usize, u16)> = Vec::new();
        let mut in_stall = false;
        let mut prev_bits = p.bits("pcm_bits", 3);
        let mut shifts = Vec::new();
        let mut last_shift = None;
        let mut outs = Vec::new();
        let mut prev_out = p.bits("pcm_out", 7);
        for i in 0..14_000usize {
            p.step();
            let rdy = p.hi("rdy");
            if !rdy {
                let a = p.bits("ab", 16) as u16;
                if !in_stall {
                    stalls.push((i, 1, a));
                    in_stall = true;
                } else if let Some(last) = stalls.last_mut() {
                    last.1 += 1;
                    if (0xc000..0xc011).contains(&a) {
                        last.2 = a;
                    }
                }
            } else {
                in_stall = false;
            }
            let b = p.bits("pcm_bits", 3);
            if b != prev_bits {
                if let Some(l) = last_shift {
                    shifts.push(i - l);
                }
                last_shift = Some(i);
                prev_bits = b;
            }
            let o = p.bits("pcm_out", 7);
            if o != prev_out {
                outs.push(o);
                prev_out = o;
            }
            if shifts.len() >= 3 && outs.len() >= 4 {
                break;
            }
        }
        let stall: Vec<String> = stalls.iter().take(3).map(|(f, n, a)| format!("h+{} for {n} half-steps reading ${a:04x}", f - w0)).collect();
        let stall = format!("stalls: {}", stall.join("; "));
        println!("  rate {r:>2}: shifts {shifts:?} half-steps apart ({} cycles); {stall}; pcm_out {outs:?}", shifts.first().map(|s| s / 2).unwrap_or(0));
        table.push(shifts.first().copied().unwrap_or(0) / 2);
    }
    println!("  dmc rate table (CPU cycles): {table:?}");
}

fn io() {
    println!("## io: $4016 <- 1, <- 0, then LDA $4016, LDA $4017; OUT0..2 and the controller strobes per half-step");
    let mut prog = Vec::new();
    prog.extend(w(0x16, 0x01));
    prog.extend(w(0x16, 0x00));
    prog.extend(w(0x16, 0x07));
    prog.extend([0xad, 0x16, 0x40, 0xad, 0x17, 0x40]);
    let mut p = P::new(&prog);
    let mut prev = (false, false, false, false, false, false, false);
    for i in 0..140usize {
        p.step();
        let now = (p.hi("out0"), p.hi("out1"), p.hi("out2"), p.hi("joy1"), p.hi("joy2"), p.hi("/r4016"), p.hi("/r4017"));
        if now != prev {
            println!(
                "  h={i} ab={:04x} rw={} db={:02x}: out0={} out1={} out2={} joy1={} joy2={} /r4016={} /r4017={}",
                p.bits("ab", 16),
                p.hi("rw") as u8,
                p.bits("db", 8),
                now.0 as u8, now.1 as u8, now.2 as u8, now.3 as u8, now.4 as u8, now.5 as u8, now.6 as u8
            );
            prev = now;
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let all = args.is_empty();
    let want = |s: &str| all || args.iter().any(|a| a == s);
    if want("duty") { duty(); }
    if want("env") { env(); }
    if want("sweep") { sweep(); }
    if want("tri") { tri(); }
    if want("noise") { noise(); }
    if want("dmc") { dmc(); }
    if want("io") { io(); }
}
