use crate::{
    core::RDFEvent,
    execution::rdf_event_to_quad,
    paper_bench::harness::{
        capture_command, citybench_event, collect_repro_metadata, ensure_output_dir, ReproMetadata,
    },
    storage::{segmented_storage::StreamingSegmentedStorage, util::StreamingConfig},
};
use oxigraph::model::{GraphName, Literal, NamedNode, Quad};
use oxigraph::store::Store;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::Instant,
};

const DEFAULT_JANUS_FLUSH_EVERY_EVENTS: usize = 100_000;
const STORAGE_MB_DIVISOR: f64 = 1024.0 * 1024.0;
const TEN_MILLION_EVENTS: usize = 10_000_000;
const BASE_TIMESTAMP_MS: u64 = 1_720_000_000_000;
const ITERATION_TIMESTAMP_STRIDE_MS: u64 = 100_000_000;
const OXIGRAPH_TIMESTAMP_PREDICATE: &str = "http://example.org/timestamp";
const OXIGRAPH_GRAPH_PREDICATE: &str = "http://example.org/graph";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

type BoxError = Box<dyn std::error::Error>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageSystemSelection {
    Janus,
    Oxigraph,
    Both,
}

impl StorageSystemSelection {
    fn systems(self) -> &'static [StorageSystem] {
        match self {
            Self::Janus => &[StorageSystem::Janus],
            Self::Oxigraph => &[StorageSystem::Oxigraph],
            Self::Both => &[StorageSystem::Janus, StorageSystem::Oxigraph],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum StorageSystem {
    Janus,
    Oxigraph,
}

impl StorageSystem {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Janus => "janus",
            Self::Oxigraph => "oxigraph",
        }
    }
}

#[derive(Clone, Debug)]
pub struct StorageFootprintConfig {
    pub event_counts: Vec<usize>,
    pub iterations: usize,
    pub output_dir: PathBuf,
    pub include_10m: bool,
    pub cleanup_runs_after_measurement: bool,
    pub system_selection: StorageSystemSelection,
}

