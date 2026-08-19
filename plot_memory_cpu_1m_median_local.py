#!/usr/bin/env python3

from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd


CSV_FILE = Path("memory_cpu_1m_median_data.csv")
OUTPUT_PREFIX = "memory_cpu_1m_median_side_by_side"

QUERY_ORDER = ["Point", "60s", "10%", "50%", "100%"]
SYSTEM_STYLES = [
    ("Janus", "Janus", "o", "-"),
    ("Oxigraph", "Decomposed Baseline", "s", "--"),
]


def load_data() -> pd.DataFrame:
    df = pd.read_csv(CSV_FILE)

    if "median_cpu_percent" not in df.columns and "median_mean_cpu_percent" in df.columns:
        df = df.rename(columns={"median_mean_cpu_percent": "median_cpu_percent"})

    required = {
        "query_label",
        "system_label",
        "median_peak_rss_mb",
        "median_cpu_percent",
        "n",
    }
    missing = required - set(df.columns)
    if missing:
        raise ValueError(f"Missing required columns: {sorted(missing)}")

    for query in QUERY_ORDER:
        subset = df[df["query_label"] == query]
        if len(subset) != 2:
            raise ValueError(f"Expected two systems for {query}, found {len(subset)}")

    return df


def ordered_subset(df: pd.DataFrame, system: str) -> pd.DataFrame:
    return (
        df[df["system_label"] == system]
        .set_index("query_label")
        .loc[QUERY_ORDER]
        .reset_index()
    )


def main() -> None:
    df = load_data()
    x = np.arange(len(QUERY_ORDER))

    fig, axes = plt.subplots(1, 2, figsize=(6.8, 2.15), sharex=True)

    panels = [
        ("median_peak_rss_mb", "Median peak RSS (MB)", "(a) Peak memory"),
        ("median_cpu_percent", "Median per-run CPU (%)", "(b) CPU utilization"),
    ]

    for ax, (metric, ylabel, title) in zip(axes, panels):
        for system_key, system_label, marker, linestyle in SYSTEM_STYLES:
            subset = ordered_subset(df, system_key)

            ax.plot(
                x,
                subset[metric].to_numpy(),
                marker=marker,
                linestyle=linestyle,
                linewidth=1.2,
                markersize=3.5,
                label=system_label,
            )

        ax.set_title(title, fontsize=8)
        ax.set_xticks(x)
        ax.set_xticklabels(QUERY_ORDER, fontsize=7)
        ax.set_ylabel(ylabel, fontsize=7)
        ax.tick_params(axis="both", labelsize=7)
        ax.grid(True, axis="y", linewidth=0.35, alpha=0.45)
        ax.grid(False, axis="x")

    fig.supxlabel("Historical access pattern", fontsize=7, y=0.02)
    handles, labels = axes[0].get_legend_handles_labels()
    fig.legend(
        handles,
        labels,
        loc="upper center",
        bbox_to_anchor=(0.5, 1.02),
        ncol=2,
        frameon=False,
        fontsize=7,
        handlelength=2.0,
        columnspacing=1.2,
    )

    fig.tight_layout(rect=(0, 0.03, 1, 0.90), w_pad=0.9)

    pdf_path = Path(f"{OUTPUT_PREFIX}.pdf")
    png_path = Path(f"{OUTPUT_PREFIX}.png")

    fig.savefig(pdf_path, bbox_inches="tight")
    fig.savefig(png_path, dpi=300, bbox_inches="tight")
    plt.close(fig)

    print(f"Wrote {pdf_path}")
    print(f"Wrote {png_path}")


if __name__ == "__main__":
    main()

# Caption suggestion:
# Median process-level resource usage at one million historical quads across
# 35 iterations. The left panel reports median peak RSS, while the right panel
# reports median per-run CPU utilization. CPU values are derived from the
# per-run average CPU samples.
