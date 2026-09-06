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

**N3 is closed** (`docs/n3-plan.md`, `docs/n3-report.md`, five steps).
The chip is `v2a03-micro`: the 6502's rung 3 as the core with the
decimal adjust disconnected and the stack pointer seeded, the APU
authored around tables measured out of this chip's rung 0 at build
time, and the two DMA units authored from frame measurements, the
whole presented at the pin contract by `Rung`, which is what a console
attaches to. Step 5 (the stalls) closed last: the sprite DMA at both
write alignments and the DMC's sample fetches against rung 0 **frame
for frame in every field, RDY included** (1,201 + 1,201 + 4,201
frames; RDY low on 1,029, 1,027 and 19). `MUTATE=1` shortens the DMA by
a pair and the gate goes red. About 9x real time with everything
attached. Carried: the reset hold (step 2), `$4015` reads, a DMC fetch
inside a sprite DMA.

N3 step 4 (the APU as tables) is closed (`docs/n3-report.md`):
`v2a03-micro` carries the APU authored around tables measured out of
rung 0 at build time (the length table, the duty sequences, the frame
sequencer's positions in both modes, and the two timers step 4 found to
be LFSRs rather than counters, `noi_t` and `pcm_t`, with their taps,
terminals and sixteen reload states each: the die's period ROMs, which
is where the noise table's index 12 comes from). Held to rung 0 on two
register programs under both frame modes, then (2026-09-06, the
console's apu_test results the finder) the mode written on the other APU
cycle parity, the mode written after the notes, and no mode write at
all: **ten worlds of 80,001 half-steps, the five output codes and the
frame IRQ flag identical at every half-step**, envelopes, both sweep
complements to their mutes, length and linear expiries, the noise LFSR
and the DMC's byte cycle, loop and end, the $4017 write's jitter and
its mode-1 immediate clock included. Every timing inside a unit is a fitted constant
in `apu::fit`, each measured with a probe when the gate's code streams
first parted. **32.1 M half-cycles/s with the APU attached, 9.0x real
time.** `MUTATE=1` reverses the duty table at build time and the gate
goes red. Carried: the stalls (step 5) and the reset hold (step 2).

N3 step 3 (the APU probes) is closed (`docs/n3-report.md`): eleven
headless measurements off rung 0, kept as examples, are the instruments
step 4's tables and datapath are authored from: the frame sequencer's
positions in both modes and at power-on, the die's 32-entry length
table (every entry the published value minus one), the four duty
sequences, the envelope and sweep clocks (the two squares' negate
arithmetic differ, as published, and measured), the triangle's 32
steps, the noise period table (**index 12 measures 964 cycles where
every published table says 762**, the one disagreement, named), the
LFSR's two taps, the DMC's rate table and fetch stalls, the sprite DMA's
513.5/514.5-cycle stall by alignment, and the controller strobes.

N3 step 2 (the core rung) is closed with one item carried
(`docs/n3-report.md`): `v2a03-micro` presents the 6502's rung 3 as this
chip's core, configured by exactly the two knobs the divergence list
justified (the decimal adjust disconnected, the stack pointer seeded
from this chip's own rung 0 nodes), nothing else authored. Every
recorded 6502 program and script runs on rung 0 and on the core rung
under the same replay: **272 programs, 52,048 half-cycles, 133 exact in
every field, 457 write-phi1 bytes differing and nothing else**, the
decimal chains agreeing outright. **32.8 M half-cycles/s, about 3,349x
rung 0 and 9.2x the 2A03's real time.** `MUTATE=1` leaves the adjust
connected and the three decimal chains go red by name. Carried: the
2A03 holds its core still under a mid-run RES where the 6502 runs on;
the rung plays the 6502's, the gate lists that one script as diverging
inside the reset window, and the hold is to be measured and authored as
a third knob.

N3 step 1 (the divergence list) is closed (`docs/n3-plan.md`,
`docs/n3-report.md`): the pin-lockstep gate's second half, chip against
chip. The 2A03's core runs every one of the 6502 repository's 274
recorded pin traces through the contract's own replay (`v6502-pins`
alone; the other chip enters as recorded text, never as an engine), and
**272 traces compare, 130 exact in every field at every half-cycle, the
rest differing only inside four named and bounded classes**: the stack
page (the two dies' simulated power-on stack pointers differ by $40,
derived from both cores' own registers), the data byte in a write's
phi1 half (nothing is serviced there), the three decimal chains (the
2A03 stores the binary sums and binary flags where the 6502 adjusts,
nine bytes listed with their arithmetic), and the reset-mid-run script
(the 2A03 holds its core still under RES where the 6502 runs on; both
read the vector at the same half-cycle). Two scripts drive pins the
2A03 does not have and are refused by name. `MUTATE=1` flips one
serviced bit and goes red as an unnamed difference. The list decided the
core rung's shape: `v6502-micro` with its decimal adjust disconnected
and its stack pointer seeded from this chip, both knobs landed in the
6502 repository and held to its golden there.

The pin-lockstep gate's chip side (N1) is in place: the 2A03's 6502 core
is presented at the pins as a `v6502-pins` `PinFrame` (one per clk0
phase, the 6502's own half-cycle) and held to what a 6502 must do there,
the reset vector fetched from $FFFC/$FFFD, execution entered at the
vector, opcode fetches marked by sync, a store landing as a write.

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
| `v2a03-micro` | The ladder rung: `v6502-micro` (a git dependency pinned by revision; its table measured out of the 6502's rung 0 at build time) as this chip's core, the APU authored around tables measured out of this chip's rung 0 at build time (`build.rs`, about a minute), and `Rung`, the whole chip at the pin contract with its DMA units. |
| `v2a03-sim` | Power-on and the reference's reset recipe, master half-stepping, the node dump the golden comparison rides on, the memory harness, the core at the `v6502-pins` contract with the cross-chip classifier, and the authored mixer. |

## Commands

```bash
bash tools/fetch-netlist.sh          # Quietust's Visual 2A03, eight files,
                                     # sha256-pinned (never committed)
cargo test --workspace --release     # counts, convergence, the goldens,
                                     # the A3 harness replay and the note;
                                     # tests SKIP by name without the extern
                                     # or the golden; REQUIRE_NETLIST=1 /
                                     # REQUIRE_GOLDEN=1 insist
MUTATE=1 cargo test --workspace --release   # must go red seven ways: the
                                     # supply-gated fix-up off (A0 replay
                                     # diverges at step 0), the timer byte
                                     # served wrong (A3 replay and plateau
                                     # both), R/W's polarity flipped in the
                                     # extracted pin frame (the vector fetch
                                     # fails its read check), one serviced bit
                                     # flipped in the cross-chip replay, the
                                     # core rung's decimal adjust left on, the
                                     # APU's duty table reversed, and the sprite
                                     # DMA a pair short
REQUIRE_PINS=1 cargo test --release -p v2a03-sim --test pin_lockstep
                                     # the pin-lockstep gate, both halves: the
                                     # core as a v6502-pins PinFrame per clk0
                                     # phase held to a conformant 6502, then
                                     # every recorded 6502 trace replayed
                                     # through it with the four divergences
                                     # named (reads ../6502/tools/pin-golden,
                                     # or PIN_GOLDEN=<dir>; SKIPS without)
REQUIRE_PINS=1 cargo test --release -p v2a03-micro --test core
                                     # N3 step 2: the core rung against rung 0
                                     # on every recorded program and script,
                                     # every field but the write-phi1 byte;
                                     # MUTATE=1 reconnects the decimal adjust
                                     # and the three decimal chains go red
REQUIRE_NETLIST=1 cargo test --release -p v2a03-micro --test stalls
                                     # N3 step 5: the whole chip at the pins
                                     # (Rung) against rung 0 frame for frame
                                     # through a sprite DMA at both alignments
                                     # and the DMC's fetches; MUTATE=1 drops a
                                     # DMA pair
cargo run --release -p v2a03-sim --example stall-probe -- dma   # the two stalls as
                                     # rung 0 shows them (dma | dmc)
cargo run --release -p v2a03-micro --example rung-trace -- 8 24  # the rung's frame
                                     # beside its core's, under the DMA
REQUIRE_NETLIST=1 cargo test --release -p v2a03-micro --test apu
                                     # N3 step 4: the APU's five output codes
                                     # and the frame IRQ against rung 0 every
                                     # half-step, ten worlds (two programs,
                                     # both frame modes, both write parities,
                                     # the mode written last, no mode write);
                                     # APU_DUMP=1 prints both streams around
                                     # the first divergence, APU_DUMP=all the
                                     # whole run, APU_PROG=1 each program's
                                     # bytes; MUTATE=1 reverses the duty table
                                     # (rebuilds) and swaps the write parity
cargo run --release -p v2a03-sim --example apu-write-probe -- [0|1] [0|1]
                                     # the $4017 write at both APU parities:
                                     # the reset, the mode-1 immediate clock,
                                     # every phase and the IRQ flag as
                                     # half-steps after the strobe (DELAY=n
                                     # NOPs first). The jitter's measurement
PROG=<hex> FROM=<h> TO=<h> cargo run --release -p v2a03-sim --example apu-world-probe
                                     # a register program on rung 0: every
                                     # change of a square's nodes in a window
                                     # (PINS=1 through the pin engine, as the
                                     # gate sets the chip up)
cargo run --release -p v2a03-micro --example bench   # the core rung, alone and
                                     # with the APU, beside rung 0
cargo run --release -p v2a03-micro --example apu-trace -- 100 140
                                     # the authored APU's state per half-step
cargo run --release -p v2a03-micro --example apu-codes -- 80000 > codes.csv
python3 tools/apu-figure.py codes.csv apu-codes.png
                                     # the five output codes over the gate's
                                     # long-note world, as a figure
cargo run --release -p v2a03-sim --example apu-frame-probe -- 0     # the frame
                                     # sequencer: 0 = 4-step, 1 = 5-step,
                                     # 2 = no $4017 write (power-on position)
cargo run --release -p v2a03-sim --example apu-length-probe # the 32-entry length table
cargo run --release -p v2a03-sim --example apu-dma-probe    # $4014: RDY, the 256 pairs,
                                     # the stall at two alignments
cargo run --release -p v2a03-sim --example apu-channel-probe -- duty env sweep tri noise dmc io seq dmcseq lfsr triseq
                                     # the channels: duty sequences, envelope,
                                     # sweep, triangle, noise table and taps,
                                     # DMC table and fetch, controller strobes,
                                     # then the per-half-step traces step 4's
                                     # constants were fitted from (a square's
                                     # timer, the DMC's output unit, the two
                                     # LFSR timers, the triangle's timer)
cargo run --release -p v2a03-sim --example lockstep-probe   # every trace, every
                                     # differing field classified: the
                                     # measurement the gate's rules are from
cargo run --release -p v2a03-sim --example nmi-latency-probe
                                     # the NMI pad driven low at every master
                                     # pulse across a NOP; where the vector
                                     # read lands: one pulse of setup before
                                     # the final phi1 begins, nothing more
                                     # (the console's gate 1, N5)
cargo run --release -p v2a03-sim --example reset-probe      # RES held 8..96 phases
                                     # mid-run: the 2A03's held core beside
                                     # the 6502's recording
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