#[derive(Clone, Debug)]
pub struct StorageFootprintOutcome {
    pub metadata: ReproMetadata,
    pub raw_rows: Vec<StorageFootprintRawRow>,
    pub summary_rows: Vec<StorageFootprintSummaryRow>,
    pub ratio_rows: Vec<StorageFootprintRatioRow>,
    pub raw_csv_path: PathBuf,
    pub summary_csv_path: PathBuf,
    pub ratio_csv_path: PathBuf,
    pub markdown_path: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
pub struct StorageFootprintRawRow {
    pub event_count: usize,
    pub iteration: usize,
    pub system: String,
    pub storage_bytes: u64,
    pub storage_mb: f64,
    pub bytes_per_event: f64,
    pub load_time_ms: f64,
    pub events_per_second: f64,
    pub path: String,
}

#[derive(Clone, Debug)]
pub struct StorageFootprintSummaryRow {
    pub event_count: usize,
    pub system: String,
    pub n: usize,
    pub median_storage_bytes: f64,
    pub median_storage_mb: f64,
    pub median_bytes_per_event: f64,
    pub mean_storage_mb: f64,
    pub std_storage_mb: f64,
    pub median_load_time_ms: f64,
    pub mean_load_time_ms: f64,
    pub std_load_time_ms: f64,
    pub median_events_per_second: f64,
}

#[derive(Clone, Debug)]
pub struct StorageFootprintRatioRow {
    pub event_count: usize,
    pub janus_median_storage_mb: f64,
    pub oxigraph_median_storage_mb: f64,
    pub janus_median_bytes_per_event: f64,
    pub oxigraph_median_bytes_per_event: f64,
    pub oxigraph_over_janus_storage_ratio: f64,
}

struct RunMeasurement {
    storage_bytes: u64,
    load_time_ms: f64,
    path: PathBuf,
}

struct RawCsvWriter {
    writer: BufWriter<File>,
}

pub fn run_storage_footprint_benchmark(
    config: &StorageFootprintConfig,
) -> Result<StorageFootprintOutcome, BoxError> {
    validate_config(config)?;
    ensure_output_dir(&config.output_dir)?;

    let metadata = collect_repro_metadata();
    let mut raw_rows = Vec::new();
    let raw_csv_path = config.output_dir.join("storage_footprint_raw.csv");
    let mut raw_csv_writer = RawCsvWriter::create(&raw_csv_path)?;

    for &event_count in &config.event_counts {
        for iteration in 1..=config.iterations {
            for &system in config.system_selection.systems() {
                let run_dir = config.output_dir.join("runs").join(format!(
                    "{}_events_{}_iter_{}",
                    system.as_str(),
                    event_count,
                    iteration
                ));
                let store_dir = run_dir.join("store");
                let measurement =
                    run_single_measurement(system, event_count, iteration, &store_dir)?;
                let storage_mb = bytes_to_mb(measurement.storage_bytes);
                let load_time_seconds = measurement.load_time_ms / 1000.0;
                let events_per_second = if load_time_seconds > 0.0 {
                    event_count as f64 / load_time_seconds
                } else {
                    0.0
                };

                let raw_row = StorageFootprintRawRow {
                    event_count,
                    iteration,
                    system: system.as_str().to_string(),
                    storage_bytes: measurement.storage_bytes,
                    storage_mb,
                    bytes_per_event: measurement.storage_bytes as f64 / event_count as f64,
                    load_time_ms: measurement.load_time_ms,
                    events_per_second,
                    path: display_path(&measurement.path),
                };
                raw_csv_writer.write_row(&raw_row)?;
                if config.cleanup_runs_after_measurement {
                    cleanup_run_store_dir(&measurement.path)?;
                }
                raw_rows.push(raw_row);
            }
        }
    }

    let summary_rows = summarize_rows(&raw_rows);
    let ratio_rows = build_ratio_rows(&summary_rows);
    let summary_csv_path = config.output_dir.join("storage_footprint_summary.csv");
    let ratio_csv_path = config.output_dir.join("storage_footprint_ratio_summary.csv");
    let markdown_path = config.output_dir.join("storage_footprint_summary.md");

    write_summary_csv(&summary_csv_path, &summary_rows)?;
    write_ratio_csv(&ratio_csv_path, &ratio_rows)?;
    write_markdown_report(&markdown_path, &metadata, config, &summary_rows, &ratio_rows)?;

    Ok(StorageFootprintOutcome {
        metadata,
        raw_rows,
        summary_rows,
        ratio_rows,
        raw_csv_path,
        summary_csv_path,
        ratio_csv_path,
        markdown_path,
    })
}

fn validate_config(config: &StorageFootprintConfig) -> Result<(), BoxError> {
    if config.event_counts.is_empty() {
        return Err("at least one event count is required".into());
    }
    if config.iterations == 0 {
        return Err("iterations must be >= 1".into());
    }
    if config.event_counts.contains(&TEN_MILLION_EVENTS) && !config.include_10m {
        return Err(
            format!("refusing to run {} events without --include-10m", TEN_MILLION_EVENTS).into()
        );
    }
    Ok(())
}

fn run_single_measurement(
    system: StorageSystem,
    event_count: usize,
    iteration: usize,
    store_dir: &Path,
) -> Result<RunMeasurement, BoxError> {
    if store_dir.exists() {
        return Err(format!(
            "store directory already exists, refusing to reuse it: {}",
            store_dir.display()
        )
        .into());
    }
    fs::create_dir_all(store_dir)?;

    let start = Instant::now();
    match system {
        StorageSystem::Janus => ingest_into_janus(store_dir, event_count, iteration)?,
        StorageSystem::Oxigraph => ingest_into_oxigraph(store_dir, event_count, iteration)?,
    }
    let load_time_ms = start.elapsed().as_secs_f64() * 1000.0;

    sync_tree(store_dir)?;
    let storage_bytes = recursive_dir_size_bytes(store_dir)?;

    Ok(RunMeasurement { storage_bytes, load_time_ms, path: store_dir.to_path_buf() })
}

fn ingest_into_janus(
    store_dir: &Path,
    event_count: usize,
    iteration: usize,
) -> Result<(), BoxError> {
    let config = janus_storage_config(store_dir);
    let flush_every = config.max_batch_events as usize;
    let storage = StreamingSegmentedStorage::new(config)?;

    for (index, event) in citybench_event_iter(event_count, iteration).enumerate() {
        storage.write_rdf_event(event)?;
        if (index + 1) % flush_every == 0 {
            storage.flush()?;
        }
    }
    storage.flush()?;
    drop(storage);
    Ok(())
}

fn ingest_into_oxigraph(
    store_dir: &Path,
    event_count: usize,
    iteration: usize,
) -> Result<(), BoxError> {
    let store = Store::open(store_dir)?;
    let mut loader = store.bulk_loader();
    loader.load_quads(citybench_oxigraph_quad_iter(event_count, iteration)?)?;
    loader.commit()?;
    store.flush()?;
    drop(store);
    Ok(())
}

fn janus_storage_config(store_dir: &Path) -> StreamingConfig {
    StreamingConfig {
        segment_base_path: store_dir.to_string_lossy().into_owned(),
        max_batch_events: DEFAULT_JANUS_FLUSH_EVERY_EVENTS as u64,
        max_batch_age_seconds: 3600,
        max_batch_bytes: 256 * 1024 * 1024,
        sparse_interval: 64,
        entries_per_index_block: 256,
    }
}

fn citybench_event_iter(event_count: usize, iteration: usize) -> impl Iterator<Item = RDFEvent> {
    let start_ts = BASE_TIMESTAMP_MS + iteration as u64 * ITERATION_TIMESTAMP_STRIDE_MS;
    (0..event_count).map(move |index| citybench_event(start_ts + index as u64, index))
}

fn citybench_oxigraph_quad_iter(
    event_count: usize,
    iteration: usize,
) -> Result<impl Iterator<Item = Quad>, BoxError> {
    let timestamp_predicate = NamedNode::new(OXIGRAPH_TIMESTAMP_PREDICATE)?;
    let graph_predicate = NamedNode::new(OXIGRAPH_GRAPH_PREDICATE)?;
    let timestamp_datatype = NamedNode::new(XSD_INTEGER)?;

    Ok(citybench_event_iter(event_count, iteration)
        .enumerate()
        .flat_map(move |(index, event)| {
            event_quads_for_persistent_oxigraph(
                iteration,
                index,
                event,
                &timestamp_predicate,
                &graph_predicate,
                &timestamp_datatype,
            )
            .expect("deterministic CityBench event should convert to persistent Oxigraph quads")
            .into_iter()
        }))
}

fn event_quads_for_persistent_oxigraph(
    iteration: usize,
    index: usize,
    event: RDFEvent,
    timestamp_predicate: &NamedNode,
    graph_predicate: &NamedNode,
    timestamp_datatype: &NamedNode,
) -> Result<[Quad; 3], BoxError> {
    let mut data_quad = rdf_event_to_quad(&event).map_err(std::io::Error::other)?;
    let event_graph_uri = format!("http://example.org/event/{iteration}/{index}");
    let event_graph_node = NamedNode::new(&event_graph_uri)?;
    let event_graph = GraphName::NamedNode(event_graph_node.clone());
    data_quad.graph_name = event_graph;

    let timestamp_quad = Quad::new(
        event_graph_node.clone(),
        timestamp_predicate.clone(),
        Literal::new_typed_literal(event.timestamp.to_string(), timestamp_datatype.clone()),
        GraphName::DefaultGraph,
    );
    let graph_quad = Quad::new(
        event_graph_node,
        graph_predicate.clone(),
        NamedNode::new(&event.graph)?,
        GraphName::DefaultGraph,
    );

    Ok([data_quad, timestamp_quad, graph_quad])
}

fn cleanup_run_store_dir(store_dir: &Path) -> Result<(), BoxError> {
    fs::remove_dir_all(store_dir).map_err(|err| {
        std::io::Error::other(format!(
            "failed to remove run store directory {} after persisting raw CSV row: {}",
            store_dir.display(),
            err
        ))
    })?;
    Ok(())
}

fn recursive_dir_size_bytes(root: &Path) -> Result<u64, BoxError> {
    let mut total = 0_u64;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        } else if metadata.is_dir() {
            total = total.saturating_add(recursive_dir_size_bytes(&path)?);
        }
    }
    Ok(total)
}

