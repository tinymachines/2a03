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

use std::cell::RefCell;
use std::rc::Rc;

use v6502_micro::machine::{MicroBus, MicroCpu};
use v6502_pins::{Load, PinEngine, PinFrame};

use crate::apu::Apu;

/// The core's bus as this chip presents it to the world: $4015 answered
/// by the APU (its status), everything else the console's. APU register
/// writes reach the APU from the core's own frames (the fitted timing),
/// so writes pass straight out and the board ignores the ones that are
/// this chip's.
struct RungBus {
    outer: Rc<RefCell<Box<dyn MicroBus>>>,
    apu: Rc<RefCell<Apu>>,
}

impl MicroBus for RungBus {
    fn read(&mut self, a: u16) -> u8 {
        // $4015 is answered inside the chip, so the external data bus
        // shows whatever the world drives there (the memory harness's
        // byte in the gates, open bus on a console); the byte the core
        // latches comes through `read_late`.
        self.outer.borrow_mut().read(a)
    }
    fn write(&mut self, a: u16, v: u8) {
        self.outer.borrow_mut().write(a, v);
    }
    fn peek(&mut self, a: u16) -> u8 {
        self.outer.borrow_mut().peek(a)
    }
    fn read_late(&mut self, a: u16) -> Option<u8> {
        if a == 0x4015 {
            // The byte as the core latches it at the end of phi2: the
            // APU one half-step on (`Apu::read_status_sampled`).
            let outer = self.outer.clone();
            return Some(self.apu.borrow_mut().read_status_sampled(&mut |a| outer.borrow_mut().read(a)));
        }
        self.outer.borrow_mut().read_late(a)
    }
}

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
    /// A DMC fetch that landed inside this DMA: the get frame its read
    /// took. MEASURED on rung 0 (tests/stalls.rs, the collision probe):
    /// the sample read takes the get cycle the DMA was about to use, the
    /// core's held cycle shows for the cycle after it, and the DMA
    /// resumes with the pair that was due on the next get frame, RDY low
    /// throughout: two cycles per collision. Where rung 0 reads the
    /// sample FROM is its own finding (docs/n3-report.md): the address
    /// here is the DMC's documented one.
    collision: Option<(u64, u16)>,
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
    pub apu: Rc<RefCell<Apu>>,
    /// Test-only: the sprite DMA moves this many pairs, 256 on the die.
    /// The stall gate's MUTATE=1 sets 255 and must go red at the last
    /// pair; setting it anywhere else is a bug by name.
    pub dma_pairs: usize,
    /// The stall gate's collision mutation: false lets a DMC fetch inside
    /// the sprite DMA take no cycles, and must go red.
    pub collision_pause: bool,
    h: u64,
    dma: Option<SpriteDma>,
    fetch: Option<DmcFetch>,
    /// The frame presented this half-step.
    frame: PinFrame,
    loads: Vec<Load>,
    reset_vector: u16,
    stack_at_h0: u8,
    /// The console's bus, shared with each core's `RungBus`.
    outer: Option<Rc<RefCell<Box<dyn MicroBus>>>>,
}

impl Rung {
    pub fn new(loads: &[Load], reset_vector: u16, stack_at_h0: u8) -> Rung {
        let core = crate::core(loads, reset_vector, stack_at_h0);
        let frame = core.pins();
        Rung { core, apu: Rc::new(RefCell::new(Apu::new())), dma_pairs: 256, collision_pause: true, h: 0, dma: None, fetch: None, frame, loads: loads.to_vec(), reset_vector, stack_at_h0, outer: None }
    }

    /// The chip on a console's bus: every read and write the core makes
    /// goes to `bus` (except $4015, this chip's own), the DMA units read
    /// and write through it, and the reset vector comes from it. There is
    /// no flat image; `power_cycle` rebuilds the core on the same bus.
    pub fn with_bus(bus: Box<dyn MicroBus>, stack_at_h0: u8) -> Rung {
        let mut r = Rung {
            core: MicroCpu::new(),
            apu: Rc::new(RefCell::new(Apu::new())),
            dma_pairs: 256,
            collision_pause: true,
            h: 0,
            dma: None,
            fetch: None,
            frame: PinFrame::default(),
            loads: Vec::new(),
            reset_vector: 0,
            stack_at_h0,
            outer: Some(Rc::new(RefCell::new(bus))),
        };
        r.power_cycle();
        r
    }

