# N3 plan: the 2A03 ladder, measured before it is built

The console sketch (nes-bus, `docs/nes-end-to-end-v0_2.md`, milestone
N3) asks for `v2a03-micro`: the 6502's rung 3 for the core with the
decimal adjust disconnected and verified against N1's divergence list,
plus the APU as tables measured out of rung 0 the way the 6502's decode
was, with the probes that measure them kept as deliverables. This
document is the plan, written before the rung; `docs/n3-report.md`
carries what each step measured. The order is the sketch's own:
measure first, author from the measurement, hold the authored thing to
the chip.

## Step 1: the divergence list (the pin-lockstep gate, second half)

N1 left the cross-chip comparison to "where both chips are reachable".
Both are reachable here without a 6502 engine: the 6502 repository
records its rung 0 at the pins as text (`tools/pin-golden/*.pins`, 274
traces, NC-SA-derived and gitignored there), and `v6502-pins` (MIT, no
die data) parses and replays it. So the second half is a test in this
repository that reads a sibling checkout's recordings (`PIN_GOLDEN=`
names another place; without them it SKIPS by name).

- `src/pins.rs`: the 2A03 core as a `v6502_pins::PinEngine`
  (`CorePins`), one frame per clk0 phase, driven by the contract's own
  `run` so a script cannot be applied differently to the two chips.
  The alignment between the two reset sequences is MEASURED
  (`examples/lockstep-probe.rs`) and then asserted at every power
  cycle.
- `src/lockstep.rs`: the comparison, every differing field classified
  by a rule with a name. Anything unnamed is loud and fails.
- Gate: every trace agrees pin for pin except the named classes, each
  bounded exactly (the stack offset derived from the two dies' own S
  registers, the decimal stores listed byte by byte from the program's
  operands, the reset window bounded by the script and the vector read,
  the write-phi1 bytes counted). `MUTATE=1` flips one serviced bit and
  must go red as an unnamed difference.

## Step 2: the core rung

`v2a03-micro` depends on `v6502-micro` (a git dependency on the 6502
repository, pinned by revision; its table is measured out of the 6502's
rung 0 at build time, NC-SA-derived, never committed) and configures it
with the two knobs step 1's list justifies: `set_decimal_adjust(false)`
and `set_stack_at_h0(Some(s))`, with `s` READ off this chip's rung 0
register nodes rather than typed. Nothing else in the core is authored
here.

- Gate: every recorded trace's program runs on this chip's rung 0 (via
  `CorePins`) and on the core rung under the same script; the two agree
  in every field except the write-phi1 byte (the class step 1 names;
  the core rung reproduces the 6502's pins there, and nothing crosses
  the pins in that half). The decimal chains agree outright: the
  binary bytes are the 2A03's. Throughput measured beside rung 0.
- `MUTATE=1` leaves the adjust connected: the decimal traces must go
  red by name.
- Anything the core rung does not reproduce of this die is listed by
  name with its bound and the measurement it waits on (the ladder's
  rule for undocumented behaviour), never masked.

## Step 3: the APU probes

Headless dumps off rung 0, each an example whose output is recorded in
the report before any table is authored from it:

- the frame sequencer: `frm_*` per CPU half-step over a full 4-step and
  a full 5-step sequence from a $4017 write, the phase pulses' positions
  in CPU cycles;
- one channel: timer (`sq0_t*`), period, duty step (`sq0_c*`), length
  (`sq0_len*`), envelope (`sq0_env*`), sweep (`sq0_swp*`), output;
- the length table: all 32 indices written to $4003, the loaded counter
  read off the nodes;
- the noise LFSR and its period table, the triangle's linear counter
  and 32-step sequence, the DMC's rate table and sample path;
- OUT0..2 and the controller strobes on $4016 writes and $4016/$4017
  reads;
- RDY during a $4014 write (`spr_dma_/rdy`, the address bus, the
  513/514-cycle question) and during a DMC fetch (`pcm_dma_/rdy`).

## Step 4: the APU as tables

`build.rs` runs rung 0 and writes the tables to `OUT_DIR` (NC-SA, never
committed): the length table, the noise and DMC period tables, the duty
sequences, the frame sequencer's step positions in both modes. The
datapath around them (timers, counters, envelope, sweep, the mixer
already authored in `src/mixer.rs`) is authored from the probes and
labelled.

- Gate: a register program that exercises every channel; the five
  output codes (the AD1/AD2 tap, `sq0_out`, `sq1_out`, `tri_out`,
  `noi_out`, `pcm_out`) bit-exact against rung 0 every CPU half-step;
  frame IRQ and DMC IRQ timing against rung 0; throughput measured.
  `MUTATE=1` drops a table entry and must go red.

## Step 5: the stalls

$4014 and the DMC fetch as RDY spans on the core rung, replayed against
rung 0 goldens that show which half-cycle each steal lands in.

## Real time

The 2A03's core runs at 1.789773 MHz (the master clock over 12), which
is 3.58 M half-cycles per second. The 6502's rung 3 measured 39.0 M
half-cycles/s, so the core alone is about 11x inside real time; the APU
tables add per-half-cycle work and the report records what is left.
The gate is stated the way P3's was: real time is a property, measured
with its margin, and nothing is built for speed until a shortfall is
recorded.