fn sync_tree(root: &Path) -> Result<(), BoxError> {
    sync_dir_entry(root)?;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_file() {
            let file = File::open(&path)?;
            file.sync_all()?;
        } else if metadata.is_dir() {
            sync_tree(&path)?;
        }
    }
    Ok(())
}

fn sync_dir_entry(path: &Path) -> Result<(), BoxError> {
    if let Ok(dir) = File::open(path) {
        dir.sync_all()?;
    }
    Ok(())
}

fn summarize_rows(rows: &[StorageFootprintRawRow]) -> Vec<StorageFootprintSummaryRow> {
    let mut grouped: BTreeMap<(usize, String), Vec<&StorageFootprintRawRow>> = BTreeMap::new();
    for row in rows {
        grouped.entry((row.event_count, row.system.clone())).or_default().push(row);
    }

    grouped
        .into_iter()
        .map(|((event_count, system), grouped_rows)| {
            let storage_bytes =
                grouped_rows.iter().map(|row| row.storage_bytes as f64).collect::<Vec<_>>();
            let storage_mb = grouped_rows.iter().map(|row| row.storage_mb).collect::<Vec<_>>();
            let bytes_per_event =
                grouped_rows.iter().map(|row| row.bytes_per_event).collect::<Vec<_>>();
            let load_time_ms = grouped_rows.iter().map(|row| row.load_time_ms).collect::<Vec<_>>();
            let events_per_second =
                grouped_rows.iter().map(|row| row.events_per_second).collect::<Vec<_>>();

            StorageFootprintSummaryRow {
                event_count,
                system,
                n: grouped_rows.len(),
                median_storage_bytes: median(&storage_bytes),
                median_storage_mb: median(&storage_mb),
                median_bytes_per_event: median(&bytes_per_event),
                mean_storage_mb: mean(&storage_mb),
                std_storage_mb: sample_std_dev(&storage_mb),
                median_load_time_ms: median(&load_time_ms),
                mean_load_time_ms: mean(&load_time_ms),
                std_load_time_ms: sample_std_dev(&load_time_ms),
                median_events_per_second: median(&events_per_second),
            }
        })
        .collect()
}

