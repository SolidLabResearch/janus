#!/usr/bin/env python3
"""
Injects synthetic anomalies into N-Quads RDF stream data.

Usage:
    python3 inject_anomalies.py --input <nquads_file> --spec <spec_json> \
        --output <output_nquads> --ground-truth <truth_json>

The anomaly spec JSON defines:
- stuck_sensor: sensor holds constant value for duration_ms
- spike: single value multiplied by multiplier
- sustained_drop: value reduced by drop_fraction for duration_ms
- gradual_drift: value increments by drift_per_step over duration_ms
"""

import sys
import json
import argparse
from pathlib import Path
from datetime import datetime
from typing import List, Dict, Tuple, Optional

def parse_nquads(file_path: str) -> List[Tuple[str, int]]:
    """Parse N-Quads file into list of (line, timestamp) tuples.

    Extracts timestamp from graph URI pattern: <...#ts_123456789>
    Falls back to 0 if no timestamp found.
    """
    lines = []
    with open(file_path, 'r', encoding='utf-8') as f:
        for line in f:
            line = line.rstrip('\n')
            if not line or line.startswith('#'):
                continue

            # Try to extract timestamp from the last term (graph URI)
            ts = extract_timestamp_from_line(line)
            lines.append((line, ts))

    return lines

def extract_timestamp_from_line(line: str) -> int:
    """Extract timestamp from N-Quads line.

    Looks for timestamps in quoted strings or URIs in the quad.
    Very permissive - returns 0 if no timestamp found.
    """
    try:
        # Look for numeric timestamps in the line
        # Example: <http://example.org/obs/123456> or similar
        import re
        matches = re.findall(r'["\']?(\d{13})["\']?', line)
        if matches:
            return int(matches[0])
    except:
        pass
    return 0

def apply_anomalies(lines: List[Tuple[str, int]], anomaly_spec: Dict) -> Tuple[List[str], Dict]:
    """Apply anomalies to the data.

    Returns:
        - Modified lines (list of strings)
        - Ground truth mapping (dict with anomaly detection info)
    """
    # Create working copy
    modified_lines = [line for line, _ in lines]
    ground_truth = {"anomalies": []}

    # Build index of lines by sensor URI for quick lookup
    sensor_lines: Dict[str, List[int]] = {}
    for i, (line, _) in enumerate(lines):
        sensor_uri = extract_sensor_uri(line)
        if sensor_uri:
            if sensor_uri not in sensor_lines:
                sensor_lines[sensor_uri] = []
            sensor_lines[sensor_uri].append(i)

    # Apply each anomaly
    for anomaly in anomaly_spec.get("anomalies", []):
        anomaly_id = anomaly.get("id", "unknown")
        anomaly_type = anomaly.get("type")
        sensor_uri = anomaly.get("sensor_uri")
        value_predicate = anomaly.get("value_predicate")

        if sensor_uri not in sensor_lines:
            print(f"WARNING: Sensor {sensor_uri} not found in data (anomaly {anomaly_id})")
            continue

        detection_timestamp = None

        if anomaly_type == "stuck_sensor":
            start_ts = anomaly.get("start_timestamp")
            duration_ms = anomaly.get("duration_ms")
            stuck_value = anomaly.get("stuck_value")

            for line_idx in sensor_lines[sensor_uri]:
                line = modified_lines[line_idx]
                line_ts = extract_timestamp_from_line(line)

                if start_ts <= line_ts < (start_ts + duration_ms):
                    modified_lines[line_idx] = replace_object_value(line, stuck_value, value_predicate)
                    if detection_timestamp is None:
                        detection_timestamp = line_ts

        elif anomaly_type == "spike":
            timestamp = anomaly.get("timestamp")
            multiplier = anomaly.get("multiplier")

            for line_idx in sensor_lines[sensor_uri]:
                line = modified_lines[line_idx]
                line_ts = extract_timestamp_from_line(line)

                if line_ts == timestamp:
                    modified_lines[line_idx] = multiply_object_value(line, multiplier, value_predicate)
                    detection_timestamp = timestamp

        elif anomaly_type == "sustained_drop":
            start_ts = anomaly.get("start_timestamp")
            duration_ms = anomaly.get("duration_ms")
            drop_fraction = anomaly.get("drop_fraction")

            for line_idx in sensor_lines[sensor_uri]:
                line = modified_lines[line_idx]
                line_ts = extract_timestamp_from_line(line)

                if start_ts <= line_ts < (start_ts + duration_ms):
                    modified_lines[line_idx] = reduce_object_value(
                        line, drop_fraction, value_predicate
                    )
                    if detection_timestamp is None:
                        detection_timestamp = line_ts

        elif anomaly_type == "gradual_drift":
            start_ts = anomaly.get("start_timestamp")
            duration_ms = anomaly.get("duration_ms")
            drift_per_step = anomaly.get("drift_per_step")

            step_count = 0
            for line_idx in sensor_lines[sensor_uri]:
                line = modified_lines[line_idx]
                line_ts = extract_timestamp_from_line(line)

                if start_ts <= line_ts < (start_ts + duration_ms):
                    drift = drift_per_step * step_count
                    modified_lines[line_idx] = add_to_object_value(line, drift, value_predicate)
                    step_count += 1
                    if detection_timestamp is None:
                        detection_timestamp = line_ts

        # Record in ground truth
        ground_truth["anomalies"].append({
            "id": anomaly_id,
            "type": anomaly_type,
            "injection_timestamp": detection_timestamp or anomaly.get("timestamp", anomaly.get("start_timestamp")),
            "sensor": sensor_uri
        })

    return modified_lines, ground_truth