    /// The console's bus, for a host that wants to reach its own board
    /// through the same object the core reads (the APU's $4015 excepted).
    pub fn bus(&self) -> Option<Rc<RefCell<Box<dyn MicroBus>>>> {
        self.outer.clone()
    }

    fn build_on_bus(&mut self) {
        let outer = self.outer.as_ref().expect("a bus").clone();
        self.apu = Rc::new(RefCell::new(Apu::new()));
        self.core = crate::core_on_bus(Box::new(RungBus { outer, apu: self.apu.clone() }), self.stack_at_h0);
    }

    /// The four-frame grain the DMA units live on: get cycles begin on
    /// frames with h = 0 mod 4 (measured: every DMA read lands there).
    fn is_get(h: u64) -> bool {
        h.is_multiple_of(4)
    }
}

impl PinEngine for Rung {
    fn power_cycle(&mut self) {
        if self.outer.is_some() {
            self.build_on_bus();
        } else {
            self.core = crate::core(&self.loads, self.reset_vector, self.stack_at_h0);
            self.apu = Rc::new(RefCell::new(Apu::new()));
        }
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
            self.dma = Some(SpriteDma { page: 0, strobe: h + 1, start: None, byte: 0, done: None, collision: None });
            self.core.set_inputs(f.res, f.irq, f.nmi, false, false);
        }
        // The core's writes reach the APU on their phi2 frame.
        if !f.rw && f.clk0 && (0x4000..=0x4017).contains(&f.ab) {
            if f.ab == 0x4014 {
                if let Some(d) = self.dma.as_mut() {
                    d.page = f.db;
                }
            } else {
                self.apu.borrow_mut().write((f.ab & 0x1f) as u8, f.db);
            }
        }
        {
            let core = &mut self.core;
            self.apu.borrow_mut().half_step(&mut |a| core.bus_read(a));
        }
        // A DMC fetch the APU performed this frame becomes a stall: RDY
        // falls one frame on (six from the enable write's frame, or
        // eight on the other APU parity; the APU says which).
        if let Some((addr, delay)) = self.apu.borrow_mut().dmc.fetched.take() {
            let request = h + delay as u64;
            self.fetch = Some(DmcFetch { addr, request, read: None, done: None });
        }
        // The standalone stall, when no sprite DMA holds the bus; inside
        // one the fetch is the DMA's collision below.
        if let Some(fe) = self.fetch.filter(|_| self.dma.is_none()) {
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
                        f.db = self.core.bus_read(fe.addr);
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
            // A DMC fetch due inside the DMA takes this get frame for its
            // read; the DMA's pairs resume four half-steps later.
            if let (Some(s), Some(fe)) = (d.start, self.fetch) {
                if d.collision.is_none() && fe.read.is_none() && h >= fe.request + 3 && h >= s && Rung::is_get(h) {
                    if self.collision_pause {
                        d.collision = Some((h, fe.addr));
                        d.start = Some(s + 4);
                    }
                    self.fetch = None;
                }
            }
            if let Some((c, addr)) = d.collision {
                if h < c + 2 {
                    // The sample read, on the DMC's own address.
                    f.ab = addr;
                    f.db = self.core.bus_read(addr);
                    f.rw = true;
                    self.dma = Some(d);
                    self.frame = f;
                    return;
                }
                if h < c + 4 {
                    // The core's held cycle shows through for one cycle.
                    self.dma = Some(d);
                    self.frame = f;
                    return;
                }
                d.collision = None;
            }
            if let Some(s) = d.start {
                let i = h - s;
                let pair = (i / 4) as usize;
                if pair < self.dma_pairs {
                    let src = ((d.page as u16) << 8) | pair as u16;
                    match i % 4 {
                        0 => {
                            d.byte = self.core.bus_read(src);
                            f.ab = src;
                            f.db = d.byte;
                            f.rw = true;
                        }
                        1 => {
                            f.ab = src;
                            f.db = d.byte;
                            f.rw = true;
                        }
                        2 => {
                            f.ab = 0x2004;
                            f.db = d.byte;
                            f.rw = false;
                        }
                        _ => {
                            // The put cycle's phi2: the byte lands.
                            f.ab = 0x2004;
                            f.db = d.byte;
                            f.rw = false;
                            self.core.bus_write(0x2004, d.byte);
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
