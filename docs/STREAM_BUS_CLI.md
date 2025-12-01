# Stream Bus CLI Documentation

## Overview

The Stream Bus CLI is a command-line tool for reading RDF data from files and publishing to Kafka/MQTT brokers while simultaneously storing in Janus's segmented storage system.

## Features

- Read RDF data from N-Triples/N-Quads files
- Publish to Kafka or MQTT brokers
- Write to Janus streaming storage
- Configurable replay rates (e.g., 64Hz for realistic streaming)
- File looping for continuous replay
- Automatic timestamp generation
- Multiple topic support
- Comprehensive metrics reporting

## Installation

Build the CLI from source:

```bash
cd janus
cargo build --release --bin stream_bus_cli
```

The binary will be available at `target/release/stream_bus_cli`.

## Usage

### Basic Syntax

```bash
stream_bus_cli --input <FILE> [OPTIONS]
```

### Required Arguments

- `--input, -i <FILE>` - Path to input RDF file (N-Triples or N-Quads format)

### Optional Arguments

- `--broker, -b <TYPE>` - Broker type: `kafka`, `mqtt`, or `none` (default: kafka)
- `--topics, -t <TOPICS>` - Comma-separated list of topics (default: sensors)
- `--rate, -r <HZ>` - Publishing rate in Hz, 0 for unlimited (default: 64)
- `--loop-file` - Loop the file indefinitely
- `--add-timestamps` - Add timestamps if not present in data
- `--kafka-servers <SERVERS>` - Kafka bootstrap servers (default: localhost:9092)
- `--mqtt-host <HOST>` - MQTT broker host (default: localhost)
- `--mqtt-port <PORT>` - MQTT broker port (default: 1883)
- `--storage-path <PATH>` - Storage directory path (default: data/stream_bus_storage)

## Examples

### 1. Storage Only (No Broker)

Process RDF file and store in Janus storage without publishing to any broker:

```bash
stream_bus_cli \
  --input data/sensors.nq \
  --broker none \
  --rate 0 \
  --add-timestamps
```

**Output:**
```
Stream Bus CLI
==============

Configuration:
  Input file: data/sensors.nq
  Broker: None
  Topics: ["sensors"]
  Rate: unlimited Hz
  Loop file: false
  Add timestamps: true
  Storage: data/stream_bus_storage

Starting the Stream Bus
...

Stream Bus Complete!
====================
Events read:      1000
Events published: 0 (0.0%)
Events stored:    1000 (100.0%)
Publish errors:   0
Storage errors:   0
Elapsed time:     0.01s
Throughput:       100000.0 events/sec
```

### 2. Kafka Publishing at 64Hz

Publish to Kafka topic at 64 events per second:

```bash
stream_bus_cli \
  --input data/iot_sensors.nq \
  --broker kafka \
  --topics sensors \
  --rate 64 \
  --kafka-servers localhost:9092
```

### 3. MQTT Publishing with File Loop

Continuously publish to MQTT broker, looping the file:

```bash
stream_bus_cli \
  --input data/temperature_readings.nq \
  --broker mqtt \
  --topics sensors/temperature \
  --rate 100 \
  --mqtt-host localhost \
  --mqtt-port 1883 \
  --loop-file
```

### 4. Multiple Topics

Publish to multiple Kafka topics:

```bash
stream_bus_cli \
  --input data/multi_sensor.nq \
  --broker kafka \
  --topics sensors,devices,readings \
  --rate 50
```

### 5. Custom Storage Path

Specify custom storage directory:

```bash
stream_bus_cli \
  --input data/experiment_01.nq \
  --broker none \
  --storage-path /data/experiments/exp01
```

### 6. High-Speed Replay

Process file at maximum speed (no rate limiting):

```bash
stream_bus_cli \
  --input data/large_dataset.nq \
  --broker kafka \
  --topics bulk_import \
  --rate 0
```

## Input File Format

The CLI accepts RDF data in N-Triples or N-Quads format.

### N-Quads Format (Recommended)

```ntriples
<http://example.org/sensor1> <http://example.org/temperature> "23.5" <http://example.org/graph1> .
<http://example.org/sensor2> <http://example.org/humidity> "65.2" <http://example.org/graph1> .
<http://example.org/sensor3> <http://example.org/pressure> "1013.25" <http://example.org/graph1> .
```

### N-Triples Format

```ntriples
<http://example.org/sensor1> <http://example.org/temperature> "23.5" .
<http://example.org/sensor2> <http://example.org/humidity> "65.2" .
```

### Comments and Empty Lines

Lines starting with `#` are treated as comments and skipped:

```ntriples
# This is a comment
<http://example.org/sensor1> <http://example.org/temperature> "23.5" <http://example.org/graph1> .

# Another comment
<http://example.org/sensor2> <http://example.org/humidity> "65.2" <http://example.org/graph1> .
```

## Metrics

The CLI reports comprehensive metrics upon completion:

- **Events read** - Total RDF statements read from file
- **Events published** - Successfully published to broker
- **Events stored** - Successfully written to storage
- **Publish errors** - Failed broker publish attempts
- **Storage errors** - Failed storage write attempts
- **Elapsed time** - Total processing duration
- **Throughput** - Events per second

### Success Rates

- **Publish success rate** - Percentage of events successfully published
- **Storage success rate** - Percentage of events successfully stored

## Rate Limiting

The `--rate` option controls publishing speed:

