#!/bin/bash
# Downloads CityBench AarhusTrafficData and converts to N-Quads

set -e

WORK_DIR="data/citybench"
ARCHIVE="AarhusTrafficData.tar.gz"
ARCHIVE_URL="http://www.ict-citypulse.eu/citybench/AarhusTrafficData.tar.gz"

echo "Creating citybench directory..."
mkdir -p "$WORK_DIR"

echo "Downloading CityBench AarhusTrafficData..."
if ! curl -L "$ARCHIVE_URL" -o "$WORK_DIR/$ARCHIVE"; then
    echo "ERROR: Failed to download CityBench dataset from $ARCHIVE_URL"
    echo "Falling back to generating synthetic dataset..."
    python3 scripts/generate_realistic_data.py --size 100000 --output "$WORK_DIR/aarhus_traffic.nq"
    exit 0
fi

echo "Extracting archive..."
tar -xzf "$WORK_DIR/$ARCHIVE" -C "$WORK_DIR/" || {
    echo "ERROR: Failed to extract archive"
    exit 1
}

echo "Converting to N-Quads format..."
python3 scripts/convert_to_nquads.py \
    "$WORK_DIR/AarhusTrafficData/" \
    "$WORK_DIR/aarhus_traffic.nq" || {
    echo "ERROR: Failed to convert to N-Quads"
    exit 1
}

echo "Creating dataset slices..."

TOTAL_LINES=$(wc -l < "$WORK_DIR/aarhus_traffic.nq")
HIST_LINES=$((TOTAL_LINES * 25 / 30))  # First 25 out of 30 minutes
LIVE_START=$((HIST_LINES + 1))
LIVE_END=$((TOTAL_LINES))

# Historical slice (first 25 minutes)
head -n "$HIST_LINES" "$WORK_DIR/aarhus_traffic.nq" > "$WORK_DIR/historical_25min.nq"
echo "Created historical_25min.nq with $HIST_LINES events"

# Live slice (next 5 minutes)
if [ "$LIVE_END" -ge "$LIVE_START" ]; then
    sed -n "${LIVE_START},${LIVE_END}p" "$WORK_DIR/aarhus_traffic.nq" > "$WORK_DIR/live_5min.nq"
    LIVE_LINES=$((LIVE_END - LIVE_START + 1))
    echo "Created live_5min.nq with $LIVE_LINES events"
fi

# Full dataset
cp "$WORK_DIR/aarhus_traffic.nq" "$WORK_DIR/full.nq"
echo "Created full.nq with $TOTAL_LINES events"

echo "✓ CityBench dataset preparation complete"
