//! The whole chip at the pins: the core rung, the APU fed from the
//! core's writes, and the DMA units that take the bus from it. This is
//! what a console attaches to, and what N3 step 5's gate holds to rung 0
//! frame for frame: while a DMA runs, the frame is the DMA's (its address,
//! its byte, its R/W, RDY low) and the core stands in its halted read
//! cycle underneath, exactly as the switch-level chip does.
//!
//! The sprite DMA, measured (`v2a03-sim/examples/stall-probe.rs`): RDY
//! falls on the $4014 write's own frame; the core repeats its next read
//! cycle (an opcode fetch, sync high) until the first "get" frame at
//! least three frames on (h = 0 mod 4), then 256 pairs follow, a read of
//! the source page on a get cycle and a write of that byte to $2004 on
//! the put cycle after it, 1,024 frames; RDY returns and the fetch
//! completes. 1,027 or 1,029 half-steps by the write's alignment.
//!
//! The DMC fetch, measured the same way: RDY falls one frame after the
//! byte-boundary completion that empties the buffer (six after the
//! enable write's frame), the core repeats its next read cycle, the
//! sample byte is read on the first get frame at least three on, RDY
//! returns two frames after that read begins and the held cycle runs
//! for real. Five or seven half-steps by alignment.
//!
//! RDY reaches rung 3 through `set_inputs`. Both units assert it as soon
//! as the phi1 of the frame that decides them shows, so the core holds
//! the read cycle after it. On release the measured die runs the held
//! read once more with RDY already high (the bus came back with RDY; a
//! bare 6502 released on a phi1 frame goes straight to the next cycle,
//! `fixture-rdy-release-phi1` in the 6502's golden), so the pin shows
//! RDY high for that cycle while the core's release is fed one cycle
//! later than the pin's rise.

use v6502_micro::machine::MicroCpu;
use v6502_pins::{Load, PinEngine, PinFrame};

use crate::apu::Apu;

/// A sprite DMA in flight.
#[derive(Clone, Copy, Debug)]
struct SpriteDma {
    page: u8,
    /// The frame the $4014 write landed on.
    strobe: u64,
    /// The first get frame, once chosen.
    start: Option<u64>,
    /// The byte read on the current pair, for its write.
    byte: u8,
    /// The frame of the last write, once done: RDY shows high from the
    /// next frame and the core is released two frames on.
    done: Option<u64>,
}

/// A DMC sample fetch in flight.
#[derive(Clone, Copy, Debug)]
struct DmcFetch {
    addr: u16,
    /// The frame RDY falls on.
    request: u64,
    /// The get frame the read lands on, once chosen.
    read: Option<u64>,
    /// The frame after the read's pair: RDY high from here, the core
    /// released two frames on.
    done: Option<u64>,
}

pub struct Rung {
    pub core: MicroCpu,
    pub apu: Apu,
    /// Test-only: the sprite DMA moves this many pairs, 256 on the die.
    /// The stall gate's MUTATE=1 sets 255 and must go red at the last
    /// pair; setting it anywhere else is a bug by name.
    pub dma_pairs: usize,
    h: u64,
    dma: Option<SpriteDma>,
    fetch: Option<DmcFetch>,
    /// The frame presented this half-step.
    frame: PinFrame,
    loads: Vec<Load>,
    reset_vector: u16,
    stack_at_h0: u8,
}

impl Rung {
    pub fn new(loads: &[Load], reset_vector: u16, stack_at_h0: u8) -> Rung {
        let core = crate::core(loads, reset_vector, stack_at_h0);
        let frame = core.pins();
        Rung { core, apu: Apu::new(), dma_pairs: 256, h: 0, dma: None, fetch: None, frame, loads: loads.to_vec(), reset_vector, stack_at_h0 }
    }

    /// The four-frame grain the DMA units live on: get cycles begin on
    /// frames with h = 0 mod 4 (measured: every DMA read lands there).
    fn is_get(h: u64) -> bool {
        h % 4 == 0
    }
}

impl PinEngine for Rung {
    fn power_cycle(&mut self) {
        self.core = crate::core(&self.loads, self.reset_vector, self.stack_at_h0);
        self.apu = Apu::new();
        self.h = 0;
        self.dma = None;
        self.fetch = None;
        self.frame = self.core.pins();
    }

