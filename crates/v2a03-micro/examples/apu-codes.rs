//! The authored APU's five output codes per half-step over the gate's
//! long-note world (the same bytes rung 0 produced, the gate having held
//! them identical), as CSV for figures.
//!
//!   cargo run --release -p v2a03-micro --example apu-codes -- [half_steps] > codes.csv

use v2a03_micro::apu::Apu;
use v6502_pins::{Load, PinEngine};

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(80_000);
    let mut prog = Vec::new();
    for (r, v) in [(0x17u8, 0x00u8), (0x15, 0x0f), (0x00, 0xa6), (0x01, 0xa9), (0x02, 0xab), (0x03, 0x09), (0x04, 0x7f), (0x05, 0x91), (0x06, 0x00), (0x07, 0x3a), (0x08, 0xc0), (0x0a, 0x50), (0x0b, 0x48), (0x0c, 0x04), (0x0e, 0x04), (0x0f, 0x10), (0x10, 0x4f), (0x11, 0x20), (0x12, 0x00), (0x13, 0x02), (0x15, 0x1f)] {
        prog.extend([0xa9, v, 0x8d, r, 0x40]);
    }
    let spin = 0x8000 + prog.len() as u16;
    prog.extend([0x4c, spin as u8, (spin >> 8) as u8]);
    let sample: Vec<u8> = (0..33u8).map(|i| i.wrapping_mul(0x5b) ^ 0xa5).collect();
    let loads = vec![Load { org: 0x8000, bytes: prog }, Load { org: 0xc000, bytes: sample }];
    let mut core = v2a03_micro::core(&loads, 0x8000, 0xbd);
    let mut apu = Apu::new();
    println!("h,sq0,sq1,tri,noi,pcm,frame_irq");
    for h in 1..=n {
        core.half_step();
        let f = core.pins();
        if !f.rw && f.clk0 && (0x4000..=0x4017).contains(&f.ab) {
            apu.write((f.ab & 0x1f) as u8, f.db);
        }
        apu.half_step(&core.mem);
        let c = apu.codes();
        println!("{h},{},{},{},{},{},{}", c[0], c[1], c[2], c[3], c[4], apu.frame_irq as u8);
    }
}