def extract_sensor_uri(line: str) -> Optional[str]:
    """Extract sensor URI (subject) from N-Quads line."""
    try:
        # Subject is first term
        if line.startswith('<'):
            end = line.find('>')
            if end > 0:
                return line[1:end]
    except:
        pass
    return None

def replace_object_value(line: str, new_value: str, value_pred: str) -> str:
    """Replace numeric object value in line if predicate matches."""
    # Very simple implementation - check if line contains value_pred
    if value_pred in line:
        # Find last numeric literal and replace it
        import re
        # Match quoted numbers or plain numbers before space and dot
        matches = list(re.finditer(r'"([\d.e\-+]+)"(?=[^"]*\.?\s*$)', line))
        if matches:
            last_match = matches[-1]
            return (
                line[:last_match.start()] +
                f'"{new_value}"' +
                line[last_match.end():]
            )
    return line

def multiply_object_value(line: str, multiplier: float, value_pred: str) -> str:
    """Multiply numeric object value if predicate matches."""
    if value_pred in line:
        import re
        matches = list(re.finditer(r'"([\d.e\-+]+)"(?=[^"]*\.?\s*$)', line))
        if matches:
            last_match = matches[-1]
            try:
                original_value = float(last_match.group(1))
                new_value = original_value * multiplier
                return (
                    line[:last_match.start()] +
                    f'"{new_value}"' +
                    line[last_match.end():]
                )
            except ValueError:
                pass
    return line

def reduce_object_value(line: str, drop_fraction: float, value_pred: str) -> str:
    """Reduce numeric object value by drop_fraction if predicate matches."""
    if value_pred in line:
        import re
        matches = list(re.finditer(r'"([\d.e\-+]+)"(?=[^"]*\.?\s*$)', line))
        if matches:
            last_match = matches[-1]
            try:
                original_value = float(last_match.group(1))
                new_value = original_value * (1 - drop_fraction)
                return (
                    line[:last_match.start()] +
                    f'"{new_value}"' +
                    line[last_match.end():]
                )
            except ValueError:
                pass
    return line

def add_to_object_value(line: str, increment: float, value_pred: str) -> str:
    """Add increment to numeric object value if predicate matches."""
    if value_pred in line:
        import re
        matches = list(re.finditer(r'"([\d.e\-+]+)"(?=[^"]*\.?\s*$)', line))
        if matches:
            last_match = matches[-1]
            try:
                original_value = float(last_match.group(1))
                new_value = original_value + increment
                return (
                    line[:last_match.start()] +
                    f'"{new_value}"' +
                    line[last_match.end():]
                )
            except ValueError:
                pass
    return line

def main():
    parser = argparse.ArgumentParser(
        description="Inject synthetic anomalies into N-Quads RDF stream data"
    )
    parser.add_argument("--input", required=True, help="Input N-Quads file")
    parser.add_argument("--spec", required=True, help="Anomaly specification JSON file")
    parser.add_argument("--output", required=True, help="Output N-Quads file (with anomalies)")
    parser.add_argument("--ground-truth", required=True, help="Output ground truth JSON file")

    args = parser.parse_args()

    # Load input
    print(f"Loading {args.input}...")
    lines = parse_nquads(args.input)
    print(f"  Loaded {len(lines)} quads")

    # Load anomaly spec
    print(f"Loading anomaly spec from {args.spec}...")
    with open(args.spec, 'r') as f:
        spec = json.load(f)
    print(f"  Loaded {len(spec.get('anomalies', []))} anomalies")

    # Apply anomalies
    print("Applying anomalies...")
    modified_lines, ground_truth = apply_anomalies(lines, spec)

    # Write modified data
    print(f"Writing modified data to {args.output}...")
    with open(args.output, 'w', encoding='utf-8') as f:
        for line in modified_lines:
            if line.strip():
                f.write(line + '\n')
    print(f"  Wrote {len(modified_lines)} quads")

    # Write ground truth
    print(f"Writing ground truth to {args.ground_truth}...")
    with open(args.ground_truth, 'w') as f:
        json.dump(ground_truth, f, indent=2)
    print(f"  Recorded {len(ground_truth['anomalies'])} anomaly injections")

    print("✓ Done")

if __name__ == "__main__":
    main()
