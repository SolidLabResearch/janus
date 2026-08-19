#!/usr/bin/env python3

from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
from matplotlib.ticker import FixedLocator, LogFormatterMathtext


MEMORY_CPU_CSV = Path("memory_cpu_1m_median_data.csv")
STORAGE_SUMMARY_CSV = Path("storage_footprint_summary.csv")

OUTPUT_PREFIX = "resource_storage_footprint_combined"

QUERY_ORDER = ["Point", "60s", "10%", "50%", "100%"]
EVENT_ORDER = [10000, 50000, 100000, 1000000, 10000000]
EVENT_LABELS = {
    10000: "10k",
    50000: "50k",
    100000: "100k",
    1000000: "1M",
    10000000: "10M",
}

SYSTEM_STYLES = [
    ("Janus", "Janus", "o", "-"),
    ("Oxigraph", "Decomposed Baseline", "s", "--"),
]


def normalize_system_label(label: str) -> str:
    value = str(label).strip().lower()
    if value == "janus":
        return "Janus"
    if value in {"oxigraph", "decomposed_oxigraph"}:
        return "Oxigraph"
    return str(label)


def load_memory_cpu_data() -> pd.DataFrame:
    df = pd.read_csv(MEMORY_CPU_CSV)

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
        raise ValueError(f"Missing required memory/CPU columns: {sorted(missing)}")

    df["system_label"] = df["system_label"].map(normalize_system_label)

    for query in QUERY_ORDER:
        subset = df[df["query_label"] == query]
        systems = set(subset["system_label"])
        if systems != {"Janus", "Oxigraph"}:
            raise ValueError(f"Expected Janus and Oxigraph for {query}, found {sorted(systems)}")

    return df


def load_storage_data() -> pd.DataFrame:
    df = pd.read_csv(STORAGE_SUMMARY_CSV)

    required = {
        "event_count",
        "system",
        "n",
        "median_storage_mb",
        "median_bytes_per_event",
        "median_load_time_ms",
        "median_events_per_second",
    }
    missing = required - set(df.columns)
    if missing:
        raise ValueError(f"Missing required storage columns: {sorted(missing)}")

    df["system_label"] = df["system"].map(normalize_system_label)
    df["event_count"] = df["event_count"].astype(int)

    for event_count in EVENT_ORDER:
        subset = df[df["event_count"] == event_count]
        systems = set(subset["system_label"])
        if systems != {"Janus", "Oxigraph"}:
            raise ValueError(
                f"Expected Janus and Oxigraph for {event_count}, found {sorted(systems)}"
            )

    return df


def ordered_query_subset(df: pd.DataFrame, system: str) -> pd.DataFrame:
    return (
        df[df["system_label"] == system]
        .set_index("query_label")
        .loc[QUERY_ORDER]
        .reset_index()
    )


def ordered_event_subset(df: pd.DataFrame, system: str) -> pd.DataFrame:
    return (
        df[df["system_label"] == system]
        .set_index("event_count")
        .loc[EVENT_ORDER]
        .reset_index()
    )


def main() -> None:
    memory_df = load_memory_cpu_data()
    storage_df = load_storage_data()

    query_x = np.arange(len(QUERY_ORDER))
    event_x = np.arange(len(EVENT_ORDER))
    event_labels = [EVENT_LABELS[event_count] for event_count in EVENT_ORDER]

    fig, axes = plt.subplots(2, 2, figsize=(6.9, 4.4))

    panels = [
        (
            axes[0, 0],
            "memory",
            "median_peak_rss_mb",
            "Median peak RSS (MB)",
            "(a) Peak memory",
            False,
        ),
        (
            axes[0, 1],
            "memory",
            "median_cpu_percent",
            "Median per-run CPU (%)",
            "(b) CPU utilization",
            False,
        ),
        (
            axes[1, 0],
            "storage",
            "median_storage_mb",
            "Median storage footprint (MB)",
            "(c) Persistent storage",
            True,
        ),
        (
            axes[1, 1],
            "storage",
            "median_load_time_ms",
            "Median write time (ms)",
            "(d) Ingestion time",
            True,
        ),
    ]

    for ax, source, metric, ylabel, title, use_log_y in panels:
        for system_key, system_label, marker, linestyle in SYSTEM_STYLES:
            if source == "memory":
                subset = ordered_query_subset(memory_df, system_key)
                x = query_x
                y = subset[metric].to_numpy()
            else:
                subset = ordered_event_subset(storage_df, system_key)
                x = event_x
                y = subset[metric].to_numpy()

            ax.plot(
                x,
                y,
                marker=marker,
                linestyle=linestyle,
                linewidth=1.2,
                markersize=3.5,
                label=system_label,
            )

        ax.set_title(title, fontsize=8)
        ax.set_ylabel(ylabel, fontsize=7)
        ax.tick_params(axis="both", labelsize=7)
        ax.grid(True, axis="y", linewidth=0.35, alpha=0.45)
        ax.grid(False, axis="x")

        if use_log_y:
            ax.set_yscale("log")

        if source == "memory":
            ax.set_xticks(query_x)
            ax.set_xticklabels(QUERY_ORDER, fontsize=7)
        else:
            ax.set_xticks(event_x)
            ax.set_xticklabels(event_labels, fontsize=7)

        if metric == "median_peak_rss_mb":
            ax.set_ylim(1000, 2000)
            ax.set_yticks([1200, 1400, 1600, 1800, 2000])
        elif metric == "median_cpu_percent":
            ax.set_ylim(10, 22)
            ax.set_yticks([10, 12, 14, 16, 18, 20, 22])
        elif metric == "median_load_time_ms":
            ax.set_ylim(1e1, 1e5)
            ax.yaxis.set_major_locator(FixedLocator([1e1, 1e2, 1e3, 1e4, 1e5]))
            ax.yaxis.set_major_formatter(LogFormatterMathtext())

    axes[0, 0].set_xlabel("Historical access pattern", fontsize=7)
    axes[0, 1].set_xlabel("Historical access pattern", fontsize=7)
    axes[1, 0].set_xlabel("Historical events", fontsize=7)
    axes[1, 1].set_xlabel("Historical events", fontsize=7)

    handles, labels = axes[0, 0].get_legend_handles_labels()
    fig.legend(
        handles,
        labels,
        loc="center left",
        bbox_to_anchor=(0.885, 0.5),
        ncol=1,
        frameon=False,
        fontsize=7,
        handlelength=2.0,
        columnspacing=1.0,
    )

    fig.tight_layout(rect=(0, 0, 0.88, 1), w_pad=1.0, h_pad=1.3)

    pdf_path = Path(f"{OUTPUT_PREFIX}.pdf")
    png_path = Path(f"{OUTPUT_PREFIX}.png")

    fig.savefig(pdf_path, bbox_inches="tight")
    fig.savefig(png_path, dpi=300, bbox_inches="tight")
    plt.close(fig)

    print(f"Wrote {pdf_path}")
    print(f"Wrote {png_path}")


if __name__ == "__main__":
    main()
