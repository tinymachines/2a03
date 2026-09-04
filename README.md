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

The pin-lockstep gate's chip side is in place: the 2A03's 6502 core is
presented at the pins as a `v6502-pins` `PinFrame` (one per clk0 phase,
the 6502's own half-cycle) and held to what a 6502 must do there, the
reset vector fetched from $FFFC/$FFFD, execution entered at the vector,
opcode fetches marked by sync, a store landing as a write. It depends
on the `v6502-pins` contract alone (MIT, no die data), never on a 6502
engine: a chip crate does not know what is on the other side of its
pins. The cross-chip comparison against a recorded 6502 trace, and the
decimal-mode divergence, are the second half and belong to the console
layer where both chips are reachable.

A3 (first sound) is closed on top of A0: the memory harness runs the
authored square-note program on the chip, the reference's own run of
the same program replays through it bit-exact with no exemptions (core,
APU and bus glue under one comparison), and sq0_out swings 0 to 15 in
plateaus whose length derives from the program's own timer byte, ten of
them measured at exactly 144 half-steps. Mixed through the authored
nesdev table, the run emits nes-bus AudioSamples: silence and
ad1(15, 0), nothing else. MUTATE=1 serves the timer operand XOR 1 and
both gates go red, the replay at the byte's first bus crossing and the
plateau at the mutated arithmetic's own 160. `docs/a3-report.md` is the
account.

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
| `v2a03-sim` | Power-on and the reference's reset recipe, master half-stepping, the node dump the golden comparison rides on, the memory harness, and the authored mixer. |

## Commands

```bash
bash tools/fetch-netlist.sh          # Quietust's Visual 2A03, eight files,
                                     # sha256-pinned (never committed)
cargo test --workspace --release     # counts, convergence, the goldens,
                                     # the A3 harness replay and the note;
                                     # tests SKIP by name without the extern
                                     # or the golden; REQUIRE_NETLIST=1 /
                                     # REQUIRE_GOLDEN=1 insist
MUTATE=1 cargo test --workspace --release   # must go red three ways: the
                                     # supply-gated fix-up off (A0 replay
                                     # diverges at step 0), the timer byte
                                     # served wrong (A3 replay and plateau
                                     # both), and R/W's polarity flipped in
                                     # the extracted pin frame (the vector
                                     # fetch fails its read check)
cargo test --release -p v2a03-sim --test pin_lockstep
                                     # the pin-lockstep gate, chip side: the
                                     # core as a v6502-pins PinFrame per clk0
                                     # phase, held to a conformant 6502's
                                     # reset vector, sync and store
node tools/golden-trace/gen.js       # regenerate the A0 golden
                                     # (601 states, about 5 s)
node tools/golden-trace/gen-a3.js    # regenerate the A3 golden (2,001
                                     # states with memory, about 4 min)
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
