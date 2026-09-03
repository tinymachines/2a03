# 2a03

A switch-level Ricoh 2A03 (the NES CPU: a 6502 core, the ÷12 clock
divider, the APU, the DMA and controller strobes), the way the family
does everything: the netlist in, the behaviour out. The fifth chip
through [halfphi](https://github.com/tinymachines/halfphi)'s identical
calls, beside [tinymachines/6502](https://github.com/tinymachines/6502),
[tinymachines/2c02](https://github.com/tinymachines/2c02) and
[tinymachines/ntsc-crt](https://github.com/tinymachines/ntsc-crt). The
plan is the console sketch's milestone N1
([nes-bus](https://github.com/tinymachines/nes-bus)`/docs/nes-end-to-end-v0_2.md`);
the milestone log starts at `docs/a0-report.md`.

## Status

A0 (the netlist loads, settles, and replays the reference) is closed:
**10,946 transistors over 5,577 defined nodes agreed by both real
parsers**, power-on converging unaided, and the reference simulator's
own trace replaying **bit-exact on every node across 601 states with no
exemption list at all**, the first chip in the family to replay with
none. Two findings, both measured before anything was authored:

- **The supply-gated family recurs, bigger**: 100 transistors gated by
  the supply rail (the 2C02 had 38), permanently conducting in silicon,
  set so by `power_on` and proven load-bearing by mutation.
- **The 2A03 is the chip that finally exercised halfphi's drive
  order.** Its SO input chain forms three contested groups at power-on
  (a layout pullup grouped with the init-driven-low pin), the first
  nonzero `contested_groups` in the family's history; the old
  `PullDown < PullUp` order resolved them high where the reference
  resolves low, eight nodes inverted for the whole trace. halfphi
  0.1.3 swaps the order, the external drive beating the depletion
  load; the swap is proven unobservable on the 6502, 6800, Z80 and
  2C02 (contested is 0 on all four, asserted in halfphi's chips test).

The unit is the MASTER half-step (one `clk_in` toggle at 21.477272
MHz); the ÷12 `clk0` is an output the divider produces. Quiescent
throughput: **114,740 master half-steps/s** (buses undriven; the ÷12
means most of the core stands still per master tick).

| Crate | Role |
|---|---|
| `v2a03-netlist` | The die data parsed by halfphi at build time and embedded; builds data-free with a loud refusal when the extern is not fetched. |
| `v2a03-sim` | Power-on and the reference's reset recipe, master half-stepping, and the node dump the golden comparison rides on. |

## Commands

```bash
bash tools/fetch-netlist.sh          # Quietust's Visual 2A03, eight files,
                                     # sha256-pinned (never committed)
cargo test --workspace --release     # counts, convergence, the golden;
                                     # tests SKIP by name without the extern
                                     # or the golden; REQUIRE_NETLIST=1 /
                                     # REQUIRE_GOLDEN=1 insist
MUTATE=1 cargo test --workspace --release   # must go red: the supply-gated
                                     # fix-up off, the replay diverges at
                                     # step 0
node tools/golden-trace/gen.js       # regenerate the golden
                                     # (601 states, about 5 s)
cargo run --release -p v2a03-sim --example bench             # quiescent throughput
cargo run --release -p v2a03-sim --example a0-diverge-probe  # the measurement the
                                     # drive-order decision was made from
cargo run --release -p v2a03-sim --example contested-probe   # the three contested
                                     # groups, counted
```

## Licensing

The code is MIT. The die data (`extern/visual2a03/`, fetched, never
committed) is Quietust's Visual 2A03, derived from the visual6502
team's CC BY-NC-SA imagery; see `NOTICE.md`.
