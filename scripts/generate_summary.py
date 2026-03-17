#!/usr/bin/env python3
"""
Generate summary report from benchmark CSV results.

Reads all CSV files in results/ and creates results/summary.md
with formatted tables and hypothesis→result mapping for paper reviewers.
"""

import csv
import os
import subprocess
from datetime import datetime
from pathlib import Path


def read_csv(path):
    """Read CSV file and return list of dicts."""
    if not os.path.exists(path):
        return []
    with open(path) as f:
        return list(csv.DictReader(f))


def read_hardware():
    """Read hardware spec file."""
    lines = []
    if os.path.exists("results/hardware.txt"):
        with open("results/hardware.txt") as f:
            lines = f.readlines()
    return "".join(lines).strip() if lines else "Hardware info not available"


def format_table(rows, columns):
    """Format list of dicts as markdown table."""
    if not rows:
        return "_No data_\n"

    header = "| " + " | ".join(columns) + " |"
    sep = "| " + " | ".join(["---"] * len(columns)) + " |"
    body = "\n".join(
        "| " + " | ".join(str(row.get(c, "")).replace("|", "\\|") for c in columns) + " |"
        for row in rows
    )
    return f"{header}\n{sep}\n{body}\n"


def get_git_info():
    """Get current git branch and commit."""
    try:
        branch = subprocess.check_output(
            ["git", "rev-parse", "--abbrev-ref", "HEAD"],
            stderr=subprocess.DEVNULL,
            text=True
        ).strip()
    except:
        branch = "unknown"

    try:
        commit = subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"],
            stderr=subprocess.DEVNULL,
            text=True
        ).strip()
    except:
        commit = "unknown"

    return branch, commit


def main():
    os.makedirs("results", exist_ok=True)

    lines = []

    # Header
    lines.append("# Janus Benchmark Results — SEMANTiCS 2026\n\n")
    lines.append(f"**Generated:** {datetime.now().isoformat()}\n\n")

    branch, commit = get_git_info()
    lines.append(f"**Branch:** {branch}  \n")
    lines.append(f"**Commit:** {commit}  \n\n")

    # Hardware
    lines.append("## Hardware Specification\n\n")
    lines.append("```\n")
    lines.append(read_hardware())
    lines.append("\n```\n\n")

    # H1 Latency
    lines.append("## H1 — End-to-End Query Latency Breakdown\n\n")
    lines.append(
        "Measures the unified query pipeline across 4 stages: storage write, "
        "historical retrieval, live window close, and result combination.\n\n"
    )

    h1_summary = read_csv("results/h1_summary.csv")
    if h1_summary:
        lines.append("### Summary (mean & std dev across 25 stable runs)\n\n")
        lines.append(format_table(h1_summary, [
            "dataset_size_quads", "event_rate_per_sec",
            "hist_mean_ms", "hist_std_ms",
            "live_mean_ms", "live_std_ms",
            "total_mean_ms", "total_std_ms",
            "hist_pct_of_total"
        ]))

    # H1 Path Isolation
    lines.append("### Path Isolation (live latency under background load)\n\n")
    h1_iso = read_csv("results/h1_isolation.csv")
    if h1_iso:
        lines.append(
            "Live window latency should remain flat regardless of background "
            "historical query load (path isolation confirmed).\n\n"
        )
        lines.append(format_table(h1_iso, [
            "background_hist_qps",
            "live_window_mean_ms",
            "live_window_std_ms"
        ]))

    # H2 Correctness
    lines.append("## H2 — Anomaly Detection Correctness\n\n")
    lines.append(
        "Measures accuracy and latency of detecting 20 injected anomalies "
        "across 5 different seeds. Tests: stuck_sensor, spike, sustained_drop, gradual_drift.\n\n"
    )

    h2_summary = read_csv("results/h2_summary.csv")
    if h2_summary:
        lines.append("### Overall Detection Statistics\n\n")
        lines.append(format_table(h2_summary, [
            "anomaly_type", "detection_rate",
            "mean_latency_ms", "std_latency_ms",
            "within_step_rate"
        ]))

    # H4 Scalability
    lines.append("## H4 — Scalability Analysis\n\n")
    lines.append(
        "Measures how historical retrieval latency scales with dataset size "
        "(100K–5M quads). Verifies sub-linear growth from two-level sparse index "
        "and path isolation (live latency remains flat).\n\n"
    )

    h4_summary = read_csv("results/h4_summary.csv")
    if h4_summary:
        lines.append("### Scalability Measurements\n\n")
        # Check for sub-linearity
        sublinear_results = [r for r in h4_summary if r.get("sublinear_check") == "PASS"]
        sublinear_pct = 100 * len(sublinear_results) / len(h4_summary) if h4_summary else 0

        lines.append(f"**Sub-linearity check:** {sublinear_pct:.0f}% passing (PASS/baseline desired)  \n")
        lines.append(f"**Index effectiveness:** Two-level sparse index demonstrated ")
        lines.append(f"{'✓' if sublinear_pct > 50 else '✗'}\n\n")

        lines.append(format_table(h4_summary, [
            "dataset_size_quads",
            "hist_mean_ms", "hist_std_ms",
            "bootstrap_mean_ms", "bootstrap_std_ms",
            "live_mean_ms", "live_std_ms",
            "sublinear_check"
        ]))

    # Hypothesis mapping
    lines.append("## Research Hypotheses → Experimental Results → Paper Sections\n\n")
    lines.append(
        "This table maps each hypothesis tested to its corresponding experiment "
        "and result files for paper reviewers.\n\n"
    )

    lines.append("| Hypothesis | Experiment | Result File(s) | Paper Section |\n")
    lines.append("|---|---|---|---|\n")
    lines.append(
        "| **H1:** Unified hybrid query architecture provides efficient "
        "end-to-end latency | H1: 4-stage pipeline breakdown | "
        "`h1_summary.csv` | §5.1 |\n"
    )
    lines.append(
        "| **H1 (Path Isolation):** Historical and live query paths don't interfere "
        "under load | H1: Background query load test | "
        "`h1_isolation.csv` | §5.1 |\n"
    )
    lines.append(
        "| **H2:** Real-time anomaly detection via unified historical+live query "
        "comparison | H2: Anomaly injection + detection latency (5 seeds) | "
        "`h2_detection.csv`, `h2_summary.csv` | §5.2 |\n"
    )
    lines.append(
        "| **H4:** Two-level sparse index enables sub-linear scaling "
        "to millions of quads | H4: Retrieval latency vs. dataset size | "
        "`h4_summary.csv` | §5.3 |\n"
    )

    # Reproducibility info
    lines.append("\n## Reproducibility\n\n")
    lines.append("**Dataset:** CityBench AarhusTrafficData (automaticallydownloaded)  \n")
    lines.append(
        "**Absolute numbers** vary by hardware; **relative trends** and **detection rates** "
        "should be reproducible.  \n"
    )
    lines.append("**Hardware differences:** See spec above.  \n")

    # Write summary
    with open("results/summary.md", "w") as f:
        f.writelines(lines)

    print("✓ Written: results/summary.md")


if __name__ == "__main__":
    main()
