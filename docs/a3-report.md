# A3 report: first sound, and the note is exactly the program's

Run stamp: 2026-09-03, rustc 1.97.1, halfphi v0.1.3. `cargo test
--release --workspace` with `REQUIRE_NETLIST=1 REQUIRE_GOLDEN=1`: 5
tests green (A0's three plus A3's two). `MUTATE=1` sends both A3 gates
red, each in its own voice: the golden replay diverges at half-step 43,
the moment the tampered byte first crosses the data bus, and the
plateau measurement reads 160 half-steps, which is the mutated timer's
own arithmetic (2 x 2 x (9+1) x 4) disagreeing with the honest
program's 144.

## What closed

- **The memory harness** (`src/harness.rs`): the CPU bus serviced
  statement for statement from the reference's macros.js (halfStep
  spins the master clock until clk0 flips; reads drive the data bus by
  flipping its pulls and settling once with all eight as seeds; writes
  capture on the rising half; unwritten memory reads zero). The
  mutation hook serves one address XOR a mask, the CHR-byte pattern
  from the 2C02's P1.
- **The A3 golden**: the reference simulator itself running the
  authored square-note program with memory (`gen-a3.js`), 2,001 states
  over every node at CPU half-step granularity, replayed through the
  harness **bit-exact with no exemption list**: the 6502 core, the
  whole APU and the bus glue under one comparison.
- **The program is authored once** (`tools/golden-trace/program-a3.json`),
  read by the JS generator and the Rust tests from the same file:
  enable square 0, duty 50 percent, constant volume 15, length halted,
  timer 8, spin.
- **The note is measured, not assumed**: `sq0_out`'s four bits sampled
  every half-step swing exactly 0 to 15 in plateaus of exactly 144
  half-steps, ten of them measured, where 144 derives from the
  program's own timer byte (the sequencer steps every 2 x (t+1) CPU
  cycles, four steps high of eight at this duty). The netlist
  proposed; the measurement disposed; they agree to the half-step.
- **First sound as a value**: the run mixed through the authored
  nesdev table (`src/mixer.rs`) into nes-bus's `AudioSamples`, one
  sample per CPU half-step at the exact rational rate. AD1 is
  two-valued, silence and ad1(15, 0); AD2 stays silent.

## Authored, and labelled as such

The mixer constants are the nesdev wiki's APU Mixer page (read
2026-09-03), the same provenance level as nes-bus's pin tables before
N0's gates: a dated claim, not a measurement from this repository.
Measuring the real AD1/AD2 pins against the table is bench work
already on the console sketch's capture list, and the transcribed
levels of the 2C02's video DAC are the precedent for what that
measurement looks like when it lands.

## Carried forward

- One channel is sung; triangle, noise and DMC have registers the
  program never touched, held at power-on state by the golden but
  never exercised. A richer stimulus (and the frame counter's four
  modes) belongs to the pin-lockstep milestone's suite rather than to
  first sound.
- The plateau arithmetic pins the sequencer's period; the duty
  PATTERN's phase (which four of the eight steps are high) is covered
  only implicitly by the node golden. A duty sweep (all four settings)
  is a natural A3 follow-up when something consumes the waveform.
