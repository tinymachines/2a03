//! The APU, authored: the tables are measured out of rung 0 at build time
//! (`tables`), the machinery around them is written from the step 3
//! probes (`docs/n3-report.md`) and labelled here, and the whole is held
//! to rung 0's five output codes every CPU half-step (`tests/apu.rs`).
//!
//! Units. Everything counts CPU HALF-STEPS (the pin contract's h), the
//! only unit the gate compares in. A square's sequencer steps every
//! 4(t+1) half-steps, the triangle's every 2(t+1), the noise LFSR and the
//! DMC shifter on their tables' half-step periods, and the frame
//! sequencer on its table's positions. Where a unit's action sits inside
//! its period (which half-step of the four a square's step lands on, how
//! many half-steps the output code lags the sequencer) is a FITTED
//! constant in `fit`, measured once against rung 0 and pinned, the P3
//! pattern: nothing there is typed from a diagram.
//!
//! What is authored from the probes and the published model, not
//! measured here: the envelope's divider and decay, the sweep's target
//! arithmetic (square 0 by ones' complement, square 1 by two's, both
//! measured), the linear counter, the length counter's expiry (the die
//! holds n-1 and plays n half-frame clocks), the LFSR's taps (measured),
//! the DMC's delta counter and sample cursor. The stalls a DMC fetch
//! imposes on the core are step 5's and are not modelled here: the fetch
//! is instantaneous and the gate's programs enable the DMC last.

use crate::tables;

/// The fitted half-step offsets. Each was measured by the gate's own
/// divergence dump (`APU_DUMP=1`) and pinned; a change to any of them
/// must come with the measurement that justifies it.
pub mod fit {
    /// The frame counter's position as the $4017 write's own frame
    /// begins (the recorder counted its offsets from that strobe frame).
    pub const FRAME_WRITE_LAG: u32 = 0;
    /// The write's APU cycle parity: a write whose frame index has this
    /// residue modulo four is the recorder's parity (every event at the
    /// table's offset); the other parity holds the sequencer
    /// `tables::FRAME_JITTER` half-steps before it starts (blargg's
    /// jitter). Fitted by the gate's odd-parity programs, the way the
    /// tick phases were; MUTATE=1 on the gate swaps it.
    pub fn frame_write_short_phase() -> u64 {
        if std::env::var_os("MUTATE").is_some() { 0 } else { 2 }
    }
    /// The frame counter's position at the pin contract's h=0, expressed
    /// as half-steps already elapsed toward the first phase_a. The write
    /// table counts from the write's own frame, which is the frame the
    /// APU applies it in; the first `half_step` after `new` is the frame
    /// after h=0, so one more has elapsed by then (a world with no $4017
    /// write at all, `tests/apu.rs`, put the power-on quarter frame a
    /// half-step late without it).
    pub fn frame_pos_at_h0() -> u32 {
        tables::FRAME_4[0].0 - tables::FRAME_POWER_ON + 1
    }
    /// Which of the four half-steps a square's timer ticks on: the tick
    /// lands on phi1 of alternate CPU cycles, 3 half-steps after a $4003
    /// strobe (`apu-channel-probe seq`), so this is that strobe's phase.
    pub const SQ_TICK_PHASE: u32 = 3;
    pub const TRI_TICK_PHASE: u32 = 0;
    /// The two LFSR timers tick on the other phase of the same grain
    /// (frames k with k % 4 == 0; the squares' are k % 4 == 2).
    pub const LFSR_TICK_PHASE: u32 = 1;
    /// Half-steps from the tick that lands a timer on its terminal to the
    /// unit's clock showing on the output: the noise shift and the DMC
    /// completion, each fitted against rung 0's code stream.
    pub const NOISE_UNIT_LAG: u8 = 3;
    pub const DMC_UNIT_LAG: u8 = 3;
    /// Half-steps a square's (and the noise's) output code lags the
    /// state it is computed from (measured 2: the step changes at the
    /// tick, the code two later; the envelope's new volume likewise).
    pub const SQ_OUT_LAG: usize = 2;
    /// Half-steps from a frame phase's rise to its effect on the
    /// envelopes, lengths and sweeps: fitted 1 (the codes then show it
    /// through their 2-half-step output lag).
    pub const FRAME_EFFECT_LAG: u8 = 1;
    /// The same for the triangle's linear counter, whose output has no
    /// lag: measured 3 (`apu-channel-probe triseq`).
    pub const TRI_FRAME_LAG: u8 = 3;
    /// The squares' length counters: their expiry shows two half-steps
    /// after the noise's from the same half-frame clock (fitted 3; the
    /// squares' sweeps and envelopes stay at FRAME_EFFECT_LAG).
    pub const SQ_LENGTH_LAG: u8 = 3;
    use crate::tables;
}

