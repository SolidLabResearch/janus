#!/usr/bin/env python3
"""
Parse a LaTeX table containing mean \pm standard deviation values and generate
a deterministic 5-panel historical-access latency figure.

Outputs:
  - historical_access_latency_5panel_shared_yaxis.pdf
  - historical_access_latency_5panel_shared_yaxis.png
  - historical_access_latency_parsed.csv

Run:
  python plot_historical_access_latency.py
"""

from __future__ import annotations

import re
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd


LATEX_TABLE = r"""
\begin{table}[t]
\centering
\caption{Historical access latency inside the hybrid benchmark. Values are mean $\pm$ standard deviation in ms.}
\label{tab:historical-access-scaling}
\scriptsize
\begin{tabular}{llrrrrr}
\toprule
Query & System & 10k & 50k & 100k & 500k & 1M \\
\midrule
Point & Janus & $0.042\pm0.006$ & $0.033\pm0.001$ & $0.046\pm0.006$ & $0.050\pm0.006$ & $0.068\pm0.004$ \\
Point & Oxigraph & $6.988\pm0.091$ & $40.722\pm0.587$ & $83.136\pm0.870$ & $429.221\pm5.237$ & $818.002\pm8.683$ \\
Fixed 60s & Janus & $1.366\pm0.217$ & $1.213\pm0.026$ & $1.213\pm0.031$ & $1.224\pm0.029$ & $1.247\pm0.060$ \\
Fixed 60s & Oxigraph & $7.292\pm0.060$ & $37.617\pm0.204$ & $81.432\pm0.547$ & $416.478\pm2.053$ & $845.388\pm6.130$ \\
Range 10\% & Janus & $1.313\pm0.125$ & $5.589\pm0.083$ & $11.103\pm0.086$ & $54.602\pm0.268$ & $111.088\pm2.654$ \\
Range 10\% & Oxigraph & $7.323\pm0.063$ & $42.236\pm0.235$ & $86.156\pm0.540$ & $445.541\pm3.121$ & $860.690\pm4.201$ \\
Range 50\% & Janus & $6.437\pm0.501$ & $27.670\pm0.144$ & $54.777\pm0.250$ & $269.867\pm0.794$ & $541.342\pm6.438$ \\
Range 50\% & Oxigraph & $9.560\pm0.117$ & $51.502\pm0.236$ & $107.374\pm0.597$ & $557.400\pm3.166$ & $1125.296\pm5.510$ \\
Range 100\% & Janus & $13.023\pm1.286$ & $55.007\pm0.467$ & $108.847\pm0.343$ & $538.963\pm1.464$ & $1075.730\pm2.685$ \\
Range 100\% & Oxigraph & $12.331\pm0.156$ & $66.026\pm0.483$ & $134.321\pm1.565$ & $698.019\pm2.919$ & $1454.357\pm8.035$ \\
\bottomrule
\end{tabular}
\end{table}
"""


QUERY_ORDER = ["Point", "Fixed 60s", "Range 10%", "Range 50%", "Range 100%"]
SYSTEM_ORDER = ["Janus", "Oxigraph"]
SIZE_ORDER = ["10k", "50k", "100k", "500k", "1M"]


def clean_latex_label(text: str) -> str:
    """Normalize simple LaTeX escapes used in row labels."""
    return (
        text.strip()
        .replace(r"\%", "%")
        .replace(r"\_", "_")
        .replace(r"\ ", " ")
    )


def parse_mean_std(cell: str) -> tuple[float, float]:
    """
    Parse a LaTeX table cell of the form:
      $0.042\\pm0.006$

    Also accepts:
      0.042\\pm0.006
      0.042 \\pm 0.006
      0.042±0.006
    """
    normalized = (
        cell.strip()
        .replace("$", "")
        .replace(r"\,", "")
        .replace(" ", "")
    )

    match = re.fullmatch(
        r"([-+]?\d+(?:\.\d+)?)"
        r"(?:\\pm|±)"
        r"([-+]?\d+(?:\.\d+)?)",
        normalized,
    )

    if match is None:
        raise ValueError(f"Could not parse mean/std cell: {cell!r}")

    mean = float(match.group(1))
    std = float(match.group(2))
    return mean, std


def extract_rows_between_rules(latex_table: str) -> list[str]:
    """Extract table rows between \\midrule and \\bottomrule."""
    match = re.search(
        r"\\midrule(?P<body>.*?)\\bottomrule",
        latex_table,
        flags=re.DOTALL,
    )

    if match is None:
        raise ValueError("Could not find table body between \\midrule and \\bottomrule.")

    body = match.group("body")
    rows = []

    for raw_row in body.split(r"\\"):
        row = raw_row.strip()

        if not row:
            continue

        if row.startswith(r"\midrule") or row.startswith(r"\bottomrule"):
            continue

        rows.append(row)

    return rows


