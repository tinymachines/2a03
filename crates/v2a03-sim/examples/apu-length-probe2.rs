//! Every length counter, silence gate and output beside the frame
//! strobes, on rung 0, across one half-frame clock: the reads gate and
//! the code gate had disagreed about where the triangle's and the
//! noise's lengths move, and this reads it off the die. The program
//! loads a two-half-frame length on all four channels, clocks once by
//! a mode-1 write, writes mode 0, and spins; the window is the second
//! step.
//!
//!   cargo run --release -p v2a03-sim --example apu-length-probe2

use v2a03_sim::harness::Harness;
use v2a03_sim::pins::ALIGN_PHASES;
use v2a03_sim::Cpu;

fn w(reg: u8, v: u8) -> [u8; 5] {
    [0xa9, v, 0x8d, reg, 0x40]
}

fn main() {
    let mut p: Vec<u8> = Vec::new();
    for (r, v) in [(0x17, 0x40), (0x15, 0x0f), (0x00, 0x10), (0x02, 0x40), (0x03, 0x18), (0x04, 0x10), (0x06, 0x40), (0x07, 0x18), (0x08, 0x7f), (0x0a, 0x40), (0x0b, 0x18), (0x0c, 0x10), (0x0e, 0x04), (0x0f, 0x18), (0x17, 0xc0), (0x17, 0x00)] {
        p.extend(w(r, v));
    }
    let spin = 0x8000 + p.len() as u16;
    p.extend([0x4c, spin as u8, (spin >> 8) as u8]);
    let mut h = Harness::new(Cpu::power_on());
    h.load(0x8000, &p, 0x8000);
    let nl = h.cpu.engine.netlist().clone();
    let n = |name: &str| nl.node(name).unwrap_or_else(|| panic!("node {name}"));
    let group = |prefix: &str, count: usize| -> Vec<halfphi::NodeId> { (0..count).map(|i| n(&format!("{prefix}{i}"))).collect() };
    let buses: Vec<(&str, Vec<halfphi::NodeId>)> = vec![
        ("sq0_len", group("sq0_len", 8)), ("sq1_len", group("sq1_len", 8)), ("tri_len", group("tri_len", 8)), ("noi_len", group("noi_len", 8)),
        ("sq0_out", group("sq0_out", 4)), ("sq1_out", group("sq1_out", 4)), ("tri_out", group("tri_out", 4)), ("noi_out", group("noi_out", 4)),
    ];
    let singles: Vec<(&str, halfphi::NodeId)> = vec![
        ("half(low)", n("frm_/half")), ("phase_b", n("frm_phase_b")), ("sq0_silence", n("sq0_silence")), ("sq1_silence", n("sq1_silence")), ("tri_silence", n("tri_silence")), ("noi_silence", n("noi_silence")), ("w4017", n("w4017")),
    ];
    let bits = |h: &Harness, ns: &[halfphi::NodeId]| -> u32 { ns.iter().enumerate().map(|(i, &nd)| (h.cpu.engine.is_high(nd) as u32) << i).sum() };
    let mut prev_b: Vec<u32> = buses.iter().map(|(_, b)| bits(&h, b)).collect();
    let mut prev_s: Vec<bool> = singles.iter().map(|(_, s)| h.cpu.engine.is_high(*s)).collect();
    for step in 0..30_400usize {
        h.half_step();
        let hh = step as i64 + 1 - ALIGN_PHASES as i64;
        let mut line = String::new();
        for (i, (name, b)) in buses.iter().enumerate() {
            let v = bits(&h, b);
            if v != prev_b[i] && (!name.ends_with("out") || hh > 29_900) {
                line.push_str(&format!(" {name} {}->{}", prev_b[i], v));
                prev_b[i] = v;
            } else if v != prev_b[i] {
                prev_b[i] = v;
            }
        }
        for (i, (name, s)) in singles.iter().enumerate() {
            let v = h.cpu.engine.is_high(*s);
            if v != prev_s[i] {
                line.push_str(&format!(" {name}->{}", v as u8));
                prev_s[i] = v;
            }
        }
        if !line.is_empty() {
            println!("h={hh}:{line}");
        }
    }
}
