#!/usr/bin/env python3
"""The five APU output codes over the gate's long-note world, as a figure.

Reads the CSV `cargo run --release -p v2a03-micro --example apu-codes`
prints (h, sq0, sq1, tri, noi, pcm, frame_irq per CPU half-step) and
draws the five code streams stacked on one time axis in milliseconds of
2A03 time (one half-step is 1 / 3,579,545 s). The streams are the rung's,
which the gate held identical to the switch-level chip's at every
half-step, so the picture is what the chip's own output nodes did.

    cargo run --release -p v2a03-micro --example apu-codes -- 80000 > codes.csv
    python3 tools/apu-figure.py codes.csv apu-codes.png
"""
import csv
import sys

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

HALF_STEP_S = 1.0 / 3_579_545.0


def main() -> int:
    src, out = sys.argv[1], sys.argv[2]
    rows = list(csv.DictReader(open(src)))
    t = [int(r["h"]) * HALF_STEP_S * 1000.0 for r in rows]
    names = [("sq0", "square 0: duty 2, envelope looping, sweep negating", 15),
             ("sq1", "square 1: duty 1, constant 15, sweep growing to its mute", 15),
             ("tri", "triangle: linear counter held, timer $050", 15),
             ("noi", "noise: envelope, period index 4", 15),
             ("pcm", "DMC: a 33-byte sample looping at rate 15", 127)]
    fig, axes = plt.subplots(len(names), 1, figsize=(11, 8.5), sharex=True)
    fig.patch.set_facecolor("#0e0f13")
    for ax, (key, label, top) in zip(axes, names):
        ax.set_facecolor("#0e0f13")
        ax.step(t, [int(r[key]) for r in rows], where="post", color="#e8c170", linewidth=0.6)
        ax.set_ylim(-1, top + 1)
        ax.set_ylabel(key, color="#cfd3dc")
        ax.set_title(label, color="#cfd3dc", fontsize=9, loc="left", pad=2)
        ax.tick_params(colors="#9aa0ad", labelsize=8)
        for s in ax.spines.values():
            s.set_color("#2a2d36")
    axes[-1].set_xlabel("2A03 time, ms (80,000 CPU half-steps)", color="#cfd3dc")
    irq = next((float(tt) for tt, r in zip(t, rows) if r["frame_irq"] == "1"), None)
    if irq is not None:
        for ax in axes:
            ax.axvline(irq, color="#7aa2f7", linewidth=0.6, alpha=0.7)
        axes[0].annotate("frame IRQ", (irq, 15), color="#7aa2f7", fontsize=8, xytext=(4, -2), textcoords="offset points")
    fig.suptitle("The 2A03's five output codes, the authored APU held to the switch-level chip at every half-step", color="#e6e8ee", fontsize=11)
    fig.tight_layout()
    fig.savefig(out, dpi=110, facecolor=fig.get_facecolor())
    print(f"wrote {out}: {len(rows)} half-steps")
    return 0


if __name__ == "__main__":
    sys.exit(main())