def latex_table_to_dataframe(latex_table: str) -> pd.DataFrame:
    """Convert the LaTeX table into a tidy dataframe."""
    records: list[dict[str, object]] = []

    for row in extract_rows_between_rules(latex_table):
        parts = [part.strip() for part in row.split("&")]

        expected_columns = 2 + len(SIZE_ORDER)
        if len(parts) != expected_columns:
            raise ValueError(
                f"Expected {expected_columns} columns, found {len(parts)} in row:\n{row}"
            )

        query = clean_latex_label(parts[0])
        system = clean_latex_label(parts[1])

        for size, cell in zip(SIZE_ORDER, parts[2:]):
            mean, std = parse_mean_std(cell)

            records.append(
                {
                    "query": query,
                    "system": system,
                    "size": size,
                    "mean_ms": mean,
                    "std_ms": std,
                }
            )

    df = pd.DataFrame.from_records(records)

    expected_rows = len(QUERY_ORDER) * len(SYSTEM_ORDER) * len(SIZE_ORDER)
    if len(df) != expected_rows:
        raise ValueError(f"Expected {expected_rows} parsed rows, found {len(df)}.")

    validate_dataframe(df)
    return df


def validate_dataframe(df: pd.DataFrame) -> None:
    """Fail loudly if the parsed table is incomplete or malformed."""
    required_columns = {"query", "system", "size", "mean_ms", "std_ms"}
    missing_columns = required_columns - set(df.columns)

    if missing_columns:
        raise ValueError(f"Missing required columns: {sorted(missing_columns)}")

    parsed_queries = set(df["query"])
    parsed_systems = set(df["system"])
    parsed_sizes = set(df["size"])

    if parsed_queries != set(QUERY_ORDER):
        raise ValueError(
            f"Unexpected query labels.\n"
            f"Expected: {QUERY_ORDER}\n"
            f"Found: {sorted(parsed_queries)}"
        )

    if parsed_systems != set(SYSTEM_ORDER):
        raise ValueError(
            f"Unexpected system labels.\n"
            f"Expected: {SYSTEM_ORDER}\n"
            f"Found: {sorted(parsed_systems)}"
        )

    if parsed_sizes != set(SIZE_ORDER):
        raise ValueError(
            f"Unexpected size labels.\n"
            f"Expected: {SIZE_ORDER}\n"
            f"Found: {sorted(parsed_sizes)}"
        )

    for query in QUERY_ORDER:
        for system in SYSTEM_ORDER:
            subset = df[(df["query"] == query) & (df["system"] == system)]

            if len(subset) != len(SIZE_ORDER):
                raise ValueError(
                    f"Expected {len(SIZE_ORDER)} values for {query}/{system}, "
                    f"found {len(subset)}."
                )

    if (df["mean_ms"] <= 0).any():
        raise ValueError("All mean latency values must be positive for log-scale plotting.")

    if (df["std_ms"] < 0).any():
        raise ValueError("Standard deviation values must be non-negative.")


def plot_shared_yaxis_figure(
    df: pd.DataFrame,
    output_prefix: str = "historical_access_latency_5panel_shared_yaxis",
) -> None:
    """
    Plot five access patterns in a 2x3 panel layout.

    The sixth panel is reserved for the legend.
    All panels use the same logarithmic y-axis range for fair comparison.
    """
    x = np.arange(len(SIZE_ORDER))

    fig, axes = plt.subplots(
        2,
        3,
        figsize=(7.2, 5.0),
        sharex=False,
        sharey=True,
    )

    axes = axes.flatten()
    handles = []
    labels = []

    for ax, query in zip(axes[:5], QUERY_ORDER):
        for system in SYSTEM_ORDER:
            subset = (
                df[(df["query"] == query) & (df["system"] == system)]
                .set_index("size")
                .loc[SIZE_ORDER]
                .reset_index()
            )

            linestyle = "-" if system == "Janus" else "--"
            marker = "o" if system == "Janus" else "s"

            handle = ax.errorbar(
                x,
                subset["mean_ms"].to_numpy(),
                yerr=subset["std_ms"].to_numpy(),
                marker=marker,
                linestyle=linestyle,
                linewidth=1.2,
                markersize=3.5,
                capsize=2,
                label=system,
            )

            if query == QUERY_ORDER[0]:
                handles.append(handle)
                labels.append(system)

        ax.set_title(query, fontsize=9)
        ax.set_yscale("log")
        ax.set_ylim(0.01, 2000)
        ax.set_xticks(x)
        ax.set_xticklabels(SIZE_ORDER, fontsize=8)
        ax.set_xlabel("Log size", fontsize=8)
        ax.tick_params(axis="x", labelsize=8, labelbottom=True)
        ax.tick_params(axis="y", labelsize=8)
        ax.grid(True, which="major", linewidth=0.4, alpha=0.5)
        ax.grid(True, which="minor", linewidth=0.25, alpha=0.25)

    axes[0].set_ylabel("Latency (ms, log scale)", fontsize=9)
    axes[3].set_ylabel("Latency (ms, log scale)", fontsize=9)

    axes[5].axis("off")
    axes[5].legend(handles, labels, loc="center", frameon=False, fontsize=9)

    plt.tight_layout()

    output_pdf = Path(f"{output_prefix}.pdf")
    output_png = Path(f"{output_prefix}.png")

    fig.savefig(output_pdf, bbox_inches="tight")
    fig.savefig(output_png, dpi=300, bbox_inches="tight")
    plt.close(fig)

    print(f"Wrote {output_pdf}")
    print(f"Wrote {output_png}")


def main() -> None:
    df = latex_table_to_dataframe(LATEX_TABLE)

    csv_path = Path("historical_access_latency_parsed.csv")
    df.to_csv(csv_path, index=False)
    print(f"Wrote {csv_path}")

    plot_shared_yaxis_figure(df)

    print("\nParsed values:")
    print(df.to_string(index=False))


if __name__ == "__main__":
    main()