    fn set_inputs(&mut self, res: bool, irq: bool, nmi: bool, _rdy: bool, _so: bool) {
        // RDY and SO are not this chip's pins; the DMA units own RDY.
        let held = self.dma.is_some() || self.fetch.is_some();
        self.core.set_inputs(res, irq, nmi, !held, false);
    }

    fn half_step(&mut self) {
        self.core.half_step();
        self.h += 1;
        let h = self.h;
        let mut f = self.core.pins();
        f.h = h;
        // A $4014 write shows its address and R/W on its phi1 frame: RDY
        // falls here, so the core samples it low at the phi2 and holds
        // the next cycle, as rung 0 does.
        if f.ab == 0x4014 && !f.rw && !f.clk0 && self.dma.is_none() {
            self.dma = Some(SpriteDma { page: 0, strobe: h + 1, start: None, byte: 0, done: None });
            self.core.set_inputs(f.res, f.irq, f.nmi, false, false);
        }
        // The core's writes reach the APU on their phi2 frame.
        if !f.rw && f.clk0 && (0x4000..=0x4017).contains(&f.ab) {
            if f.ab == 0x4014 {
                if let Some(d) = self.dma.as_mut() {
                    d.page = f.db;
                }
            } else {
                self.apu.write((f.ab & 0x1f) as u8, f.db);
            }
        }
        self.apu.half_step(&self.core.mem);
        // A DMC fetch the APU performed this frame becomes a stall: RDY
        // falls one frame on (six from the enable write's frame).
        if let Some((addr, from_enable)) = self.apu.dmc.fetched.take() {
            let request = h + if from_enable { 6 } else { 1 };
            self.fetch = Some(DmcFetch { addr, request, read: None, done: None });
        }
        if let Some(fe) = self.fetch {
            if h + 1 == fe.request {
                self.core.set_inputs(f.res, f.irq, f.nmi, false, false);
            }
            if h >= fe.request {
                let mut fe = fe;
                if fe.read.is_none() && h >= fe.request + 3 && Rung::is_get(h) {
                    fe.read = Some(h);
                }
                match (fe.read, fe.done) {
                    (Some(r), None) => {
                        f.rdy = false;
                        f.ab = fe.addr;
                        f.db = self.core.mem[fe.addr as usize];
                        f.rw = true;
                        if h == r + 1 {
                            fe.done = Some(h);
                        }
                        self.fetch = Some(fe);
                    }
                    (Some(_), Some(d)) => {
                        // The held read runs for real with RDY high; the
                        // core's release is fed at the end of its second
                        // frame.
                        f.rdy = true;
                        if h == d + 2 {
                            self.core.set_inputs(f.res, f.irq, f.nmi, true, false);
                            self.fetch = None;
                        } else {
                            self.fetch = Some(fe);
                        }
                    }
                    _ => {
                        f.rdy = false;
                        self.fetch = Some(fe);
                    }
                }
            }
        }
        if let Some(mut d) = self.dma {
            if let Some(done) = d.done {
                // The held fetch runs for real with RDY high; the core is
                // released at the end of its second frame.
                f.rdy = true;
                if h == done + 2 {
                    self.core.set_inputs(f.res, f.irq, f.nmi, true, false);
                    self.dma = None;
                }
                self.frame = f;
                return;
            }
            if h >= d.strobe {
                f.rdy = false;
            }
            if d.start.is_none() && h >= d.strobe + 3 && Rung::is_get(h) {
                d.start = Some(h);
            }
            if let Some(s) = d.start {
                let i = h - s;
                let pair = (i / 4) as usize;
                if pair < self.dma_pairs {
                    let src = ((d.page as u16) << 8) | pair as u16;
                    match i % 4 {
                        0 => {
                            d.byte = self.core.mem[src as usize];
                            f.ab = src;
                            f.db = d.byte;
                            f.rw = true;
                        }
                        1 => {
                            f.ab = src;
                            f.db = d.byte;
                            f.rw = true;
                        }
                        _ => {
                            f.ab = 0x2004;
                            f.db = d.byte;
                            f.rw = false;
                        }
                    }
                    if i % 4 == 3 && pair + 1 == self.dma_pairs {
                        d.done = Some(h);
                    }
                    self.dma = Some(d);
                }
            } else {
                self.dma = Some(d);
            }
        }
        self.frame = f;
    }

    fn pins(&self) -> PinFrame {
        self.frame
    }

    fn h(&self) -> u64 {
        self.h
    }
}