fn build_ratio_rows(summary_rows: &[StorageFootprintSummaryRow]) -> Vec<StorageFootprintRatioRow> {
    let mut janus_by_event_count = BTreeMap::new();
    let mut oxigraph_by_event_count = BTreeMap::new();

    for row in summary_rows {
        match row.system.as_str() {
            "janus" => {
                janus_by_event_count.insert(row.event_count, row.clone());
            }
            "oxigraph" => {
                oxigraph_by_event_count.insert(row.event_count, row.clone());
            }
            _ => {}
        }
    }

    janus_by_event_count
        .into_iter()
        .filter_map(|(event_count, janus_row)| {
            let oxigraph_row = oxigraph_by_event_count.get(&event_count)?;
            let ratio = if janus_row.median_storage_mb > 0.0 {
                oxigraph_row.median_storage_mb / janus_row.median_storage_mb
            } else {
                0.0
            };
            Some(StorageFootprintRatioRow {
                event_count,
                janus_median_storage_mb: janus_row.median_storage_mb,
                oxigraph_median_storage_mb: oxigraph_row.median_storage_mb,
                janus_median_bytes_per_event: janus_row.median_bytes_per_event,
                oxigraph_median_bytes_per_event: oxigraph_row.median_bytes_per_event,
                oxigraph_over_janus_storage_ratio: ratio,
            })
        })
        .collect()
}

fn write_summary_csv(path: &Path, rows: &[StorageFootprintSummaryRow]) -> Result<(), BoxError> {
    let mut file = File::create(path)?;
    writeln!(
        file,
        "event_count,system,n,median_storage_bytes,median_storage_mb,median_bytes_per_event,mean_storage_mb,std_storage_mb,median_load_time_ms,mean_load_time_ms,std_load_time_ms,median_events_per_second"
    )?;
    for row in rows {
        writeln!(
            file,
            "{},{},{},{:.3},{:.6},{:.6},{:.6},{:.6},{:.3},{:.3},{:.3},{:.6}",
            row.event_count,
            row.system,
            row.n,
            row.median_storage_bytes,
            row.median_storage_mb,
            row.median_bytes_per_event,
            row.mean_storage_mb,
            row.std_storage_mb,
            row.median_load_time_ms,
            row.mean_load_time_ms,
            row.std_load_time_ms,
            row.median_events_per_second
        )?;
    }
    Ok(())
}

