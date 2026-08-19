#!/usr/bin/env python3

from __future__ import annotations

import re
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd


LATEX_TABLE = r"""
\begin{table}[t]
\centering
\caption{Peak process memory at one million historical quads. Values are median peak RSS in MB across 35 iterations.}
\label{tab:peak-memory-1m}
\scriptsize
\begin{tabular}{lrr}
\toprule
Query & Janus & Oxigraph \\
\midrule
Point lookup & $1141.8$ & $1837.6$ \\
Fixed 60s & $1172.0$ & $1834.5$ \\
Range 10\% & $1220.3$ & $1836.8$ \\
Range 50\% & $1844.8$ & $1844.8$ \\
Range 100\% & $1892.4$ & $1892.4$ \\
\bottomrule
\end{tabular}
\end{table}
"""

QUERY_ORDER = ["Point lookup", "Fixed 60s", "Range 10%", "Range 50%", "Range 100%"]
SHORT_QUERY_LABELS = {
    "Point lookup": "Point",
    "Fixed 60s": "60s",
    "Range 10%": "10%",
    "Range 50%": "50%",
    "Range 100%": "100%",
}
SYSTEM_ORDER = ["Janus", "Oxigraph"]


def clean_label(text: str) -> str:
    return text.strip().replace(r"\%", "%")


def parse_median_cell(cell: str) -> float:
    normalized = cell.strip().replace("$", "").replace(" ", "")
    match = re.fullmatch(r"[-+]?\d+(?:\.\d+)?", normalized)
    if match is None:
        raise ValueError(f"Could not parse median cell: {cell!r}")
    return float(normalized)


def extract_rows_between_rules(latex_table: str) -> list[str]:
    match = re.search(r"\\midrule(?P<body>.*?)\\bottomrule", latex_table, flags=re.DOTALL)
    if match is None:
        raise ValueError("Could not find table body between \\midrule and \\bottomrule.")

    rows = []
    for raw_row in match.group("body").split(r"\\"):
        row = raw_row.strip()
        if row:
            rows.append(row)
    return rows


def latex_table_to_dataframe(latex_table: str) -> pd.DataFrame:
    records: list[dict[str, object]] = []

    for row in extract_rows_between_rules(latex_table):
        parts = [part.strip() for part in row.split("&")]
        if len(parts) != 3:
            raise ValueError(f"Expected 3 columns, found {len(parts)} in row:\n{row}")

        query = clean_label(parts[0])
        records.append({"query": query, "system": "Janus", "median_mb": parse_median_cell(parts[1])})
        records.append({"query": query, "system": "Oxigraph", "median_mb": parse_median_cell(parts[2])})

    df = pd.DataFrame.from_records(records)
    expected_rows = len(QUERY_ORDER) * len(SYSTEM_ORDER)
    if len(df) != expected_rows:
        raise ValueError(f"Expected {expected_rows} parsed rows, found {len(df)}.")
    return df


def plot_memory_lines(df: pd.DataFrame, output_prefix: str = "peak_memory_1m_median_lineplot_compact") -> None:
    x = np.arange(len(QUERY_ORDER))

    fig, ax = plt.subplots(figsize=(4.4, 2.2))

    for system, marker, linestyle in [("Janus", "o", "-"), ("Oxigraph", "s", "--")]:
        subset = df[df["system"] == system].set_index("query").loc[QUERY_ORDER].reset_index()
        ax.plot(
            x,
            subset["median_mb"].to_numpy(),
            marker=marker,
            linestyle=linestyle,
            linewidth=1.1,
            markersize=3.2,
            label=system,
        )

    ax.set_xticks(x)
    ax.set_xticklabels([SHORT_QUERY_LABELS[label] for label in QUERY_ORDER], fontsize=7)
    ax.set_ylabel("Peak RSS (MB)", fontsize=8)
    ax.set_xlabel("Access pattern", fontsize=8, labelpad=1)
    ax.tick_params(axis="y", labelsize=7)
    ax.grid(True, axis="y", linewidth=0.35, alpha=0.4)
    ax.legend(
        frameon=False,
        fontsize=7,
        ncol=2,
        loc="upper center",
        bbox_to_anchor=(0.5, 1.18),
        handlelength=1.8,
        columnspacing=1.0,
    )

    fig.tight_layout(pad=0.3)

    pdf_path = Path(f"{output_prefix}.pdf")
    png_path = Path(f"{output_prefix}.png")
    fig.savefig(pdf_path, bbox_inches="tight", pad_inches=0.02)
    fig.savefig(png_path, dpi=300, bbox_inches="tight", pad_inches=0.02)
    plt.close(fig)

    print(f"Wrote {pdf_path}")
    print(f"Wrote {png_path}")


def main() -> None:
    df = latex_table_to_dataframe(LATEX_TABLE)
    plot_memory_lines(df)


if __name__ == "__main__":
    main()
