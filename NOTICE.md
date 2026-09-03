# Notices

The code in this repository is MIT (see `LICENSE`). What it consumes
is not:

- `extern/visual2a03/` (fetched by `tools/fetch-netlist.sh`, pinned by
  sha256, **never committed**): Quietust's Visual 2A03 die data and
  simulator (segdefs, transdefs, nodenames, wires.js, chipsim.js,
  macros.js, testprogram.js, memtable.js), from
  `www.qmtpro.com/~nes/chipimages/visual2a03/`. The pages carry no
  explicit licence text (checked 2026-09-02); the netlist derives from
  the visual6502 team's RP2A03 die photography, which is CC BY-NC-SA,
  so this repository treats the data as NC-SA with attribution to
  Quietust and visual6502.org. **NonCommercial and ShareAlike
  propagate to any artifact embedding it**, which includes any build
  of `v2a03-netlist` made with the extern present. The open courtesy
  item about confirming terms with Quietust directly is shared with
  the 2c02 repository.
- The golden trace (`tools/golden-trace/golden-2a03.txt`, gitignored)
  is generated locally by running that simulator and is derived data
  under the same terms.
