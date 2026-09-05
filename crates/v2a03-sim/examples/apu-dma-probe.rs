//! N3 step 3, probe 3: the sprite DMA ($4014) as the core sees it. A
//! program writes $4014 = $02 and the probe prints every CPU half-step
//! from the write until the core runs again: `spr_dma_/rdy`, the
//! internal `rdy`, `RnWstretched`, the address and data buses, R/W and
//! the DMA's own counters (`spr_a*`, `spr_addr*`), so the stall's
//! length, its alignment rule and the read/write pairing are numbers
//! read off the chip. Runs the write at two CPU-cycle parities (a
//! 3-cycle `LDA $00` inserted before it) because the documented stall is
//! 513 or 514 cycles depending on which half of the APU's two-cycle
//! period the write lands in.
//!
//!   cargo run --release -p v2a03-sim --example apu-dma-probe -- [verbose]

use v2a03_sim::harness::Harness;
use v2a03_sim::Cpu;

fn run(shift: bool, verbose: bool) {
    // [LDA $00] ; LDA #$02 ; STA $4014 ; NOP x4 ; JMP spin
    let mut prog = Vec::new();
    if shift {
        prog.extend([0xa5u8, 0x00]);
    }
    prog.extend([0xa9u8, 0x02, 0x8d, 0x14, 0x40, 0xea, 0xea, 0xea, 0xea]);
    let spin = 0x8000 + prog.len() as u16;
    prog.extend([0x4c, spin as u8, (spin >> 8) as u8]);
    let mut h = Harness::new(Cpu::power_on());
    h.load(0x8000, &prog, 0x8000);
    for i in 0..256usize {
        h.memory[0x0200 + i] = (i as u8).wrapping_mul(3);
    }
    let nl = h.cpu.engine.netlist().clone();
    let n = |name: &str| nl.node(name).unwrap_or_else(|| panic!("node {name}"));
    let (w4014, dma_rdy, rdy, stretched, rw, clk0) = (n("w4014"), n("spr_dma_/rdy"), n("rdy"), n("RnWstretched"), n("rw"), n("clk0"));
    let ab: Vec<_> = (0..16).map(|i| n(&format!("ab{i}"))).collect();
    let db: Vec<_> = (0..8).map(|i| n(&format!("db{i}"))).collect();
    let spr_a: Vec<_> = (0..16).map(|i| n(&format!("spr_a{i}"))).collect();
    let spr_addr: Vec<_> = (0..16).map(|i| n(&format!("spr_addr{i}"))).collect();
    let apu_clk1 = n("apu_clk1");
    let bits = |h: &Harness, ns: &[halfphi::NodeId]| -> u32 { ns.iter().enumerate().map(|(i, &nd)| (h.cpu.engine.is_high(nd) as u32) << i).sum() };

    let mut write_at = None;
    let mut prev_w = false;
    let mut rdy_low_from = None;
    let mut rdy_low_to = None;
    let (mut reads, mut writes_2004) = (0, 0);
    let mut last_rdy = true;
    for step in 0..2400usize {
        h.half_step();
        let w = h.cpu.engine.is_high(w4014);
        if w && !prev_w {
            write_at = Some(step);
            println!("$4014 write strobe at half-step {step} (shift={shift}), apu_clk1={} clk0={}", h.cpu.engine.is_high(apu_clk1) as u8, h.cpu.engine.is_high(clk0) as u8);
        }
        prev_w = w;
        let r = h.cpu.engine.is_high(rdy);
        if !r && last_rdy {
            rdy_low_from = Some(step);
        }
        if r && !last_rdy {
            rdy_low_to = Some(step);
        }
        last_rdy = r;
        if let Some(w0) = write_at {
            let a = bits(&h, &ab) as u16;
            let rw_v = h.cpu.engine.is_high(rw);
            let c = h.cpu.engine.is_high(clk0);
            if !rdy_low_to.is_some_and(|t| step > t + 8) {
                if c && !rw_v && a == 0x2004 {
                    writes_2004 += 1;
                }
                if !c && rw_v && (0x0200..0x0300).contains(&a) && !r {
                    reads += 1;
                }
                if verbose || step < w0 + 16 || rdy_low_to.is_some_and(|t| step <= t + 8) {
                    println!(
                        "  h+{:>4} clk0={} ab={a:04x} db={:02x} rw={} rdy={} spr_dma_/rdy={} RnWstretched={} spr_a={:04x} spr_addr={:04x}",
                        step - w0,
                        c as u8,
                        bits(&h, &db),
                        rw_v as u8,
                        r as u8,
                        h.cpu.engine.is_high(dma_rdy) as u8,
                        h.cpu.engine.is_high(stretched) as u8,
                        bits(&h, &spr_a),
                        bits(&h, &spr_addr)
                    );
                }
            }
            if rdy_low_to.is_some_and(|t| step > t + 8) {
                break;
            }
        }
    }
    let (w0, f, t) = (write_at.unwrap(), rdy_low_from.unwrap(), rdy_low_to.unwrap());
    println!(
        "shift={shift}: rdy low from h+{} to h+{} ({} half-steps = {} CPU cycles); {reads} reads in $02xx while rdy low, {writes_2004} writes to $2004",
        f - w0,
        t - w0,
        t - f,
        (t - f) as f64 / 2.0
    );
}

fn main() {
    let verbose = std::env::args().nth(1).is_some();
    run(false, verbose);
    run(true, verbose);
}
