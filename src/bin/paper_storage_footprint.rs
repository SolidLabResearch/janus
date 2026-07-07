use clap::Parser;
use janus::{
    core::RDFEvent,
    paper_bench::harness::citybench_event,
    storage::{segmented_storage::StreamingSegmentedStorage, util::StreamingConfig},
};
use oxigraph::{
    model::{GraphName, NamedNode, Quad, Term},
    store::Store,
};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

const DEFAULT_EVENT_COUNTS: [usize; 5] = [10_000, 50_000, 100_000, 500_000, 1_000_000];
const BASE_TIMESTAMP_MS: u64 = 1_800_500_000_000;
const HISTORICAL_INTERVAL_MS: u64 = 60;
const RESULTS_CSV_PATH: &str = "results/paper_storage_footprint.csv";

#[derive(Parser, Debug)]
#[command(name = "paper_storage_footprint")]
#[command(about = "Measure Janus vs Oxigraph on-disk historical RDF event-log footprint")]
struct Args {
    /// Comma-separated historical event counts to benchmark.
    #[arg(long, default_value = "10000,50000,100000,500000,1000000")]
    event_counts: String,
}

#[derive(Debug, Clone)]
struct StorageFootprintRow {
    event_count: usize,
    system: &'static str,
    storage_bytes: u64,
    storage_mb: f64,
    bytes_per_event: f64,
    load_time_ms: f64,
    events_per_second: f64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let event_counts = parse_event_counts(&args.event_counts)?;
    let csv_path = PathBuf::from(RESULTS_CSV_PATH);
    if let Some(parent) = csv_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut rows = Vec::new();
    for event_count in event_counts {
        let size_root = PathBuf::from(format!("target/paper-storage-footprint/{event_count}"));
        if size_root.exists() {
            fs::remove_dir_all(&size_root)?;
        }
        let janus_dir = size_root.join("janus");
        let oxigraph_dir = size_root.join("oxigraph");
        fs::create_dir_all(&janus_dir)?;
        fs::create_dir_all(&oxigraph_dir)?;

        let events = build_historical_events(event_count);

        println!("Loading {event_count} events into Janus...");
        rows.push(benchmark_janus(event_count, &events, &janus_dir)?);

        println!("Loading {event_count} events into Oxigraph...");
        rows.push(benchmark_oxigraph(event_count, &events, &oxigraph_dir)?);
    }

    write_csv(&csv_path, &rows)?;
    print_summary(&rows);
    println!("\nSaved CSV to {}", csv_path.display());
    Ok(())
}

fn parse_event_counts(raw: &str) -> Result<Vec<usize>, Box<dyn Error>> {
    let values = raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::parse::<usize>)
        .collect::<Result<Vec<_>, _>>()?;
    if values.is_empty() {
        return Err("event_counts must not be empty".into());
    }
    for value in &values {
        if !DEFAULT_EVENT_COUNTS.contains(value) {
            return Err(format!(
                "unsupported event count {value}; expected one of 10000, 50000, 100000, 500000, 1000000"
            )
            .into());
        }
    }
    Ok(values)
}

fn build_historical_events(event_count: usize) -> Vec<RDFEvent> {
    (0..event_count)
        .map(|index| {
            let timestamp = BASE_TIMESTAMP_MS + index as u64 * HISTORICAL_INTERVAL_MS;
            citybench_event(timestamp, index)
        })
        .collect()
}

fn benchmark_janus(
    event_count: usize,
    events: &[RDFEvent],
    storage_dir: &Path,
) -> Result<StorageFootprintRow, Box<dyn Error>> {
    let load_start = Instant::now();
    {
        let storage = StreamingSegmentedStorage::new(config_for_path(storage_dir))?;
        for event in events {
            storage.write_rdf_event(event.clone())?;
        }
        storage.flush()?;
    }
    let load_time_ms = load_start.elapsed().as_secs_f64() * 1000.0;
    let storage_bytes = directory_size_bytes(storage_dir)?;
    Ok(build_row(event_count, "janus", storage_bytes, load_time_ms))
}

