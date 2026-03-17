#!/usr/bin/env python3
"""
Generate paper-ready figures from H1/H2/H4 benchmark CSVs.
Outputs PNG files to results/figures/.

Usage: python3 scripts/generate_plots.py
"""
import csv
import os
import sys

try:
    import matplotlib.pyplot as plt
    import matplotlib.ticker as ticker
    import numpy as np
except ImportError:
    print("ERROR: matplotlib and numpy required. Run: pip3 install matplotlib numpy")
    sys.exit(1)

os.makedirs("results/figures", exist_ok=True)

STYLE = {
    "figure.figsize": (5, 3.5),
    "axes.spines.top": False,
    "axes.spines.right": False,
    "font.size": 10,
    "axes.labelsize": 10,
    "legend.fontsize": 9,
    "xtick.labelsize": 9,
    "ytick.labelsize": 9,
}
plt.rcParams.update(STYLE)


def read_csv(path):
    if not os.path.exists(path):
        print(f"WARNING: {path} not found — skipping figure")
        return []
    with open(path) as f:
        return list(csv.DictReader(f))


# ── Figure 1: H1 latency breakdown (stacked bar) ──────────────────────────────
def plot_h1_breakdown():
    rows = read_csv("results/h1_summary.csv")
    if not rows:
        return

    # Use event_rate=100 slice; fall back to first 3 rows if not present
    filtered = [r for r in rows if r["event_rate_per_sec"] == "100"]
    rows = filtered if filtered else rows[:3]

    labels = [f"{int(r['dataset_size_quads'])//1000}K" for r in rows]
    write  = [float(r["write_mean_ms"])      for r in rows]
    hist   = [float(r["hist_mean_ms"])        for r in rows]
    live   = [float(r["live_mean_ms"])        for r in rows]
    comp   = [float(r["comparator_mean_ms"])  for r in rows]

    x = np.arange(len(labels))
    width = 0.5
    colors = ["#4C9BE8", "#E87B4C", "#4CE87B", "#B44CE8"]

    fig, ax = plt.subplots()
    bottoms_hist  = [w for w in write]
    bottoms_live  = [w + h for w, h in zip(write, hist)]
    bottoms_comp  = [w + h + l for w, h, l in zip(write, hist, live)]

    ax.bar(x, write, width, label="Storage write",        color=colors[0])
    ax.bar(x, hist,  width, bottom=bottoms_hist,  label="Historical retrieval", color=colors[1])
    ax.bar(x, live,  width, bottom=bottoms_live,  label="Live window",          color=colors[2])
    ax.bar(x, comp,  width, bottom=bottoms_comp,  label="Comparator",           color=colors[3])

    ax.set_xlabel("Historical dataset size")
    ax.set_ylabel("Latency (ms)")
    ax.set_xticks(x)
    ax.set_xticklabels(labels)
    ax.legend(loc="upper left", frameon=False)
    ax.yaxis.set_major_formatter(ticker.FormatStrFormatter("%.0f"))

    plt.tight_layout()
    out = "results/figures/h1_latency_breakdown.png"
    plt.savefig(out, dpi=150, bbox_inches="tight")
    plt.close()
    print(f"Saved: {out}")


# ── Figure 2: H1 path isolation (line chart) ──────────────────────────────────
def plot_h1_isolation():
    rows = read_csv("results/h1_isolation.csv")
    if not rows:
        return

    qps  = [int(r["background_hist_qps"])  for r in rows]
    mean = [float(r["live_window_mean_ms"]) for r in rows]
    std  = [float(r["live_window_std_ms"])  for r in rows]

    fig, ax = plt.subplots()
    ax.errorbar(qps, mean, yerr=std, fmt="o-", capsize=4,
                color="#4C9BE8", label="Live window latency")
    ax.axhline(mean[0], linestyle="--", color="#aaa", linewidth=0.8,
               label="Baseline (0 background queries)")
    ax.set_xlabel("Background historical queries/sec")
    ax.set_ylabel("Live window latency (ms)")
    ax.set_xticks(qps)
    ax.legend(frameon=False)

    plt.tight_layout()
    out = "results/figures/h1_isolation.png"
    plt.savefig(out, dpi=150, bbox_inches="tight")
    plt.close()
    print(f"Saved: {out}")


# ── Figure 3: H2 detection rate and latency by anomaly type ───────────────────
def plot_h2_detection():
    rows = read_csv("results/h2_summary.csv")
    if not rows:
        return

    per_type = [r for r in rows if r.get("anomaly_type", "") != "overall"]
    if not per_type:
        per_type = rows

    types   = [r["anomaly_type"].replace("_", "\n") for r in per_type]
    rates   = [float(r["detection_rate"]) * 100      for r in per_type]
    latency = [float(r["mean_latency_ms"]) / 1000.0  for r in per_type]

    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(8, 3.5))

    colors = ["#4CE87B" if r >= 100.0 else "#E87B4C" for r in rates]
    ax1.bar(types, rates, color=colors)
    ax1.set_ylabel("Detection rate (%)")
    ax1.set_ylim(0, 115)
    ax1.axhline(100, linestyle="--", color="#aaa", linewidth=0.8)
    ax1.set_title("Anomaly detection rate")

    ax2.bar(types, latency, color="#4C9BE8")
    ax2.set_ylabel("Mean detection latency (s)")
    ax2.set_title("Detection latency by type")

    plt.tight_layout()
    out = "results/figures/h2_detection.png"
    plt.savefig(out, dpi=150, bbox_inches="tight")
    plt.close()
    print(f"Saved: {out}")


# ── Figure 4: H4 scalability — historical vs live latency (log-log) ───────────
def plot_h4_scalability():
    rows = read_csv("results/h4_summary.csv")
    if not rows:
        return

    sizes     = [int(r["dataset_size_quads"])         for r in rows]
    hist_mean = [float(r["hist_mean_ms"])               for r in rows]
    hist_std  = [float(r["hist_std_ms"])                for r in rows]
    live_mean = [float(r["live_mean_ms"])               for r in rows]

    fig, ax = plt.subplots()

    ax.errorbar(sizes, hist_mean, yerr=hist_std, fmt="o-", capsize=4,
                color="#E87B4C", label="Historical retrieval")
    ax.plot(sizes, live_mean, "s--", color="#4C9BE8",
            label="Live window (should be flat)")

    # Linear reference line anchored at smallest size
    linear = [hist_mean[0] * (s / sizes[0]) for s in sizes]
    ax.plot(sizes, linear, ":", color="#bbb", linewidth=0.9,
            label="Linear growth (reference)")

    ax.set_xscale("log")
    ax.set_yscale("log")
    ax.set_xlabel("Historical dataset size (quads)")
    ax.set_ylabel("Latency (ms)")
    ax.legend(frameon=False)
    ax.xaxis.set_major_formatter(ticker.FuncFormatter(
        lambda x, _: f"{int(x)//1000}K" if x < 1_000_000 else f"{int(x)//1_000_000}M"
    ))

    plt.tight_layout()
    out = "results/figures/h4_scalability.png"
    plt.savefig(out, dpi=150, bbox_inches="tight")
    plt.close()
    print(f"Saved: {out}")


if __name__ == "__main__":
    plot_h1_breakdown()
    plot_h1_isolation()
    plot_h2_detection()
    plot_h4_scalability()
    print("\nAll available figures saved to results/figures/")
