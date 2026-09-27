//! The chip's saved state (feature `state`): everything a console needs
//! to stop the 2A03 anywhere and start it again there, as serde data. The
//! console encodes it; this says what it is.
//!
//! The core is the 6502 repository's rung 3, and its own `snapshot` and
//! `restore` are the one account of what the core holds (`MicroState`,
//! at the pinned revision). The mirrors below only teach serde that
//! account's shape: every field is listed, and a field the 6502 adds is a
//! compile error here, not a field quietly left out of every save.
//!
//! Not saved, because a console builds them the same way before it
//! restores: the bus, the core's configuration (the decimal adjust
//! disconnected, the stack seed) and the two test knobs.

use serde::{Deserialize, Serialize};
use v6502_micro::datapath::Datapath;
use v6502_micro::flags::Caps;
use v6502_micro::machine::MicroState;
use v6502_pins::{PinEngine, PinFrame};

use crate::apu::Apu;
use crate::rung::{DmcFetch, Rung, SpriteDma};

#[derive(Serialize, Deserialize)]
#[serde(remote = "Datapath")]
struct DatapathDef {
    lax_magic: u8,
    a: u8,
    x: u8,
    y: u8,
    s_in: u8,
    s_out: u8,
    pcl: u8,
    pch: u8,
    pclp: u8,
    pchp: u8,
    abl: u8,
    abh: u8,
    dl: u8,
    dor: u8,
    add: u8,
    ai: u8,
    bi: u8,
    dec_add: u8,
    sb: u8,
    db: u8,
    adl: u8,
    adh: u8,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "Caps")]
struct CapsDef {
    last_read: u8,
    last_write: u8,
    sum: Option<(u8, u8, bool, u8)>,
    sum_daa: bool,
    sum_pre: Option<(u8, u8, bool, u8)>,
    srs: Option<(u8, u8)>,
    srs_pre: Option<(u8, u8)>,
    logic: Option<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "MicroState")]
struct MicroStateDef {
    mem: Vec<u8>,
    half_cycle: u64,
    p: u8,
    #[serde(with = "DatapathDef")]
    dp: Datapath,
    stream: u8,
    pos: usize,
    phi1_next: bool,
    op: u8,
    cur_key: u8,
    kil: bool,
    cin_from_c: bool,
    seam: u64,
    #[serde(with = "CapsDef")]
    caps: Caps,
    reads: u32,
    writes: u32,
    next_op: u8,
    fetch_pc: u16,
    pin_w: u64,
    pin_db: u8,
    pin_hold: u8,
    inputs: [bool; 5],
    irq_seen: bool,
    nmi_pending: bool,
    nmi_low_at_phi1: bool,
    brk_takes_nmi: bool,
    irq_seen_prev: bool,
    nmi_pending_prev: bool,
    hijack_next: u8,
    hijacked: u8,
    stalled: bool,
    res_seen: bool,
    res_pend: bool,
    res_sel: bool,
    res_phase: u8,
    mask_sync: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "PinFrame")]
struct PinFrameDef {
    h: u64,
    clk0: bool,
    ab: u16,
    db: u8,
    rw: bool,
    sync: bool,
    res: bool,
    irq: bool,
    nmi: bool,
    rdy: bool,
    so: bool,
}

/// The 2A03 at one half-step: the core, the APU, the DMA units, the pins
/// as last presented, and the held read's memo.
#[derive(Serialize, Deserialize)]
pub struct RungState {
    #[serde(with = "MicroStateDef")]
    core: MicroState,
    apu: Apu,
    h: u64,
    dma: Option<SpriteDma>,
    fetch: Option<DmcFetch>,
    #[serde(with = "PinFrameDef")]
    frame: PinFrame,
    quiet: bool,
    memo: Vec<Option<u8>>,
}

impl Rung {
    /// Whether the chip stands where a state can be taken: at the end of
    /// a CPU cycle, its phi2 just played. Inside a cycle the core holds
    /// the byte its read latched as the clock fell, and the pinned core's
    /// `MicroState` does not carry it: restored there, the core would ask
    /// the bus again at phi2 (a second read of a register that counts its
    /// reads, and the wrong byte on the pins, which tests/state.rs saw).
    pub fn at_cycle_end(&self) -> bool {
        self.pins().clk0
    }

    /// The chip as it stands; refused inside a CPU cycle (`at_cycle_end`).
    pub fn save_state(&self) -> Result<RungState, String> {
        if !self.at_cycle_end() {
            return Err("a state is taken where a CPU cycle ends, and this is its phi1".into());
        }
        let (dma, fetch, frame) = self.state_parts();
        Ok(RungState {
            core: self.core.snapshot(),
            apu: *self.apu.borrow(),
            h: self.h(),
            dma,
            fetch,
            frame,
            quiet: self.quiet_now(),
            memo: self.memo_now(),
        })
    }

    /// The chip as `st` saved it, on the bus it already has. A state the
    /// core refuses, or a memo of the wrong size, leaves the chip as it
    /// was.
    pub fn load_state(&mut self, st: &RungState) -> Result<(), String> {
        if st.memo.len() != 0x10000 {
            return Err(format!("the memo is {} entries, not 65536", st.memo.len()));
        }
        self.core.restore(&st.core)?;
        *self.apu.borrow_mut() = st.apu;
        // MUTATE_STATE=1 drops the DMC fetch in flight, and tests/state.rs
        // must go red.
        let fetch = if std::env::var_os("MUTATE_STATE").is_some() { None } else { st.fetch };
        self.set_state_parts(st.h, st.dma, fetch, st.frame, st.quiet, &st.memo);
        Ok(())
    }
}
