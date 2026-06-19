#!/usr/bin/env python3
"""Generate paper-ready figures and compact tables from Janus benchmark outputs."""

from __future__ import annotations

import argparse
import csv
import re
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Iterable, List, Tuple

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.ticker import FuncFormatter


@dataclass
class PlotArtifact:
    path: Path
    kind: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hybrid-csv", type=Path, required=True)
    parser.add_argument("--scaling-csv", type=Path, required=True)
    parser.add_argument("--subquery-md", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    return parser.parse_args()


def read_csv_rows(path: Path) -> List[Dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def ensure_output_dir(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True)


def save_figure(fig: plt.Figure, base_path: Path) -> List[Path]:
    png_path = base_path.with_suffix(".png")
    pdf_path = base_path.with_suffix(".pdf")
    fig.savefig(png_path, dpi=300, bbox_inches="tight")
    fig.savefig(pdf_path, bbox_inches="tight")
    plt.close(fig)
    return [png_path, pdf_path]


def format_bytes(value: float) -> str:
    if abs(value) >= 1_000_000:
        return f"{value / 1_000_000:.2f} MB"
    if abs(value) >= 1_000:
        return f"{value / 1_000:.1f} kB"
    return f"{value:.0f} B"


def format_ms(value: float) -> str:
    return f"{value:.3f}"


def human_dataset_size(value: str) -> str:
    try:
        size = int(float(value))
    except ValueError:
        return value
    if size >= 1_000_000:
        return f"{size // 1_000_000}M"
    if size >= 1_000:
        return f"{size // 1_000}k"
    return str(size)


def parse_subquery_markdown(path: Path) -> Dict[str, Dict[str, str]]:
    text = path.read_text(encoding="utf-8")
    query_blocks: Dict[str, Dict[str, str]] = {}
    current_query = None
    in_code_block = False
    for line in text.splitlines():
        if line.startswith("```"):
            in_code_block = not in_code_block
            continue
        if line.startswith("delta_nested_minus_explicit"):
            current_query = None
            in_code_block = False
            continue
        if line.startswith("query="):
            current_query = line.split("=", 1)[1].strip()
            query_blocks[current_query] = {}
            continue
        if current_query is None or not in_code_block:
            continue
        if line.startswith("  ") and "=" in line:
            key, value = line.strip().split("=", 1)
            query_blocks[current_query][key] = value
    return query_blocks


def plot_historical_scaling(rows: List[Dict[str, str]], output_dir: Path) -> List[Path]:
    grouped: Dict[str, List[Tuple[int, float]]] = defaultdict(list)
    for row in rows:
        grouped[row["query_type"]].append((int(row["dataset_size_quads"]), float(row["p50_latency_ms"])))

    fig, ax = plt.subplots(figsize=(10.5, 6.5))
    palette = plt.get_cmap("tab10")
    for index, query_type in enumerate(sorted(grouped)):
        points = sorted(grouped[query_type], key=lambda item: item[0])
        xs = [point[0] for point in points]
        ys = [point[1] for point in points]
        label = query_type.replace("_", " ")
        ax.plot(xs, ys, marker="o", linewidth=2.0, markersize=5, label=label, color=palette(index % 10))

    ax.set_title("Historical query latency by dataset size")
    ax.set_xlabel("Historical dataset size (quads)")
    ax.set_ylabel("P50 latency (ms)")
    ax.set_yscale("log")
    ax.grid(True, axis="both", which="major", linestyle="--", linewidth=0.7, alpha=0.35)
    ax.grid(True, axis="y", which="minor", linestyle=":", linewidth=0.5, alpha=0.2)
    ax.set_xticks(sorted({int(row["dataset_size_quads"]) for row in rows}))
    ax.xaxis.set_major_formatter(FuncFormatter(lambda value, _: human_dataset_size(str(int(value)))))
    ax.yaxis.set_major_formatter(FuncFormatter(lambda value, _: f"{value:g}"))
    ax.legend(frameon=False, ncol=2, loc="upper left")
    fig.tight_layout()
    return save_figure(fig, output_dir / "historical_scaling_p50")


def plot_hybrid_latency(rows: List[Dict[str, str]], output_dir: Path) -> List[Path]:
    metrics = [
        ("p50_e2e_latency_ms", "P50 end-to-end latency (ms)"),
        ("p95_e2e_latency_ms", "P95 end-to-end latency (ms)"),
        ("avg_coordination_overhead_ms", "Avg. coordination overhead (ms)"),
    ]
    systems = [("janus_unified", "Janus unified"), ("Oxigraph historical + Janus live window processor + external join", "Decomposed")]
    values = {row["system"]: row for row in rows}

    fig, ax = plt.subplots(figsize=(10.5, 6.3))
    width = 0.36
    x_positions = list(range(len(metrics)))
    offsets = [-width / 2, width / 2]
    colors = ["#2a6fdb", "#d1495b"]

    for offset, (system_key, label), color in zip(offsets, systems, colors):
        row = values[system_key]
        ys = [float(row[field]) for field, _ in metrics]
        ax.bar([x + offset for x in x_positions], ys, width=width, label=label, color=color, edgecolor="black", linewidth=0.5)

    ax.set_title("Unified and decomposed hybrid execution latency")
    ax.set_ylabel("Milliseconds")
    ax.set_xticks(x_positions)
    ax.set_xticklabels([label for _, label in metrics])
    ax.grid(True, axis="y", linestyle="--", linewidth=0.7, alpha=0.35)
    ax.legend(frameon=False)
    fig.tight_layout()
    return save_figure(fig, output_dir / "hybrid_coordination_latency")


def plot_hybrid_transfer(rows: List[Dict[str, str]], output_dir: Path) -> List[Path]:
    fig, ax = plt.subplots(figsize=(8.2, 5.5))
    systems = [("janus_unified", "Janus unified", "#2a6fdb"), ("Oxigraph historical + Janus live window processor + external join", "Decomposed", "#d1495b")]
    values = {row["system"]: row for row in rows}
    xs = range(len(systems))
    ys = [float(values[key]["avg_external_transfer_bytes"]) for key, _, _ in systems]
    labels = [label for _, label, _ in systems]
    colors = [color for _, _, color in systems]

    bars = ax.bar(xs, ys, color=colors, edgecolor="black", linewidth=0.5)
    ax.set_title("External transfer in unified and decomposed execution")
    ax.set_ylabel("Bytes transferred")
    ax.set_xticks(list(xs))
    ax.set_xticklabels(labels)
    ax.grid(True, axis="y", linestyle="--", linewidth=0.7, alpha=0.35)
    for bar, value in zip(bars, ys):
        ax.annotate(format_bytes(value), (bar.get_x() + bar.get_width() / 2, bar.get_height()),
                    ha="center", va="bottom", fontsize=9, xytext=(0, 4), textcoords="offset points")
    fig.tight_layout()
    return save_figure(fig, output_dir / "hybrid_coordination_transfer")


def build_subquery_table(query_blocks: Dict[str, Dict[str, str]]) -> str:
    rows = [
        ("explicit baseline", query_blocks.get("explicit_define_baseline", {})),
        ("nested historical subquery", query_blocks.get("nested_historical_subquery", {})),
    ]
    lines = [
        "| Query form | parse_total_ms_avg | planning_lowering_ms_avg | historical_materialization_ms_avg | live_startup_ms_avg | baseline_bindings | planning path |",
        "| --- | ---: | ---: | ---: | ---: | ---: | --- |",
    ]
    for label, data in rows:
        planning = data.get("planning", "")
        planning_summary = "No nested subquery planning diagnostics"
        if "HistoricalMaterializedOnce" in planning:
            planning_summary = "Execution mode: HistoricalMaterializedOnce; Physical plan: MaterializeHistoricalResult"
        lines.append(
            f"| {label} | {data.get('parse_total_ms_avg', '')} | {data.get('planning_lowering_ms_avg', '')} | "
            f"{data.get('historical_materialization_ms_avg', '')} | {data.get('live_startup_ms_avg', '')} | "
            f"{data.get('baseline_bindings', '')} | {planning_summary} |"
        )
    return "\n".join(lines)


def build_hybrid_table(rows: List[Dict[str, str]]) -> str:
    order = {"janus_unified": 0, "Oxigraph historical + Janus live window processor + external join": 1}
    rows = sorted(rows, key=lambda row: order.get(row["system"], 99))
    lines = [
        "| System | p50_e2e_latency_ms | p95_e2e_latency_ms | avg_coordination_overhead_ms | avg_external_transfer_bytes | components | process_boundaries | serialization_steps |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for row in rows:
        label = "Janus unified" if row["system"] == "janus_unified" else "Decomposed"
        lines.append(
            f"| {label} | {row['p50_e2e_latency_ms']} | {row['p95_e2e_latency_ms']} | {row['avg_coordination_overhead_ms']} | "
            f"{row['avg_external_transfer_bytes']} | {row['components']} | {row['process_boundaries']} | {row['serialization_steps']} |"
        )
    return "\n".join(lines)


def build_scaling_table(rows: List[Dict[str, str]]) -> str:
    selected_queries = [
        "point_lookup",
        "fixed_window",
        "proportional_range_10",
        "proportional_range_50",
        "full_range",
        "hybrid_baseline_lookup",
    ]
    by_query: Dict[str, List[Dict[str, str]]] = defaultdict(list)
    for row in rows:
        by_query[row["query_type"]].append(row)

    lines = [
        "| Query type | Smallest dataset latency | Largest dataset latency | Overall trend |",
        "| --- | ---: | ---: | --- |",
    ]
    for query_type in selected_queries:
        query_rows = sorted(by_query[query_type], key=lambda row: int(row["dataset_size_quads"]))
        smallest = query_rows[0]
        largest = query_rows[-1]
        trend = {
            "point_lookup": "Flat to slightly decreasing",
            "fixed_window": "Nearly flat",
            "proportional_range_10": "Strong growth with dataset size",
            "proportional_range_50": "Very strong growth with dataset size",
            "full_range": "Steep scan-heavy growth",
            "hybrid_baseline_lookup": "Steep scan-heavy growth, highest variance",
        }[query_type]
        lines.append(
            f"| {query_type} | {smallest['p50_latency_ms']} ms at {smallest['dataset_size_quads']} quads | "
            f"{largest['p50_latency_ms']} ms at {largest['dataset_size_quads']} quads | {trend} |"
        )
    return "\n".join(lines)


def write_tables_md(output_dir: Path, subquery_blocks: Dict[str, Dict[str, str]], hybrid_rows: List[Dict[str, str]], scaling_rows: List[Dict[str, str]]) -> Path:
    path = output_dir / "janus_benchmark_tables.md"
    content = "\n\n".join(
        [
            "# Janus Benchmark Tables",
            "## Hybrid coordination summary",
            build_hybrid_table(hybrid_rows),
            "## Nested historical subquery comparison",
            build_subquery_table(subquery_blocks),
            "## Historical scaling selected rows",
            build_scaling_table(scaling_rows),
        ]
    )
    path.write_text(content + "\n", encoding="utf-8")
    return path


def main() -> int:
    args = parse_args()
    ensure_output_dir(args.output_dir)

    hybrid_rows = read_csv_rows(args.hybrid_csv)
    scaling_rows = read_csv_rows(args.scaling_csv)
    subquery_blocks = parse_subquery_markdown(args.subquery_md)

    generated_paths: List[Path] = []
    generated_paths.extend(plot_historical_scaling(scaling_rows, args.output_dir))
    generated_paths.extend(plot_hybrid_latency(hybrid_rows, args.output_dir))
    generated_paths.extend(plot_hybrid_transfer(hybrid_rows, args.output_dir))
    generated_paths.append(write_tables_md(args.output_dir, subquery_blocks, hybrid_rows, scaling_rows))

    for path in generated_paths:
        print(path)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