fn write_ratio_csv(path: &Path, rows: &[StorageFootprintRatioRow]) -> Result<(), BoxError> {
    let mut file = File::create(path)?;
    writeln!(
        file,
        "event_count,janus_median_storage_mb,oxigraph_median_storage_mb,janus_median_bytes_per_event,oxigraph_median_bytes_per_event,oxigraph_over_janus_storage_ratio"
    )?;
    for row in rows {
        writeln!(
            file,
            "{},{:.6},{:.6},{:.6},{:.6},{:.6}",
            row.event_count,
            row.janus_median_storage_mb,
            row.oxigraph_median_storage_mb,
            row.janus_median_bytes_per_event,
            row.oxigraph_median_bytes_per_event,
            row.oxigraph_over_janus_storage_ratio
        )?;
    }
    Ok(())
}

fn write_markdown_report(
    path: &Path,
    metadata: &ReproMetadata,
    config: &StorageFootprintConfig,
    summary_rows: &[StorageFootprintSummaryRow],
    ratio_rows: &[StorageFootprintRatioRow],
) -> Result<(), BoxError> {
    let mut file = File::create(path)?;
    let date_display = capture_command("date", &["+%Y-%m-%d %H:%M:%S %Z"]);

    writeln!(file, "# Storage Footprint Benchmark Summary")?;
    writeln!(file)?;
    writeln!(file, "## Benchmark Command")?;
    writeln!(file)?;
    writeln!(file, "```bash")?;
    writeln!(file, "{}", metadata.benchmark_command)?;
    writeln!(file, "```")?;
    writeln!(file)?;
    writeln!(file, "## Machine and Run Metadata")?;
    writeln!(file)?;
    writeln!(
        file,
        "- Date: {}",
        if date_display == "unknown" {
            "unknown"
        } else {
            &date_display
        }
    )?;
    writeln!(file, "- Git branch: {}", metadata.branch)?;
    writeln!(file, "- Git commit: {}", metadata.git_commit_sha)?;
    writeln!(file, "- Rust: {}", metadata.rustc_version)?;
    writeln!(file, "- OS: {}", metadata.os)?;
    writeln!(file, "- CPU: {}", metadata.cpu_model)?;
    writeln!(
        file,
        "- Cleanup runs after measurement: {}",
        config.cleanup_runs_after_measurement
    )?;
    writeln!(
        file,
        "- RAM bytes: {}",
        metadata
            .ram_bytes
            .map_or_else(|| "unknown".to_string(), |bytes| bytes.to_string())
    )?;
    writeln!(file)?;
    writeln!(file, "## Summary")?;
    writeln!(file)?;
    writeln!(
        file,
        "| event_count | system | n | median_storage_bytes | median_storage_mb | median_bytes_per_event | mean_storage_mb | std_storage_mb | median_load_time_ms | mean_load_time_ms | std_load_time_ms | median_events_per_second |"
    )?;
    writeln!(
        file,
        "| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"
    )?;
    for row in summary_rows {
        writeln!(
            file,
            "| {} | {} | {} | {:.3} | {:.6} | {:.6} | {:.6} | {:.6} | {:.3} | {:.3} | {:.3} | {:.6} |",
            row.event_count,
            row.system,
            row.n,
            row.median_storage_bytes,
            row.median_storage_mb,
            row.median_bytes_per_event,
            row.mean_storage_mb,
            row.std_storage_mb,
            row.median_load_time_ms,
            row.mean_load_time_ms,
            row.std_load_time_ms,
            row.median_events_per_second
        )?;
    }
    writeln!(file)?;
    writeln!(file, "## Ratios")?;
    writeln!(file)?;
    writeln!(
        file,
        "| event_count | janus_median_storage_mb | oxigraph_median_storage_mb | janus_median_bytes_per_event | oxigraph_median_bytes_per_event | oxigraph_over_janus_storage_ratio |"
    )?;
    writeln!(file, "| ---: | ---: | ---: | ---: | ---: | ---: |")?;
    for row in ratio_rows {
        writeln!(
            file,
            "| {} | {:.6} | {:.6} | {:.6} | {:.6} | {:.6} |",
            row.event_count,
            row.janus_median_storage_mb,
            row.oxigraph_median_storage_mb,
            row.janus_median_bytes_per_event,
            row.oxigraph_median_bytes_per_event,
            row.oxigraph_over_janus_storage_ratio
        )?;
    }
    writeln!(file)?;
    writeln!(
        file,
        "Persistent storage footprint is measured after ingestion has completed and the store has been flushed, closed, and measured on disk. The metric includes database metadata and indexes in addition to persisted RDF event payloads."
    )?;
    writeln!(
        file,
        "This benchmark compares Janus's append-oriented historical RDF event log against Oxigraph as a general-purpose persistent RDF store. Higher Oxigraph disk usage is expected because it maintains general RDF database structures and indexes for SPARQL access patterns."
    )?;
    Ok(())
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn bytes_to_mb(bytes: u64) -> f64 {
    bytes as f64 / STORAGE_MB_DIVISOR
}

fn display_path(path: &Path) -> String {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf()).display().to_string()
}

