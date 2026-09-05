# N3 report: the 2A03 ladder

The plan is `docs/n3-plan.md`, written first. This report carries what
each step measured, in the order the steps closed.

## Step 1: the divergence list (closed 2026-09-05)

Run stamp: rustc 1.97.1, halfphi v0.1.6, the 6502 repository's pin
golden as recorded by `v6502-sim 0.1.0 nodes 1725 transistors 3510`
(274 traces). `REQUIRE_NETLIST=1 REQUIRE_PINS=1 cargo test --release -p
v2a03-sim --test pin_lockstep`: both halves green in about 12 s.
`MUTATE=1`: the chip side red (R/W flipped in the frame, the vector
fetch fails its read check), the cross-chip side red at the first
serviced byte it touches (`decimal-adc h=30: the differing byte is not
a write's phi2`).

### Alignment, measured

The contract's h=0 is the frame the reset sequence leaves behind, which
on the 6502's rung 0 is the first opcode fetch (its `power_cycle` runs
the vector reads before anyone looks). The 2A03's `power_on` (the
reference's initChip recipe) leaves the core earlier: `lockstep-probe`
printed both streams beside each other and the first sync-high fetch of
the reset vector sat at the 2A03's phase 17 on every trace tried. That
number is `pins::ALIGN_PHASES`, and `power_cycle` asserts the frame it
lands on is a sync-high read of the reset vector, so a changed reset
recipe fails by name instead of shifting every comparison.

### The result

**272 traces compared, 2 refused by name, 130 exact in every field at
every half-cycle.** The other 142 differ only inside four named classes,
each bounded by a rule the test asserts:

| class | frames | rule |
|---|---|---|
| stack | 2,088 | the address differs, both in page 1, by exactly $40 in the low byte |
| write-phi1 | 453 | the data byte differs on the phi1 half of a write cycle |
| s-leak | 2 | the same $40 in page 0, in `op-ba` only |
| data | 17 | a serviced byte differs: the nine decimal stores, and eight inside the reset window |
| ab, rw | 14, 7 | inside the reset window only |

**The stack page.** Silicon leaves S undefined at power-on. The 6502's
simulation settles it at $00 and the 2A03's at $C0 (both bit-exact
against their own references from state 0, so each is its die data's
answer), and after the reset's three decrements one core pushes at $01FD
and the other at $01BD. The test reads S off the 2A03's register nodes
at h=0 ($BD) and off the recording's first push ($FD), derives the
offset ($40), and requires every page-1 difference to be exactly that
and nothing else in the frame. Two traces write S themselves (`op-9a`
TXS, `op-9b` SHS) and their stack pages agree outright, listed. One
trace leaks S off the page: `op-ba` is TSX, whose following `$34 $12`
bytes run as `NOP zp,X` and index page 0 by the copied S, two phases at
$000F against $00CF, allowed for that trace alone.

