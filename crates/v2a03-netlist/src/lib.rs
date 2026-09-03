//! The 2A03's netlist as a value: halfphi's blob, embedded at build
//! time, decoded on demand. The counts are measured by the build and
//! pinned by the tests against the reference simulator's own loading of
//! the same files, so a parser change that silently altered the chip
//! would fail by number.

use std::sync::Arc;

pub use halfphi::{Engine, Netlist};

include!(concat!(env!("OUT_DIR"), "/counts.rs"));

static BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/netlist.bin"));

/// Whether the die data was present at build time. Tests SKIP by name
/// when it was not; nothing else should ever branch on this.
pub fn available() -> bool {
    !BLOB.is_empty()
}

/// The decoded netlist. Panics by name when built data-free.
pub fn netlist() -> Arc<Netlist> {
    assert!(
        available(),
        "built without extern/visual2a03: run tools/fetch-netlist.sh and rebuild"
    );
    Arc::new(Netlist::decode(BLOB).expect("embedded blob decodes"))
}