/// One of the die's two LFSR-shaped timers at run time (`tables::LfsrTimer`
/// is the measurement; this is the machine). `pending` counts down to the
/// unit's clock after a terminal tick.
#[derive(Clone, Copy, Debug)]
pub struct Timer {
    t: u32,
    mask: u32,
    pending: u8,
    lag: u8,
    spec: &'static tables::LfsrTimer,
}

impl Timer {
    fn new(spec: &'static tables::LfsrTimer, bits: u32, lag: u8) -> Timer {
        Timer { t: spec.at_h0, mask: (1 << bits) - 1, pending: 0, lag, spec }
    }
    /// One tick; `rate` selects the reload. Returns nothing: the unit's
    /// clock comes from `half_step` when `pending` runs out.
    fn tick(&mut self, rate: u8) {
        if self.t == self.spec.terminal {
            self.t = self.spec.reload[rate as usize & 15] as u32;
        } else {
            let (i, j) = self.spec.taps;
            let fb = (self.t >> i & 1) ^ (self.t >> j & 1);
            self.t = ((self.t << 1) & self.mask) | fb;
            if self.t == self.spec.terminal {
                self.pending = self.lag;
            }
        }
    }
    /// Every half-step: true on the one where the unit clocks.
    fn half_step(&mut self) -> bool {
        if self.pending > 0 {
            self.pending -= 1;
            return self.pending == 0;
        }
        false
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Envelope {
    start: bool,
    divider: u8,
    decay: u8,
    period: u8,
    loop_: bool,
    constant: bool,
}

impl Envelope {
    fn write(&mut self, v: u8) {
        self.period = v & 0x0f;
        self.constant = v & 0x10 != 0;
        self.loop_ = v & 0x20 != 0;
    }
    fn quarter(&mut self) {
        if self.start {
            self.start = false;
            self.decay = 15;
            self.divider = self.period;
        } else if self.divider == 0 {
            self.divider = self.period;
            if self.decay > 0 {
                self.decay -= 1;
            } else if self.loop_ {
                self.decay = 15;
            }
        } else {
            self.divider -= 1;
        }
    }
    fn volume(&self) -> u8 {
        if self.constant { self.period } else { self.decay }
    }
}

/// The length counter as the die holds it: loaded with the table's n-1,
/// expiring on the clock that would take it below zero, so a channel
/// plays for n half-frame clocks.
#[derive(Clone, Copy, Default, Debug)]
pub struct Length {
    count: u8,
    expired: bool,
    halt: bool,
    enabled: bool,
}

impl Length {
    fn load(&mut self, idx: u8) {
        if self.enabled {
            self.count = tables::LENGTH[idx as usize & 31];
            self.expired = false;
        }
    }
    fn half(&mut self) {
        if !self.halt && !self.expired {
            if self.count == 0 {
                self.expired = true;
            } else {
                self.count -= 1;
            }
        }
    }
    fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.count = 0;
            self.expired = true;
        }
    }
    fn playing(&self) -> bool {
        !self.expired
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Square {
    second: bool,
    duty: u8,
    env: Envelope,
    len: Length,
    period: u16,
    timer: u32,
    /// A low-byte write makes the next tick a reload (t = p) instead of a
    /// decrement, with no step: measured on both the squares (the duty
    /// drop lands one tick after an immediate load would put it) and the
    /// triangle (whose tick frame is the strobe frame, so its timer shows
    /// the period on the strobe itself).
    reload_pending: bool,
    step: u8,
    // sweep
    swp_enabled: bool,
    swp_period: u8,
    swp_negate: bool,
    swp_shift: u8,
    swp_divider: u8,
    swp_reload: bool,
}

impl Square {
    fn write(&mut self, reg: u8, v: u8) {
        match reg & 3 {
            0 => {
                self.duty = v >> 6;
                self.env.write(v);
                self.len.halt = v & 0x20 != 0;
            }
            1 => {
                self.swp_enabled = v & 0x80 != 0;
                self.swp_period = (v >> 4) & 7;
                self.swp_negate = v & 0x08 != 0;
                self.swp_shift = v & 7;
                self.swp_reload = true;
            }
            2 => {
                self.period = (self.period & 0x700) | v as u16;
                self.reload_pending = true;
            }
            _ => {
                self.period = (self.period & 0xff) | ((v as u16 & 7) << 8);
                self.len.load(v >> 3);
                self.env.start = true;
                // The published restart of the sequencer; unobservable in
                // the gate's programs (c is 0 from power-on when they
                // write), so authored, not measured.
                self.step = 0;
            }
        }
    }
    /// The sweep's target, with the two squares' measured complements.
    fn target(&self) -> i32 {
        let change = (self.period >> self.swp_shift) as i32;
        if self.swp_negate {
            self.period as i32 - change - if self.second { 0 } else { 1 }
        } else {
            self.period as i32 + change
        }
    }
    fn muted(&self) -> bool {
        self.period < 8 || self.target() > 0x7ff
    }
    /// The half-frame clock's sweep half; the length counter is clocked
    /// apart (`fit::SQ_LENGTH_LAG`).
    fn half(&mut self) {
        let t = self.target();
        if self.swp_divider == 0 && self.swp_enabled && self.swp_shift > 0 && !self.muted() && t >= 0 {
            self.period = t as u16;
        }
        if self.swp_divider == 0 || self.swp_reload {
            self.swp_divider = self.swp_period;
            self.swp_reload = false;
        } else {
            self.swp_divider -= 1;
        }
    }
    /// Every 4 half-steps: the timer holds p, counts to 0, and the
    /// reload steps the sequencer, p+1 ticks per step (measured).
    fn tick(&mut self) {
        if self.reload_pending {
            self.reload_pending = false;
            self.timer = self.period as u32;
        } else if self.timer == 0 {
            self.timer = self.period as u32;
            // With the period 0 (power-on, or a write of 0) the die's
            // step counter holds: sq_c stayed 0 from power-on to the
            // first $4003 write with t=0 throughout (`seq` probe).
            if self.period != 0 {
                self.step = (self.step + 7) & 7; // counts down, as the die's sq_c does
            }
        } else {
            self.timer -= 1;
        }
    }
    fn out(&self) -> u8 {
        let high = tables::DUTY[self.duty as usize] >> self.step & 1 != 0;
        if high && self.len.playing() && !self.muted() { self.env.volume() } else { 0 }
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Triangle {
    len: Length,
    control: bool,
    lin_reload_value: u8,
    lin: u8,
    lin_reload: bool,
    period: u16,
    timer: u32,
    reload_pending: bool,
    step: u8,
}

impl Triangle {
    fn write(&mut self, reg: u8, v: u8) {
        match reg & 3 {
            0 => {
                self.control = v & 0x80 != 0;
                self.len.halt = self.control;
                self.lin_reload_value = v & 0x7f;
            }
            2 => {
                // The next tick reloads (the strobe frame is a tick frame
                // for this timer, so t = p shows on the strobe itself);
                // the high byte's write leaves the timer alone.
                self.period = (self.period & 0x700) | v as u16;
                self.reload_pending = true;
            }
            3 => {
                self.period = (self.period & 0xff) | ((v as u16 & 7) << 8);
                self.len.load(v >> 3);
                self.lin_reload = true;
            }
            _ => {}
        }
    }
    fn quarter(&mut self) {
        if self.lin_reload {
            self.lin = self.lin_reload_value;
        } else if self.lin > 0 {
            self.lin -= 1;
        }
        if !self.control {
            self.lin_reload = false;
        }
    }
    /// Every 2 half-steps: holds p, counts to 0, the reload steps the
    /// sequencer when both counters allow, p+1 ticks per step.
    fn tick(&mut self) {
        if self.reload_pending {
            self.reload_pending = false;
            self.timer = self.period as u32;
        } else if self.timer == 0 {
            self.timer = self.period as u32;
            if self.lin > 0 && self.len.playing() && self.period != 0 {
                self.step = (self.step + 1) & 31;
            }
        } else {
            self.timer -= 1;
        }
    }
    fn out(&self) -> u8 {
        // 15..0 then 0..15: the measured 32-step sequence.
        if self.step < 16 { 15 - self.step } else { self.step - 16 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Noise {
    env: Envelope,
    len: Length,
    mode1: bool,
    period_idx: u8,
    timer: Timer,
    /// Bit i is the die's noi_c{i}; the shift moves toward c14 and c0
    /// takes the feedback (measured over 24 states in each mode).
    lfsr: u16,
}

impl Default for Noise {
    fn default() -> Noise {
        Noise { env: Envelope::default(), len: Length::default(), mode1: false, period_idx: 0, timer: Timer::new(&tables::NOISE_TIMER, 11, fit::NOISE_UNIT_LAG), lfsr: 0x7ffe }
    }
}

impl Noise {
    fn write(&mut self, reg: u8, v: u8) {
        match reg & 3 {
            0 => {
                self.env.write(v);
                self.len.halt = v & 0x20 != 0;
            }
            2 => {
                self.mode1 = v & 0x80 != 0;
                self.period_idx = v & 0x0f;
            }
            3 => {
                self.len.load(v >> 3);
                self.env.start = true;
            }
            _ => {}
        }
    }
    fn tick(&mut self) {
        self.timer.tick(self.period_idx);
    }
    fn half_step(&mut self) {
        if self.timer.half_step() {
            let c14 = self.lfsr >> 14 & 1;
            let tap = if self.mode1 { self.lfsr >> 8 & 1 } else { self.lfsr >> 13 & 1 };
            self.lfsr = ((self.lfsr << 1) & 0x7fff) | (c14 ^ tap);
        }
    }
    fn out(&self) -> u8 {
        if self.lfsr >> 14 & 1 == 0 && self.len.playing() { self.env.volume() } else { 0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Dmc {
    irq_enable: bool,
    loop_: bool,
    rate: u8,
    out: u8,
    sample_addr: u16,
    sample_len: u16,
    addr: u16,
    remaining: u16,
    buffer: Option<u8>,
    shifter: u8,
    /// The die's `pcm_bits`, counting UP 0..7 per completion; the byte
    /// boundary is the wrap to 0.
    bits: u8,
    silence: bool,
    timer: Timer,
    pub irq: bool,
    enabled: bool,
    /// A sample byte fetched this half-step, for the rung's stall: the
    /// address, and whether the enable write asked for it (the byte
    /// boundary's fetch and the enable's reach the bus on different
    /// schedules, measured).
    pub fetched: Option<(u16, bool)>,
}

impl Default for Dmc {
    /// Power-on as measured: silence set, the shifter all ones, the bit
    /// counter at 0, the timer at its h=0 state.
    fn default() -> Dmc {
        Dmc {
            irq_enable: false,
            loop_: false,
            rate: 0,
            out: 0,
            sample_addr: 0xc000,
            sample_len: 1,
            addr: 0,
            remaining: 0,
            buffer: None,
            shifter: 0xff,
            bits: 0,
            silence: true,
            timer: Timer::new(&tables::DMC_TIMER, 9, fit::DMC_UNIT_LAG),
            irq: false,
            enabled: false,
            fetched: None,
        }
    }
}

impl Dmc {
    fn write(&mut self, reg: u8, v: u8) {
        match reg & 3 {
            0 => {
                self.irq_enable = v & 0x80 != 0;
                if !self.irq_enable {
                    self.irq = false;
                }
                self.loop_ = v & 0x40 != 0;
                self.rate = v & 0x0f;
            }
            1 => self.out = v & 0x7f,
            2 => self.sample_addr = 0xc000 | ((v as u16) << 6),
            _ => self.sample_len = ((v as u16) << 4) | 1,
        }
    }
    fn set_enabled(&mut self, on: bool, read: &mut dyn FnMut(u16) -> u8) {
        self.enabled = on;
        if on {
            if self.remaining == 0 {
                self.addr = self.sample_addr;
                self.remaining = self.sample_len;
            }
            if self.buffer.is_none() {
                self.fetch(read);
                if let Some(f) = self.fetched.as_mut() {
                    f.1 = true;
                }
            }
        } else {
            self.remaining = 0;
        }
    }
    fn fetch(&mut self, read: &mut dyn FnMut(u16) -> u8) {
        if self.remaining == 0 {
            return;
        }
        self.fetched = Some((self.addr, false));
        self.buffer = Some(read(self.addr));
        self.addr = if self.addr == 0xffff { 0x8000 } else { self.addr + 1 };
        self.remaining -= 1;
        if self.remaining == 0 {
            if self.loop_ {
                self.addr = self.sample_addr;
                self.remaining = self.sample_len;
            } else if self.irq_enable {
                self.irq = true;
            }
        }
    }
    fn tick(&mut self) {
        self.timer.tick(self.rate);
    }
    /// A completion, the measured order (`apu-channel-probe dmcseq`): the
    /// output moves by the bit shifted out unless silent, the shifter
    /// shifts, the bit counter wraps, and at the wrap the buffer (if
    /// full) becomes the next byte and its refill is fetched.
    fn half_step(&mut self, read: &mut dyn FnMut(u16) -> u8) {
        if !self.timer.half_step() {
            return;
        }
        if !self.silence {
            if self.shifter & 1 != 0 {
                if self.out <= 125 {
                    self.out += 2;
                }
            } else if self.out >= 2 {
                self.out -= 2;
            }
        }
        self.shifter >>= 1;
        self.bits = (self.bits + 1) & 7;
        if self.bits == 0 {
            match self.buffer.take() {
                Some(b) => {
                    self.silence = false;
                    self.shifter = b;
                    self.fetch(read);
                }
                None => self.silence = true,
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Apu {
    pub sq: [Square; 2],
    pub tri: Triangle,
    pub noise: Noise,
    pub dmc: Dmc,
    /// Half-steps elapsed on the frame sequencer's clock since its
    /// (re)start; compared against the mode's table.
    frame_pos: u32,
    /// Half-steps the sequencer still waits before `frame_pos` moves
    /// (the other write parity's jitter).
    frame_hold: u32,
    five_step: bool,
    irq_inhibit: bool,
    pub frame_irq: bool,
    /// Half-cycles since the contract's h=0.
    pub h: u64,
    /// The squares' and the noise's codes as they were `fit::SQ_OUT_LAG`
    /// half-steps ago.
    sq_lag: [[u8; 3]; 4],
    /// A frame phase whose effect is due in this many half-steps, once
    /// for the envelopes/lengths/sweeps and once for the triangle.
    frame_due: Option<(u8, u8)>,
    tri_due: Option<u8>,
    sq_len_due: Option<u8>,
    /// A register write presented this frame, applied as the frame's
    /// `half_step` begins, before its ticks.
    pending: Option<(u8, u8)>,
}

impl Default for Apu {
    fn default() -> Apu {
        Apu::new()
    }
}

impl Apu {
    /// The APU as it stands at the pin contract's h=0 after power-on:
    /// the frame counter already `fit::frame_pos_at_h0()` half-steps into
    /// its first period (measured, `tables::FRAME_POWER_ON`).
    pub fn new() -> Apu {
        let mut sq = [Square::default(); 2];
        sq[1].second = true;
        Apu {
            sq,
            tri: Triangle::default(),
            noise: Noise::default(),
            dmc: Dmc::default(),
            frame_pos: fit::frame_pos_at_h0(),
            frame_hold: 0,
            five_step: false,
            irq_inhibit: false,
            frame_irq: false,
            h: 0,
            sq_lag: [[0; 3]; 4],
            frame_due: None,
            tri_due: None,
            sq_len_due: None,
            pending: None,
        }
    }

    /// A CPU write to $4000 + reg, presented on its phi2 frame; applied
    /// as that frame's `half_step` begins.
    pub fn write(&mut self, reg: u8, v: u8) {
        debug_assert!(self.pending.is_none(), "two writes in one half-cycle");
        self.pending = Some((reg, v));
    }

    fn apply(&mut self, reg: u8, v: u8, read: &mut dyn FnMut(u16) -> u8) {
        match reg {
            0x00..=0x03 => self.sq[0].write(reg, v),
            0x04..=0x07 => self.sq[1].write(reg, v),
            0x08..=0x0b => self.tri.write(reg, v),
            0x0c..=0x0f => self.noise.write(reg, v),
            0x10..=0x13 => self.dmc.write(reg, v),
            0x15 => {
                self.sq[0].len.set_enabled(v & 1 != 0);
                self.sq[1].len.set_enabled(v & 2 != 0);
                self.tri.len.set_enabled(v & 4 != 0);
                self.noise.len.set_enabled(v & 8 != 0);
                self.dmc.set_enabled(v & 0x10 != 0, read);
                self.dmc.irq = false;
            }
            0x17 => {
                self.five_step = v & 0x80 != 0;
                self.irq_inhibit = v & 0x40 != 0;
                if self.irq_inhibit {
                    self.frame_irq = false;
                }
                // The sequencer restarts; on the other APU parity it
                // waits FRAME_JITTER half-steps first, and every event
                // lands that much later (measured, `apu-write-probe`).
                let hold = if self.h % 4 == fit::frame_write_short_phase() { 0 } else { tables::FRAME_JITTER };
                self.frame_pos = fit::FRAME_WRITE_LAG;
                self.frame_hold = hold;
                if self.five_step {
                    // A mode-1 write clocks the quarter and half frames
                    // at once: the same effects a phase-1 rise schedules,
                    // from the write's own frame, so the length counters
                    // move at MODE1_CLOCK half-steps after the strobe
                    // (three, the measured figure, which is the phase
                    // path's own length lag).
                    let lead = (tables::MODE1_CLOCK - fit::SQ_LENGTH_LAG as u32) as u8 + hold as u8;
                    self.frame_due = Some((1, fit::FRAME_EFFECT_LAG + lead));
                    self.tri_due = Some(fit::TRI_FRAME_LAG + lead);
                    self.sq_len_due = Some(fit::SQ_LENGTH_LAG + lead);
                }
            }
            _ => {}
        }
    }

    /// $4015 as read: the length counters' states, the two IRQ flags.
    /// Reading clears the frame IRQ flag.
    pub fn read_status(&mut self) -> u8 {
        let mut v = 0u8;
        for (i, s) in self.sq.iter().enumerate() {
            if s.len.playing() && s.len.enabled {
                v |= 1 << i;
            }
        }
        if self.tri.len.playing() && self.tri.len.enabled {
            v |= 4;
        }
        if self.noise.len.playing() && self.noise.len.enabled {
            v |= 8;
        }
        if self.dmc.remaining > 0 {
            v |= 0x10;
        }
        if self.frame_irq {
            v |= 0x40;
        }
        if self.dmc.irq {
            v |= 0x80;
        }
        self.frame_irq = false;
        v
    }

    fn quarter(&mut self) {
        self.sq[0].env.quarter();
        self.sq[1].env.quarter();
        self.noise.env.quarter();
    }

    fn half(&mut self) {
        self.sq[0].half();
        self.sq[1].half();
        self.tri.len.half();
        self.noise.len.half();
    }

    /// One CPU half-cycle. `read` is the CPU bus as the DMC's sample
    /// fetch sees it (the console's, or a flat image in the gates).
    pub fn half_step(&mut self, read: &mut dyn FnMut(u16) -> u8) {
        if let Some((reg, v)) = self.pending.take() {
            self.apply(reg, v, read);
        }
        // The frame sequencer: the table's phase rises at their measured
        // offsets, repeating on the period.
        let (table, period) = if self.five_step {
            (tables::FRAME_5, tables::FRAME_5_PERIOD)
        } else {
            (tables::FRAME_4, tables::FRAME_4_PERIOD)
        };
        for &(at, phase) in table {
            if self.frame_pos == at {
                self.frame_due = Some((phase, fit::FRAME_EFFECT_LAG));
                self.tri_due = Some(fit::TRI_FRAME_LAG);
                if phase != 0 && phase != 2 {
                    self.sq_len_due = Some(fit::SQ_LENGTH_LAG);
                }
            }
        }
        if let Some(due) = self.sq_len_due {
            if due == 0 {
                self.sq_len_due = None;
                self.sq[0].len.half();
                self.sq[1].len.half();
            } else {
                self.sq_len_due = Some(due - 1);
            }
        }
        if let Some(due) = self.tri_due {
            if due == 0 {
                self.tri_due = None;
                self.tri.quarter();
            } else {
                self.tri_due = Some(due - 1);
            }
        }
        if let Some((phase, due)) = self.frame_due {
            if due == 0 {
                self.frame_due = None;
                match phase {
                    0 | 2 => self.quarter(),
                    1 => {
                        self.quarter();
                        self.half();
                    }
                    3 => {
                        self.quarter();
                        self.half();
                        if !self.irq_inhibit {
                            self.frame_irq = true;
                        }
                    }
                    _ => {
                        self.quarter();
                        self.half();
                    }
                }
            } else {
                self.frame_due = Some((phase, due - 1));
            }
        }
        if self.frame_hold > 0 {
            self.frame_hold -= 1;
        } else {
            self.frame_pos += 1;
            if self.frame_pos >= table[0].0 + period {
                self.frame_pos = table[0].0;
            }
        }
        // The timers, each on its own half-step grain.
        if (self.h as u32 + fit::SQ_TICK_PHASE).is_multiple_of(4) {
            self.sq[0].tick();
            self.sq[1].tick();
        }
        if (self.h as u32 + fit::TRI_TICK_PHASE).is_multiple_of(2) {
            self.tri.tick();
        }
        if (self.h as u32 + fit::LFSR_TICK_PHASE).is_multiple_of(4) {
            self.noise.tick();
            self.dmc.tick();
        }
        self.noise.half_step();
        self.dmc.half_step(read);
        self.sq_lag[(self.h as usize) % 4] = [self.sq[0].out(), self.sq[1].out(), self.noise.out()];
        self.h += 1;
    }

    /// The five output codes rung 0 exposes on `sq0_out`, `sq1_out`,
    /// `tri_out`, `noi_out` (4 bits) and `pcm_out` (7 bits).
    pub fn codes(&self) -> [u8; 5] {
        // The squares' codes lag their sequencers (fit::SQ_OUT_LAG); the
        // FIFO holds the last four, indexed by h.
        // The entry for frame k sits at (k-1) % 4; the code shown at
        // frame k is frame k-LAG's.
        let lagged = self.sq_lag[(self.h as usize + 4 - 1 - fit::SQ_OUT_LAG) % 4];
        [lagged[0], lagged[1], self.tri.out(), lagged[2], self.dmc.out]
    }
}
