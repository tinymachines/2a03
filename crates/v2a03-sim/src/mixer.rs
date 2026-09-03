//! The mixer: the five channels' digital output codes to the two audio
//! pins' levels, in the units nes-bus's `CpuPins` doc calls "the units
//! the mixer table uses" (0 at silence, most of 1.0 never reached).
//!
//! AUTHORED FROM THE NESDEV WIKI, not measured here: the constants are
//! the "APU Mixer" page's own (https://www.nesdev.org/wiki/APU_Mixer,
//! read 2026-09-03), the same provenance level as nes-bus's pin tables.
//! AD1 carries the two squares; AD2 carries triangle, noise and DMC.
//! The page's convention that an all-zero group outputs exactly zero is
//! kept explicitly, because the formula alone would divide by zero.
//! Measuring the real pins against this table is bench work (the
//! console sketch's capture list), and until then this file is a
//! labelled claim, exactly like the pin tables were before N0's gates.

/// AD1: square 0 and square 1, each a 4-bit code 0..=15.
pub fn ad1(sq0: u8, sq1: u8) -> f32 {
    let sum = (sq0 + sq1) as f64;
    if sum == 0.0 {
        return 0.0;
    }
    (95.88 / (8128.0 / sum + 100.0)) as f32
}

/// AD2: triangle and noise (4-bit codes) and the DMC level (7-bit).
pub fn ad2(tri: u8, noi: u8, pcm: u8) -> f32 {
    let inner = tri as f64 / 8227.0 + noi as f64 / 12241.0 + pcm as f64 / 22638.0;
    if inner == 0.0 {
        return 0.0;
    }
    (159.79 / (1.0 / inner + 100.0)) as f32
}
