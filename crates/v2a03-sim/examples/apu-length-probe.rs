//! N3 step 3, probe 2: the length table, read off rung 0. A program
//! enables square 0 (constant volume, halt clear) and writes $4003 with
//! each of the 32 length indices in turn; a few half-steps after each
//! `w4003` strobe the probe reads the loaded counter off `sq0_len0..7`
//! and prints the table. The frame sequencer's first half-frame clock
//! is 7,458 cycles after power-on's $4017 state, far past this program's
//! 32 writes, so nothing decrements a value before it is read; the probe
//! prints the half-step of each read so that can be checked against the
//! frame probe's numbers rather than assumed.
//!
//!   cargo run --release -p v2a03-sim --example apu-length-probe

use v2a03_sim::harness::Harness;
use v2a03_sim::Cpu;

fn main() {
    // LDA #$01 STA $4015 ; LDA #$30 STA $4000 ; 32 x (LDA #i<<3 ; STA $4003) ; JMP spin
    let mut prog = vec![0xa9u8, 0x01, 0x8d, 0x15, 0x40, 0xa9, 0x30, 0x8d, 0x00, 0x40];
    for i in 0..32u8 {
        prog.extend([0xa9, i << 3, 0x8d, 0x03, 0x40]);
    }
    let spin = 0x8000 + prog.len() as u16;
    prog.extend([0x4c, spin as u8, (spin >> 8) as u8]);
    let mut h = Harness::new(Cpu::power_on());
    h.load(0x8000, &prog, 0x8000);
    let nl = h.cpu.engine.netlist().clone();
    let n = |name: &str| nl.node(name).unwrap_or_else(|| panic!("node {name}"));
    let len: Vec<_> = (0..8).map(|i| n(&format!("sq0_len{i}"))).collect();
    let (w4003, reload, on) = (n("w4003"), n("sq0_len_reload"), n("sq0_on"));
    let db: Vec<_> = (0..8).map(|i| n(&format!("db{i}"))).collect();
    let bits = |h: &Harness, ns: &[halfphi::NodeId]| -> u32 { ns.iter().enumerate().map(|(i, &nd)| (h.cpu.engine.is_high(nd) as u32) << i).sum() };

    let mut table = Vec::new();
    let mut prev_w = false;
    let mut pending: Option<(u8, usize)> = None;
    println!("index  written  sq0_len (loaded)  read at half-step  reload/on at the strobe");
    for step in 0..6000usize {
        h.half_step();
        let w = h.cpu.engine.is_high(w4003);
        if w && !prev_w {
            let byte = bits(&h, &db) as u8;
            println!("  strobe: $4003 <- {byte:02x} at half-step {step}, sq0_len_reload={} sq0_on={}", h.cpu.engine.is_high(reload) as u8, h.cpu.engine.is_high(on) as u8);
            pending = Some((byte >> 3, step));
        }
        prev_w = w;
        if let Some((idx, at)) = pending {
            // Read once the strobe has had six half-steps to land.
            if step == at + 6 {
                let v = bits(&h, &len) as u8;
                println!("  {idx:>3}    {:02x}       {v:>3} (${v:02x})          {step}", idx << 3);
                table.push(v);
                pending = None;
            }
        }
        if table.len() == 32 {
            break;
        }
    }
    println!("length table by index (32 entries): {table:?}");
}
