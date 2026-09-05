//! The rung's presented frame beside its core's own frame, per half-step,
//! over the sprite DMA program: what the core does underneath the DMA.
//!
//!   cargo run --release -p v2a03-micro --example rung-trace -- <from> <to>

use v2a03_micro::rung::Rung;
use v6502_pins::{line, Load, PinEngine};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let from: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(6);
    let to: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(24);
    let mut prog = vec![0xa9u8, 0x02, 0x8d, 0x14, 0x40, 0xea, 0xea, 0xea, 0xea];
    let spin = 0x8000 + prog.len() as u16;
    prog.extend([0x4c, spin as u8, (spin >> 8) as u8]);
    let page: Vec<u8> = (0..=255u8).map(|i| i.wrapping_mul(3)).collect();
    let loads = vec![Load { org: 0x8000, bytes: prog }, Load { org: 0x0200, bytes: page }];
    let mut rung = Rung::new(&loads, 0x8000, 0xbd);
    rung.power_cycle();
    for h in 1..=to {
        rung.half_step();
        if h >= from {
            println!("h={h:>5} rung {} | core {}", line(&rung.pins()), line(&rung.core.pins()));
        }
    }
}
