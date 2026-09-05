//! The authored APU's state per half-step over a register program, for
//! reading beside `apu-channel-probe seq` when the gate diverges.
//!
//!   cargo run --release -p v2a03-micro --example apu-trace -- <from> <to>

use v2a03_micro::apu::Apu;
use v6502_pins::{Load, PinEngine};

fn w(reg: u8, v: u8) -> [u8; 5] {
    [0xa9, v, 0x8d, reg, 0x40]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let from: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(100);
    let to: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(140);
    let mut p = Vec::new();
    p.extend(w(0x17, 0x00));
    p.extend(w(0x15, 0x0f));
    p.extend(w(0x00, 0xa6));
    p.extend(w(0x01, 0xa9));
    p.extend(w(0x02, 0xab));
    p.extend(w(0x03, 0x09));
    p.extend(w(0x04, 0x7f));
    p.extend(w(0x05, 0x91));
    p.extend(w(0x06, 0x00));
    p.extend(w(0x07, 0x3a));
    let spin = 0x8000 + p.len() as u16;
    p.extend([0x4c, spin as u8, (spin >> 8) as u8]);
    let loads = vec![Load { org: 0x8000, bytes: p }];
    let mut core = v2a03_micro::core(&loads, 0x8000, 0xbd);
    let mut apu = Apu::new();
    for h in 0..to {
        core.half_step();
        let f = core.pins();
        let wrote = !f.rw && f.clk0 && (0x4000..=0x4017).contains(&f.ab);
        if wrote {
            apu.write((f.ab & 0x1f) as u8, f.db);
        }
        apu.half_step(&mut |a| core.mem[a as usize]);
        if h + 1 >= from {
            println!("h={:>4} clk0={} ab={:04x} db={:02x} rw={}{} codes={:?} sq1={:?}", h + 1, f.clk0 as u8, f.ab, f.db, f.rw as u8, if wrote { " WRITE" } else { "" }, apu.codes(), apu.sq[1]);
        }
    }
}