**The write's phi1.** A write is serviced as clk0 rises and a read as it
falls; in a write cycle's phi1 half nothing is serviced and the world
drives nothing. There the 6502's pins show the last byte read (which is
what its rungs reproduce) and the 2A03's show something else: INC's
final write shows the new byte ($41 against the 6502's $40), a DEC shows
$55 where $FF is about to land, BRK's pushes show $07 and $15 before $05
and $32. Not investigated past the classification: every write's own
phi2 byte is compared as data and agrees on all 272 traces. The class is
counted and must stay nonzero, so its disappearance would be a finding
too.

**Decimal.** The 2A03 die has the decimal adjust disconnected, and the
three decimal chains show exactly that and nothing more. Every
differing serviced byte is a write's phi2, listed in the test with the
arithmetic beside it:

| trace | address | 6502 (adjusted) | 2A03 (binary) |
|---|---|---|---|
| decimal-adc | $0080 | $47 | $41 (19+28) |
| | $0081 | $10 | $0A (09+01) |
| | $0082 | $99 | $33 (99+99+1) |
| | $0083 | $00 | $A0 (50+50) |
| | $01FA (PHP) | $FD | $FC (C clear: $A0 carries nothing; V stays) |
| decimal-sbc | $0080 | $29 | $2F (42-13) |
| | $0081 | $05 | $0B (10-05) |
| | $0082 | $99 | $FF (00-01) |
| decimal-mixed | $0080 | $26 | $20 (1F+01) |

The 9A-00 subtraction, the CMP under D and the add after CLD already
agreed. The flags are binary too: the one PHP that differs is the carry
the decimal adjust would have produced.

**Reset mid-run.** With RES driven low at h=20 for eight phases (the
recorded script), the 6502 runs its in-flight BRK on with the vector
select turning to $FFFC one cycle later, freewheels, and reads the vector
at h=42. The 2A03 holds its core still under RES (address bus, R/W and
data frozen at the interrupted push, h=22 to 28), then on release
freewheels two phases at a garbage address, fetches with sync, performs
the reset's three stack reads, and reads the vector at h=42: the same
half-cycle. `reset-probe` measured the hold at 8, 12, 16, 24, 48 and 96
phases: the vector read lands at 42 + (hold - 8) every time, so the
post-release sequence is 13 phases on both dies and the difference is
entirely the held span. The test bounds the window to (first RES-low
frame, vector read - 6), requires the vector read at the same h, and
requires the window to be nonempty.

**Refused by name.** `fixture-rdy-stall` drives RDY and
`fixture-so-pulse` drives SO. Neither is a 2A03 pin: RDY is an internal
node the DMA units own (`spr_dma_/rdy`, `pcm_dma_/rdy`) and SO an
unbonded pad the reference holds low. `CorePins` records the request and
the test refuses the comparison rather than reading a coincidence.

### What this decides for step 2

The core rung is `v6502-micro` with two knobs, both now in the 6502
repository (commit `dc2dada`, `tests/knobs.rs`): the decimal adjust
disconnected (the selector never sees D, so the binary spans play with
binary flags; held there to produce exactly the table above), and the
stack pointer at h=0 seeded with the value this chip's rung 0 measures.
The write-phi1 byte is the one thing the core rung will not reproduce
of this die, and step 2's gate names it the same way.

## Step 2: the core rung (closed 2026-09-05, one item carried)

Run stamp: rustc 1.97.1, halfphi v0.1.6, `v6502-micro` and `v6502-pins`
at the 6502 repository's `dc2dada`. `REQUIRE_NETLIST=1 REQUIRE_PINS=1
cargo test --release -p v2a03-micro --test core`: green in about 9 s
after the build (building `v2a03-micro` the first time checks out the
6502 repository at that revision, builds its netlist and runs its rung 0
over 256 opcodes to record rung 3's table, about 80 s; the table is
NC-SA-derived and stays in `OUT_DIR`). `MUTATE=1` leaves the decimal
adjust connected: all three decimal chains go red at their first store
(h=21) and the test says so by name.

### What closed

- **`crates/v2a03-micro`**: the core is `v6502-micro` configured by the
  two knobs step 1 justified, `set_decimal_adjust(false)` and
  `set_stack_at_h0(Some(s))`, with `s` read off this chip's rung 0
  register nodes at h=0 ($BD) in the test, never typed. Nothing else
  about the core is authored here.
- **Gate**: every recorded 6502 program and script (272; the two that
  drive RDY or SO refused as in step 1) runs on this chip's rung 0
  through `CorePins` and on the core rung under the same `run`.
  **272 programs, 52,048 half-cycles compared, 133 exact in every
  field; 457 write-phi1 bytes differ and nothing else does.** The
  decimal chains agree outright: the binary bytes of step 1's table are
  now the core rung's bytes too.
- **Throughput** (`examples/bench.rs`, the reference's program):

  | engine | half-cycles/s |
  |---|---|
  | rung 0, switch level through the memory harness | 9,796 |
  | core rung | 32,807,382 (best of 3) |

  About 3,349x rung 0 and **9.2x the 2A03's real time** (3,579,545
  half-cycles/s, the master clock over 12, two half-cycles per cycle).
  The 6502's rung 3 measured 39.0 M on its own bench; the difference is
  the program and the seeded machine, not the knobs, and is inside what
  the APU tables will spend.

### Carried

- **The reset hold.** Rung 3 plays the 6502's response to RES: the
  in-flight BRK runs on, the vector select turns, the datapath
  freewheels. This die holds its core still while RES is low (step 1's
  measurement) and then plays the same 13-phase release sequence from
  the frozen state, so the addresses inside the window differ
  (`eae9`/`00e9` against the 6502's `5801`/`0057`/`00ff`). The gate
  lists `fixture-reset-mid-run` as diverging by name, bounds the
  difference to the reset window, requires the vector read at the same
  half-cycle on both, and fails if the trace ever replays clean. To
  close it: a probe on this chip's rung 0 (the 6502's `reset-probe`
  shape: control lines and latches per half-cycle under RES) to see
  what the held core resumes from, then a third knob on `v6502-micro`
  authored from that, held to this trace. A power-on reset is
  unaffected (the console asserts RES before h=0).
- **The write-phi1 byte** stays the one thing about this die's pins the
  core rung does not reproduce, as step 1 recorded.

## Step 3: the APU probes (closed 2026-09-05)

Every number below was read off rung 0 by an example that stays in the
tree as a deliverable (`crates/v2a03-sim/examples/apu-*-probe.rs`);
nothing here is quoted from a document. Where a published table exists
it is compared afterwards, and one entry disagrees, named below.
Positions are in CPU cycles after the register write's own bus cycle
(the `w40xx` strobe) unless stated; the chip's unit is the half-step.

**The frame sequencer** (`apu-frame-probe`, modes 0, 1 and no write).
The $4017 write restarts the 15-bit LFSR (`frm_t0..14`, $7FE0 at the
strobe in both modes). 4-step: `phase_a..d` at 7458.5, 14914.5, 22372.5,
29830.5, then every 29830; `frame_irq` rises with `phase_d`. 5-step:
`phase_a..c` at the same three positions, no `phase_d`, `phase_e` at
37282.5, period 37282, no IRQ. Without any $4017 write the counter runs
from power-on's reset with the same spacing, `phase_a` 7458 cycles after
the first half-step `power_on` leaves (the pin contract's h=0 is 17
phases later). Envelope and sweep updates land on these clocks: the
envelope's decay steps 7458/7456 apart alternating (quarter frames), the
sweep's period changes 14916 apart (half frames).

**The length table** (`apu-length-probe`): $4003 written with each of
the 32 indices, `sq0_len0..7` read six half-steps after the strobe. By
index: 9, 253, 19, 1, 39, 3, 79, 5, 159, 7, 59, 9, 13, 11, 25, 13, 11,
15, 23, 17, 47, 19, 95, 21, 191, 23, 71, 25, 15, 27, 31, 29. Every
entry is the published table's value minus one: the die's counter holds
n-1 and a channel plays for n half-frame clocks.

**The squares** (`apu-channel-probe duty|env|sweep`). The duty
sequencer `sq0_c` counts down 7..0, one step per 2(t+1) CPU cycles (36
half-steps at t=8), and `sq0_out` is 15 on steps {6}, {6,5}, {6,5,4,3}
and {7,4,3,2,1,0} for duty 0..3: the published 12.5/25/50/75 percent
sequences. Envelope (period 0, no loop): `sq0_envc` 0 at the $4003
write, 15 at the first quarter frame (+7396 here; the start flag's
reload), then one step down per quarter frame to 0. Sweep (timer $100,
shift 1, period 0): positive on both squares $100, $180, $240 at the
half frames; negative $100, $07F, $03F on square 0 (the ones'
complement) and $100, $080, $040 on square 1 (the two's complement),
the two squares' documented difference, measured.

**The triangle** (`tri`): control set, linear reload 127, timer 8. The
first step lands 7396 cycles after the $400B write, at the first quarter
frame, because the linear counter loads there and the sequencer needs
it nonzero. Then the 32-step sequence 15..0, 0..15 at one step per t+1
cycles (18 half-steps), the doubled 0 and 15 showing as 36.

**The noise** (`noise`). Period table by $400E index, as half-steps
between LFSR shifts over two, in CPU cycles: 4, 8, 16, 32, 64, 96, 128,
160, 202, 254, 380, 508, **964**, 1016, 2034, 4068. Fifteen entries
agree with every published NTSC table; **index 12 measures 964 where
the published value is 762.** The noise timer `noi_t0..10` is not a
loaded countdown (its values between shifts range up to $7FF at every
index), which reads as an LFSR-shaped timer whose reset pattern is a
ROM row per index; under that structure one wrong bit in a row moves
that entry arbitrarily, so this is either a Visual 2A03 transcription
defect in that row or a real quirk of the die, and only silicon can
say. The table used from here is the die's, the entry named. The LFSR
`noi_c0..14` shifts toward `c14` with `c0` fed by the feedback; the
feedback is `c13 xor c14` in mode 0 and `c8 xor c14` in mode 1 (the
published bit-1 and bit-6 taps with the register numbered the other
way), both read off 24 consecutive states. Power-on leaves it
`011111111111111` (c0 leftmost), and the first shift after enabling
waits 8063 half-steps (the timer running out its power-on state).

**The DMC** (`dmc`). Rate table by $4010 index, in CPU cycles between
shifts: 428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128,
106, 84, 72, 54, all sixteen the published values. Enabling with an
empty buffer fetches $C000 19 cycles after the $4015 write, RDY low 5
half-steps; the next fetch ($C001, after eight shifts) holds RDY low 7
half-steps. The output counter steps by 2 per bit: $40, $42, $40, $42
under a %10100101 sample.

**Sprite DMA** (`apu-dma-probe`): $4014 written at two CPU-cycle
parities. RDY falls at the write strobe; after one or two cycles at the
next instruction's address the DMA runs 256 read/write pairs (a read of
$02xx on phi1, a write of the same byte to $2004 in the next cycle,
`spr_addr` counting), and RDY returns after **1029 or 1027 half-steps
(514.5 or 513.5 CPU cycles)**, the published 513/514 plus the write's
own half. `RnWstretched` rises for the duration.

**The controller port** (`io`): $4016 bit 0..2 land on OUT0..2 at the
write; a read of $4016 pulls `joy1` (and `/r4016`) low for the read's
two half-steps, $4017 pulls `joy2` (`/r4017`). The chip drives no data
on those reads: the controller bits are the board's (N4's 74LS368s).

### What this decides for step 4

The tables to measure at build time: the length table, the noise and
DMC period tables, the duty sequences, the frame sequencer positions in
both modes. Everything else (timers, the envelope, the sweep with its
two complements, the linear counter, the LFSR taps, the DMC's delta
counter and fetch) is authored from the probes above and labelled. The
gate stays the plan's: the five output codes against rung 0 every CPU
half-step, on a program that exercises every channel.