fn benchmark_oxigraph(
    event_count: usize,
    events: &[RDFEvent],
    storage_dir: &Path,
) -> Result<StorageFootprintRow, Box<dyn Error>> {
    let load_start = Instant::now();
    {
        let store = Store::open(storage_dir)?;
        let timestamp_predicate = NamedNode::new("http://example.org/timestamp")?;
        let graph_predicate = NamedNode::new("http://example.org/graph")?;
        let decimal_datatype = NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal")?;
        let integer_datatype = NamedNode::new("http://www.w3.org/2001/XMLSchema#integer")?;

        let mut loader = store.bulk_loader();
        loader.load_ok_quads::<_, oxigraph::store::StorageError>(
            events.iter().enumerate().flat_map(|(index, event)| {
                let event_graph_uri = format!("http://example.org/event/{index}");
                let event_graph_node = NamedNode::new_unchecked(event_graph_uri.clone());
                let event_graph = GraphName::NamedNode(event_graph_node.clone());

                let subject = NamedNode::new_unchecked(event.subject.clone());
                let predicate = NamedNode::new_unchecked(event.predicate.clone());
                let object = Term::Literal(oxigraph::model::Literal::new_typed_literal(
                    event.object.clone(),
                    decimal_datatype.clone(),
                ));
                let graph_quad = Quad::new(subject, predicate, object, event_graph);

                let ts_quad = Quad::new(
                    event_graph_node.clone(),
                    timestamp_predicate.clone(),
                    Term::Literal(oxigraph::model::Literal::new_typed_literal(
                        event.timestamp.to_string(),
                        integer_datatype.clone(),
                    )),
                    GraphName::DefaultGraph,
                );

                let source_graph_quad = Quad::new(
                    event_graph_node,
                    graph_predicate.clone(),
                    Term::NamedNode(NamedNode::new_unchecked(event.graph.clone())),
                    GraphName::DefaultGraph,
                );

                [
                    Ok::<Quad, oxigraph::store::StorageError>(graph_quad),
                    Ok::<Quad, oxigraph::store::StorageError>(ts_quad),
                    Ok::<Quad, oxigraph::store::StorageError>(source_graph_quad),
                ]
            }),
        )?;
        loader.commit()?;
        store.flush()?;
    }
    let load_time_ms = load_start.elapsed().as_secs_f64() * 1000.0;
    let storage_bytes = directory_size_bytes(storage_dir)?;
    Ok(build_row(event_count, "oxigraph", storage_bytes, load_time_ms))
}

fn build_row(
    event_count: usize,
    system: &'static str,
    storage_bytes: u64,
    load_time_ms: f64,
) -> StorageFootprintRow {
    let load_time_seconds = load_time_ms / 1000.0;
    let events_per_second = if load_time_seconds > 0.0 {
        event_count as f64 / load_time_seconds
    } else {
        0.0
    };
    StorageFootprintRow {
        event_count,
        system,
        storage_bytes,
        storage_mb: storage_bytes as f64 / 1024.0 / 1024.0,
        bytes_per_event: storage_bytes as f64 / event_count as f64,
        load_time_ms,
        events_per_second,
    }
}

fn config_for_path(storage_dir: &Path) -> StreamingConfig {
    StreamingConfig {
        segment_base_path: storage_dir.display().to_string(),
        max_batch_events: 1_000_000,
        max_batch_age_seconds: 3600,
        max_batch_bytes: 1_000_000_000,
        sparse_interval: 64,
        entries_per_index_block: 256,
    }
}

fn directory_size_bytes(path: &Path) -> Result<u64, Box<dyn Error>> {
    let mut total = 0_u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            total += directory_size_bytes(&entry.path())?;
        } else {
            total += metadata.len();
        }
    }
    Ok(total)
}

fn write_csv(csv_path: &Path, rows: &[StorageFootprintRow]) -> Result<(), Box<dyn Error>> {
    let mut output = String::from(
        "event_count,system,storage_bytes,storage_mb,bytes_per_event,load_time_ms,events_per_second\n",
    );
    for row in rows {
        output.push_str(&format!(
            "{},{},{},{:.6},{:.6},{:.3},{:.3}\n",
            row.event_count,
            row.system,
            row.storage_bytes,
            row.storage_mb,
            row.bytes_per_event,
            row.load_time_ms,
            row.events_per_second
        ));
    }
    fs::write(csv_path, output)?;
    Ok(())
}

fn print_summary(rows: &[StorageFootprintRow]) {
    println!(
        "\n{:<12} {:<10} {:>14} {:>12} {:>16} {:>14} {:>18}",
        "event_count",
        "system",
        "storage_bytes",
        "storage_mb",
        "bytes_per_event",
        "load_time_ms",
        "events_per_second"
    );
    println!("{}", "-".repeat(104));
    for row in rows {
        println!(
            "{:<12} {:<10} {:>14} {:>12.3} {:>16.3} {:>14.3} {:>18.3}",
            row.event_count,
            row.system,
            row.storage_bytes,
            row.storage_mb,
            row.bytes_per_event,
            row.load_time_ms,
            row.events_per_second
        );
    }
}
