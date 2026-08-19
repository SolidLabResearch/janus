# Stream Bus CLI

`stream_bus_cli` replays RDF events from an N-Triples or N-Quads file into
Janus storage and, optionally, an MQTT broker.

```bash
cargo run --bin stream_bus_cli -- --help
```

## Example

```bash
cargo run --bin stream_bus_cli -- \
  --input data/sensors.nq \
  --broker mqtt \
  --topics sensors \
  --rate 64 \
  --mqtt-host localhost \
  --mqtt-port 1883 \
  --storage-path data/stream_bus_storage
```

## Options

| Option | Meaning |
| --- | --- |
| `--input <path>` | Required N-Triples or N-Quads source file. |
| `--broker mqtt|none` | Publish to MQTT, or only write to storage. |
| `--topics <a,b>` | Comma-separated MQTT topics; default `sensors`. |
| `--rate <Hz>` | Target publish rate; `0` means unlimited. |
| `--loop-file` | Replay the input repeatedly. |
| `--add-timestamps` | Add timestamps when they are absent. |
| `--mqtt-host`, `--mqtt-port` | MQTT connection settings. |
| `--storage-path` | Segmented-storage destination. |

The CLI reports read, published, and stored event counts plus errors and
throughput on completion. See [Live Streaming](./LIVE_STREAMING_GUIDE.md) for
the end-to-end broker and query workflow.