- `--rate 64` - 64 events per second (64Hz)
- `--rate 100` - 100 events per second
- `--rate 1000` - 1000 events per second
- `--rate 0` - Unlimited (maximum speed)

Rate limiting applies a consistent interval between events:

```
64 Hz = 1 event every 15.6ms
100 Hz = 1 event every 10ms
1000 Hz = 1 event every 1ms
```

## File Looping

The `--loop-file` flag enables continuous replay:

```bash
stream_bus_cli \
  --input data/sensors.nq \
  --broker kafka \
  --topics sensors \
  --rate 64 \
  --loop-file
```

The file will be read repeatedly until manually stopped (Ctrl+C).

## Timestamp Handling

### With `--add-timestamps`

Automatically adds current system timestamp to each event:

```bash
stream_bus_cli --input data/sensors.nq --add-timestamps
```

### Without `--add-timestamps`

Attempts to parse timestamp from object field. If parsing fails, uses current timestamp.

## Broker Configuration

### Kafka

```bash
stream_bus_cli \
  --input data/sensors.nq \
  --broker kafka \
  --topics sensors \
  --kafka-servers kafka1:9092,kafka2:9092,kafka3:9092
```

**Kafka Properties:**
- Bootstrap servers: Comma-separated list of brokers
- Client ID: `janus_stream_bus`
- Message timeout: 5000ms

### MQTT

```bash
stream_bus_cli \
  --input data/sensors.nq \
  --broker mqtt \
  --topics sensors/temperature \
  --mqtt-host mqtt.example.com \
  --mqtt-port 1883
```

**MQTT Properties:**
- QoS: AtLeastOnce
- Keep-alive: 30 seconds
- Client ID: `janus_stream_bus`

## Storage Configuration

The storage system uses the following settings:

- **Max batch events**: 500,000 events
- **Max batch age**: 1 second
- **Max batch bytes**: 50 MB
- **Sparse interval**: 1000 (index every 1000th event)
- **Entries per index block**: 100

Data is stored in segmented log files with two-level indexing for efficient queries.

## Error Handling

### File Not Found

```
Error: Failed to open the file: No such file or directory
```

**Solution:** Check file path and ensure it exists.

### Invalid Broker Type

```
Error: Unknown broker type: invalid_broker
Valid options: kafka, mqtt, none
```

**Solution:** Use one of the valid broker types.

### Connection Errors

Kafka/MQTT connection failures are logged but don't stop processing. Events will still be stored locally.

### Malformed RDF Lines

Invalid RDF statements are skipped with a warning:

```
Failed to parse line: <invalid line> - Error: Invalid RDF format: expected at least 4 parts, got 2
```

## Performance Benchmarks

Typical performance on modern hardware:

| Events | Rate | Throughput | Duration |
|--------|------|------------|----------|
| 1,000 | Unlimited | ~100K/sec | 0.01s |
| 10,000 | Unlimited | ~250K/sec | 0.04s |
| 100,000 | Unlimited | ~300K/sec | 0.33s |
| 1,000,000 | Unlimited | ~350K/sec | 2.85s |
| 1,000 | 64 Hz | 64/sec | 15.6s |
| 10,000 | 100 Hz | 100/sec | 100s |

## Stopping the CLI

Press `Ctrl+C` to gracefully stop the stream bus:

```
^C
Received Ctrl+C, stopping...
```

The CLI will finish processing the current event and report final metrics.

## Integration with Janus

The Stream Bus CLI integrates with other Janus components:

1. **Storage** - Events are written to segmented storage for historical queries
2. **Live Processing** - Can feed into live stream processing queries
3. **Query Engine** - Stored data can be queried via JanusQL

## Testing

Run CLI tests:

```bash
cargo test --test stream_bus_cli_test
```

The test suite includes:
- Help flag functionality
- Storage-only mode
- Rate limiting verification
- Error handling
- Configuration parsing
- Metrics calculation

## Troubleshooting

### No data in storage directory

**Cause:** Batch buffer hasn't flushed yet.

**Solution:** 
- Process more events (>500,000)
- Wait for background flush (1 second)
- Check logs for storage errors

### Low throughput

**Cause:** Rate limiting or slow disk I/O.

**Solution:**
- Use `--rate 0` for maximum speed
- Check disk performance
- Verify network connectivity (for brokers)

### High memory usage

**Cause:** Large batch buffer accumulation.

**Solution:**
- Reduce `max_batch_events` in storage config
- Process in smaller batches
- Monitor with system tools

## Advanced Usage

### Piping from Standard Input

```bash
cat data/sensors.nq | stream_bus_cli --input /dev/stdin --broker none
```

### Batch Processing Multiple Files

```bash
for file in data/*.nq; do
  stream_bus_cli --input "$file" --broker kafka --topics batch_import
done
```

### Monitoring with External Tools

```bash
stream_bus_cli --input large.nq --broker kafka 2>&1 | tee -a processing.log
```

## See Also

- [Stream Bus Module Documentation](../src/stream_bus/stream_bus.rs)
- [Janus Architecture](../ARCHITECTURE.md)
- [Benchmark Results](../BENCHMARK_RESULTS.md)
- [Getting Started Guide](../GETTING_STARTED.md)

## License

MIT License - See [LICENCE.md](../LICENCE.md)

## Contact

For questions or issues:
- Email: mailkushbisen@gmail.com
- GitHub: https://github.com/SolidLabResearch/janus