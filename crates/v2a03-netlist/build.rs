//! Parses the Visual 2A03 die data through halfphi and embeds the
//! resulting netlist blob, plus the counts measured from it (which the
//! tests hold to the reference simulator's own independently loaded
//! numbers).
//!
//! Without extern/visual2a03 (tools/fetch-netlist.sh) the crate still
//! builds, data-free: the blob is empty, `netlist_missing` is set, and
//! the library refuses at runtime by name. A fresh clone must build and
//! its tests must SKIP loudly rather than fail, the family pattern.

use std::path::Path;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(netlist_missing)");
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let ext = Path::new(&manifest).join("../../extern/visual2a03");
    println!("cargo:rerun-if-changed={}", ext.display());
    let out = std::env::var("OUT_DIR").unwrap();
    let out = Path::new(&out);

    if !ext.join("segdefs.js").exists() {
        println!("cargo::rustc-cfg=netlist_missing");
        println!(
            "cargo:warning=extern/visual2a03 not fetched (tools/fetch-netlist.sh); building data-free"
        );
        std::fs::write(out.join("netlist.bin"), []).unwrap();
        std::fs::write(
            out.join("counts.rs"),
            "pub const NODE_COUNT: usize = 0;\npub const TRANSISTOR_COUNT: usize = 0;\npub const NAME_COUNT: usize = 0;\n",
        )
        .unwrap();
        return;
    }

    let read = |f: &str| std::fs::read_to_string(ext.join(f)).unwrap();
    let parsed = halfphi::parse(&halfphi::ChipSource {
        segdefs: &read("segdefs.js"),
        transdefs: &read("transdefs.js"),
        nodenames: &read("nodenames.js"),
        // The 2A03 spells its rails gnd/pwr, like the 2C02.
        rails: halfphi::Rails { ground: "gnd", supply: "pwr" },
    })
    .expect("visual2a03 data did not parse");
    let nl = halfphi::Netlist::decode(&parsed.blob).expect("blob decodes");

    std::fs::write(out.join("netlist.bin"), &parsed.blob).unwrap();
    std::fs::write(
        out.join("counts.rs"),
        format!(
            "pub const NODE_COUNT: usize = {};\npub const TRANSISTOR_COUNT: usize = {};\npub const NAME_COUNT: usize = {};\n",
            nl.node_count(),
            nl.transistor_count(),
            parsed.name_count,
        ),
    )
    .unwrap();
}