impl RawCsvWriter {
    fn create(path: &Path) -> Result<Self, BoxError> {
        let mut writer = BufWriter::new(File::create(path)?);
        writeln!(
            writer,
            "event_count,iteration,system,storage_bytes,storage_mb,bytes_per_event,load_time_ms,events_per_second,path"
        )?;
        writer.flush()?;
        Ok(Self { writer })
    }

    fn write_row(&mut self, row: &StorageFootprintRawRow) -> Result<(), BoxError> {
        writeln!(
            self.writer,
            "{},{},{},{},{:.6},{:.6},{:.3},{:.6},{}",
            row.event_count,
            row.iteration,
            row.system,
            row.storage_bytes,
            row.storage_mb,
            row.bytes_per_event,
            row.load_time_ms,
            row.events_per_second,
            csv_escape(&row.path)
        )?;
        self.writer.flush()?;
        Ok(())
    }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

fn sample_std_dev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let avg = mean(values);
    let variance =
        values.iter().map(|value| (value - avg).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    variance.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn small_storage_footprint_run_writes_non_zero_storage_for_both_systems() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let output_dir = temp_dir.path().join("storage_footprint");
        let outcome = run_storage_footprint_benchmark(&StorageFootprintConfig {
            event_counts: vec![10, 100],
            iterations: 1,
            output_dir: output_dir.clone(),
            include_10m: false,
            cleanup_runs_after_measurement: false,
            system_selection: StorageSystemSelection::Both,
        })
        .expect("small benchmark run should succeed");

        assert_eq!(outcome.raw_rows.len(), 4);
        assert!(outcome.raw_rows.iter().all(|row| row.storage_bytes > 0));
        assert!(outcome.raw_rows.iter().all(|row| row.storage_mb > 0.0));
        assert!(outcome.raw_csv_path.is_file());
        assert!(outcome.summary_csv_path.is_file());
        assert!(outcome.ratio_csv_path.is_file());
        assert!(outcome.markdown_path.is_file());
    }

    #[test]
    fn ten_million_requires_explicit_opt_in() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let err = run_storage_footprint_benchmark(&StorageFootprintConfig {
            event_counts: vec![TEN_MILLION_EVENTS],
            iterations: 1,
            output_dir: temp_dir.path().join("guard"),
            include_10m: false,
            cleanup_runs_after_measurement: false,
            system_selection: StorageSystemSelection::Janus,
        })
        .expect_err("10M run should be rejected without include_10m");

        assert!(err.to_string().contains("--include-10m"));
    }

    #[test]
    fn cleanup_enabled_removes_run_store_dirs_but_keeps_result_files() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let output_dir = temp_dir.path().join("storage_footprint_cleanup");
        let outcome = run_storage_footprint_benchmark(&StorageFootprintConfig {
            event_counts: vec![10],
            iterations: 1,
            output_dir: output_dir.clone(),
            include_10m: false,
            cleanup_runs_after_measurement: true,
            system_selection: StorageSystemSelection::Both,
        })
        .expect("cleanup benchmark run should succeed");

        assert!(outcome.raw_csv_path.is_file());
        assert!(outcome.summary_csv_path.is_file());
        assert!(outcome.ratio_csv_path.is_file());
        assert!(outcome.markdown_path.is_file());
        assert!(!output_dir.join("runs/janus_events_10_iter_1/store").exists());
        assert!(!output_dir.join("runs/oxigraph_events_10_iter_1/store").exists());
    }
}
