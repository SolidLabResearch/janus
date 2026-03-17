#!/usr/bin/env python3
"""
Convert RDF files (.ttl, .n3) to N-Quads format with timestamps.

Usage:
    python3 convert_to_nquads.py <input_dir> <output_file>

This script:
1. Reads all .ttl and .n3 files from input_dir
2. Extracts or assigns timestamps to each quad
3. Sorts by timestamp
4. Writes to output_file in N-Quads format
"""

import sys
import os
from pathlib import Path
from datetime import datetime, timedelta
import re
from rdflib import Graph, Namespace, Literal, URIRef
import json

def extract_timestamp_from_triple(subject, predicate, obj, default_ts_ms):
    """
    Try to extract timestamp from object if predicate is dc:date or ssn:observationResultTime.
    Returns timestamp in milliseconds.
    """
    dc = Namespace("http://purl.org/dc/elements/1.1/")
    ssn = Namespace("http://purl.oclc.org/NET/ssnx/ssn#")

    # Check if predicate is a timestamp/date predicate
    if predicate in [dc.date, ssn.observationResultTime]:
        try:
            if isinstance(obj, Literal):
                obj_str = str(obj)
                # Try to parse as ISO datetime
                for fmt in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d"]:
                    try:
                        dt = datetime.strptime(obj_str, fmt)
                        # Convert to milliseconds since epoch
                        return int(dt.timestamp() * 1000)
                    except ValueError:
                        continue
                # Try ISO format with timezone
                if "T" in obj_str:
                    try:
                        # Handle ISO format with Z or +00:00
                        obj_str_clean = obj_str.replace("Z", "+00:00")
                        # Python 3.7+ supports fromisoformat
                        dt = datetime.fromisoformat(obj_str_clean)
                        return int(dt.timestamp() * 1000)
                    except:
                        pass
        except Exception as e:
            pass

    return default_ts_ms

def convert_to_nquads(input_dir, output_file):
    """
    Convert RDF files in input_dir to N-Quads format.
    """
    input_path = Path(input_dir)
    if not input_path.exists():
        print(f"ERROR: Input directory '{input_dir}' does not exist")
        return False

    # Find all RDF files
    rdf_files = list(input_path.glob("**/*.ttl")) + list(input_path.glob("**/*.n3"))
    if not rdf_files:
        print(f"WARNING: No .ttl or .n3 files found in '{input_dir}'")
        return False

    print(f"Found {len(rdf_files)} RDF files")

    quads = []
    base_timestamp = int(datetime(2015, 1, 1, 0, 0, 0).timestamp() * 1000)  # 2015-01-01 00:00:00 UTC
    next_sequential_ts = base_timestamp
    increment_ms = 300000  # 5 minutes = 300,000 ms

    citybench_ns = Namespace("http://citybench.org/")

    for i, rdf_file in enumerate(rdf_files):
        print(f"  [{i+1}/{len(rdf_files)}] Processing {rdf_file.name}...", end=" ", flush=True)

        try:
            g = Graph()
            g.parse(str(rdf_file), format='ttl')

            file_graph = URIRef(f"http://citybench.org/graph/{rdf_file.stem}")

            triples_in_file = 0
            for s, p, o in g:
                # Extract or assign timestamp
                ts_ms = extract_timestamp_from_triple(s, p, o, next_sequential_ts)

                # If no timestamp was extracted, use sequential
                if ts_ms == next_sequential_ts or ts_ms < base_timestamp:
                    ts_ms = next_sequential_ts
                    next_sequential_ts += increment_ms

                quads.append((ts_ms, s, p, o, file_graph))
                triples_in_file += 1

            print(f"{triples_in_file} quads")
        except Exception as e:
            print(f"ERROR: {e}")
            continue

    if not quads:
        print("ERROR: No quads extracted from input files")
        return False

    print(f"\nTotal quads: {len(quads)}")
    print("Sorting by timestamp...")
    quads.sort(key=lambda q: q[0])

    print(f"Writing to {output_file}...")
    try:
        with open(output_file, 'w', encoding='utf-8') as f:
            for ts_ms, s, p, o, g in quads:
                # Format as N-Quads: <s> <p> <o> <g> .
                s_str = format_term(s)
                p_str = format_term(p)
                o_str = format_term(o)
                g_str = format_term(g)
                f.write(f"{s_str} {p_str} {o_str} {g_str} .\n")

        print(f"✓ Wrote {len(quads)} quads to {output_file}")
        return True
    except Exception as e:
        print(f"ERROR: Failed to write output: {e}")
        return False

def format_term(term):
    """Format RDF term as N-Quads string."""
    if isinstance(term, URIRef):
        return f"<{term}>"
    elif isinstance(term, Literal):
        # Format literal with language or datatype
        if term.language:
            return f'"{escape_string(str(term))}"@{term.language}'
        elif term.datatype:
            return f'"{escape_string(str(term))}"^^<{term.datatype}>'
        else:
            return f'"{escape_string(str(term))}"'
    else:
        # Blank node
        return f"_:{term}"

def escape_string(s):
    """Escape special characters in string literals."""
    s = s.replace("\\", "\\\\")
    s = s.replace('"', '\\"')
    s = s.replace("\n", "\\n")
    s = s.replace("\r", "\\r")
    s = s.replace("\t", "\\t")
    return s

if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(1)

    input_dir = sys.argv[1]
    output_file = sys.argv[2]

    success = convert_to_nquads(input_dir, output_file)
    sys.exit(0 if success else 1)
