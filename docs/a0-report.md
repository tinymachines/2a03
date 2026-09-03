# A0 report: the fifth chip loads, settles, and replays with no exemptions

Run stamp: 2026-09-02, rustc 1.97.1, halfphi v0.1.3 (released for this
milestone; see the finding below). `cargo test --workspace --release`
with `REQUIRE_NETLIST=1 REQUIRE_GOLDEN=1`: 3 tests green (counts,
convergence, the golden). `MUTATE=1` goes red: the supply-gated fix-up
switched off diverges from the golden at step 0.

## What closed

- **The netlist**: Quietust's Visual 2A03, eight files fetched by
  pinned sha256 (recorded at first fetch, 2026-09-02). Both real
  parsers agree: **10,946 conducting transistors over 5,577 defined
  nodes** in a 20,001-id space, 2,021 names, rails spelled `gnd`/`pwr`
  like the 2C02.
- **Power-on converges unaided**, zero nonconvergent settles.
- **The node golden replays bit-exact on every node across 601 states
  with no exemption list at all**, the first chip in the family to
  replay with none (the 6502 carries the rail encoding caveat, the
  2C02 its nine reset-less latches). The unit is the MASTER half-step,
  one `clk_in` toggle; the ÷12 `clk0` is an output the divider
  produces, and the golden generator restates macros.js initChip
  statement for statement with the buses undriven.

## The finding: halfphi's drive order, finally exercised

The first replay diverged on exactly eight nodes, every one inverted
for all 601 states, two of them named: `so` and `c_so`
(`examples/a0-diverge-probe.rs`). The cause is the case the family had
documented and never seen: `Stats::contested_groups`, zero on every
chip since the count was built, is **3** on the 2A03 from power-on.
Quietust's init drives the `so` node low while its group carries a
layout pullup; the reference resolves such a group by first match
walking out from the driven seed (low), and halfphi's old
`PullDown < PullUp` order resolved it by maximum (high).

halfphi 0.1.3 swaps the two: **PullDown outranks PullUp**, the
external drive beating the depletion load, which is both the physical
reading and the reference's observed one. With the swap the replay is
bit-exact, zero divergent nodes. The swap is proven unobservable
everywhere else: contested is 0 on the 6502, 6800 and Z80 (asserted in
halfphi's chips test from 0.1.3 on) and the 6502 workspace's full 117
tests, goldens required, re-proved green before the release was cut
(digest d1c6947ca960e9eb, tags `v0.1.3` there and `halfphi-v0.1.3` in
the 6502 repo). The 2C02's suite re-proves green on the 0.1.3 pin in
its own repository.

- **The supply-gated family recurs, bigger: 100 transistors** gated by
  the supply rail (2C02: 38, Z80: 32), permanently conducting in
  silicon, set so by `power_on` and proven load-bearing by the MUTATE
  red above. The 67 ground-gated are permanently off in both models.

## Throughput

**114,740 master half-steps/s** quiescent, best of three
(`examples/bench.rs`), buses undriven. Faster than the 2C02's 50,457
because a master half-step moves less of this chip: the ÷12 keeps most
of the core still per tick. Real time is 42.95 M master half-steps/s,
so rung 0 is 374x slow, which is what the ladder is for.

## Carried forward

- **Promotion of the supply-gated fix-up into halfphi is deferred,
  deliberately.** The sketch said "if it recurs, promote"; it recurs,
  but folding it into halfphi changes the Z80's recorded baseline (its
  32 supply-gated transistors are documented off there, and its
  convergence was measured with them off), so the promotion wants its
  own measured change across all five chips, not a rider on 0.1.3.
- A1/A2 (the die pages) come after the ladder, per the console sketch
  v0.2's decision record; the probes they would have hosted are N3
  deliverables.
- A3 (first sound: the AD1/AD2 taps against the wiki's mixer table) is
  next in this repository.
- The harness (memory, the 24-edge-style register protocol, DMA) and a
  golden through it arrive with the pin-lockstep gate against
  `v6502-pins` rung 3, the console sketch's "chip versus chip through
  the contract".
