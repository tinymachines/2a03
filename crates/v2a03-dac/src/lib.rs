//! The two audio DACs: the five channels' digital output codes to the
//! two audio pins' levels, in the units nes-bus's `CpuPins` doc calls
//! "the units the mixer table uses" (0 at silence, most of 1.0 never
//! reached). Its own crate with no dependencies so the console (N7)
//! reaches it without the switch-level crates; `v2a03-sim` re-exports
//! it as `mixer`, the name A3 used.
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
//!
//! What the constants are, read against the NES-001 schematic
//! (2026-09-06, the console's N7 plan): each pin is pulled down by
//! 100 ohms on the board (R3, R4), which is the "+100" in both
//! groups; the two groups' numerators are in the ratio 20/12
//! (159.79/95.88), the board's summing resistors R7 20K on AD1 and R8
//! 12K on AD2, so `ad1 + ad2` is the two pins as the summing node
//! weights them, not the pins themselves.

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
