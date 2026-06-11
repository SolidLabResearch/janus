use crate::core::RDFEvent;
use crate::extensions::query_options::build_evaluator;
use crate::storage::segmented_storage::StreamingSegmentedStorage;
use crate::storage::util::StreamingConfig;
use clap::ValueEnum;
use oxigraph::model::{GraphName, Literal, NamedNode, Quad, Term};
use oxigraph::sparql::QueryResults;
use oxigraph::store::Store;
use plotters::prelude::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::Instant;

type BoxError = Box<dyn Error>;

pub const DEFAULT_DATASET_SIZES: [usize; 4] = [10_000, 50_000, 100_000, 500_000];
pub const DEFAULT_FIXED_RANGE_SECONDS: u64 = 60;
pub const RESULT_MARKDOWN_PATH: &str = "target/paper_benchmarks/h2_range_comparison_results.md";
pub const FIXED_60S_PLOT_PATH: &str = "target/paper_benchmarks/h2_fixed_60s_janus_vs_oxigraph.png";
pub const FULL_HISTORY_PLOT_PATH: &str =
    "target/paper_benchmarks/h2_full_history_janus_vs_oxigraph.png";

const BASE_TIMESTAMP_MS: u64 = 1_720_000_000_000;
const TIMESTAMP_STEP_MS: u64 = 10;
const TIMESTAMP_PREDICATE: &str = "http://example.org/schema/timestamp";
const TIMESTAMP_DATATYPE: &str = "http://www.w3.org/2001/XMLSchema#long";

static CONFIG_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum HistoricalRangeMode {
    Warm,
    Cold,
}

impl HistoricalRangeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warm => "warm",
            Self::Cold => "cold",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, ValueEnum)]
pub enum HistoricalRangeQueryCaseArg {
    #[value(name = "fixed_60s")]
    Fixed60s,
    #[value(name = "full_history")]
    FullHistory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum HistoricalRangeQueryCase {
    Fixed60sRange,
    FullHistoryRange,
}

impl HistoricalRangeQueryCase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fixed60sRange => "fixed_60s_range",
            Self::FullHistoryRange => "full_history_range",
        }
    }

    pub fn from_arg(value: HistoricalRangeQueryCaseArg) -> Self {
        match value {
            HistoricalRangeQueryCaseArg::Fixed60s => Self::Fixed60sRange,
            HistoricalRangeQueryCaseArg::FullHistory => Self::FullHistoryRange,
        }
    }

    fn janus_row_label(self) -> &'static str {
        match self {
            Self::Fixed60sRange => "Janus fixed 60s",
            Self::FullHistoryRange => "Janus full history",
        }
    }

    fn oxigraph_row_label(self) -> &'static str {
        match self {
            Self::Fixed60sRange => "Oxigraph fixed 60s FILTER",
            Self::FullHistoryRange => "Oxigraph full history FILTER",
        }
    }

    fn takeaway(self) -> &'static str {
        match self {
            Self::Fixed60sRange => "Bounded timestamp lookup",
            Self::FullHistoryRange => "Full historical read",
        }
    }
}

#[derive(Clone, Debug)]
pub struct HistoricalRangeComparisonConfig {
    pub runs: usize,
    pub warmup_runs: usize,
    pub dataset_sizes: Vec<usize>,
    pub query_cases: Vec<HistoricalRangeQueryCase>,
    pub fixed_range_seconds: u64,
    pub mode: HistoricalRangeMode,
    pub debug_equivalence: bool,
    pub output_dir: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
pub struct HistoricalRangeRawRow {
    pub system: String,
    pub dataset_size_quads: usize,
    pub event_count: usize,
    pub query_case: String,
    pub range_start_ms: u64,
    pub range_end_ms: u64,
    pub range_width_ms: u64,
    pub result_count: usize,
    pub result_hash: String,
    pub equivalent_to_baseline: bool,
    pub latency_ms: f64,
    pub run_id: usize,
    pub is_warmup: bool,
    pub mode: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct HistoricalRangeSummaryRow {
    pub dataset_size_quads: usize,
    pub query_case: String,
    pub range_width_ms: u64,
    pub result_count: usize,
    pub janus_p50_ms: f64,
    pub oxigraph_p50_ms: f64,
    pub janus_p95_ms: f64,
    pub oxigraph_p95_ms: f64,
    pub janus_avg_ms: f64,
    pub oxigraph_avg_ms: f64,
    pub ratio_oxigraph_over_janus: f64,
    pub equivalent: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct HistoricalRangeRunOutcome {
    pub dataset_size_quads: usize,
    pub event_count: usize,
    pub query_case: String,
    pub range_start_ms: u64,
    pub range_end_ms: u64,
    pub range_width_ms: u64,
    pub janus_result_count: usize,
    pub oxigraph_result_count: usize,
    pub janus_result_hash: String,
    pub oxigraph_result_hash: String,
    pub equivalent: bool,
    pub run_id: usize,
    pub is_warmup: bool,
}

#[derive(Clone, Debug)]
pub struct HistoricalRangeComparisonOutcome {
    pub changed_output_files: Vec<PathBuf>,
    pub raw_rows: Vec<HistoricalRangeRawRow>,
    pub summary_rows: Vec<HistoricalRangeSummaryRow>,
    pub run_outcomes: Vec<HistoricalRangeRunOutcome>,
}

#[derive(Clone)]
struct PreparedDataset {
    dataset_size_quads: usize,
    event_count: usize,
    timestamp_min: u64,
    timestamp_max: u64,
    janus_storage: Arc<StreamingSegmentedStorage>,
    oxigraph_store: Store,
}

#[derive(Clone, Debug)]
struct RangeDefinition {
    start_ms: u64,
    end_ms_exclusive: u64,
}

struct RunExecutionContext<'a> {
    run_id: usize,
    is_warmup: bool,
    fixed_range_seconds: u64,
    mode: HistoricalRangeMode,
    debug_equivalence: bool,
    output_dir: &'a Path,
}

#[derive(Clone, Debug, Serialize)]
struct CanonicalResultRow {
    subject: String,
    predicate: String,
    object: String,
    graph: String,
    timestamp_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
struct EquivalenceReport {
    dataset_size_quads: usize,
    event_count: usize,
    query_case: String,
    range_start_ms: u64,
    range_end_ms: u64,
    range_width_ms: u64,
    janus_result_count: usize,
    oxigraph_result_count: usize,
    janus_result_hash: String,
    oxigraph_result_hash: String,
    equivalent: bool,
}

struct EquivalenceDebugContext<'a> {
    output_dir: &'a Path,
    dataset: &'a PreparedDataset,
    query_case: HistoricalRangeQueryCase,
    run_id: usize,
    is_warmup: bool,
    range: &'a RangeDefinition,
    janus_rows: &'a [CanonicalResultRow],
    oxigraph_rows: &'a [CanonicalResultRow],
    janus_hash: &'a str,
    oxigraph_hash: &'a str,
}

pub fn run_historical_range_comparison(
    config: &HistoricalRangeComparisonConfig,
) -> Result<HistoricalRangeComparisonOutcome, BoxError> {
    ensure_output_dir(&config.output_dir)?;
    ensure_output_dir(Path::new("target/paper_benchmarks"))?;

    let mut raw_rows = Vec::new();
    let mut run_outcomes = Vec::new();

    for &dataset_size in &config.dataset_sizes {
        let warm_dataset = if config.mode == HistoricalRangeMode::Warm {
            Some(prepare_dataset(dataset_size)?)
        } else {
            None
        };

        for &query_case in &config.query_cases {
            for run_id in 0..config.warmup_runs {
                let dataset = if let Some(dataset) = &warm_dataset {
                    dataset.clone()
                } else {
                    prepare_dataset(dataset_size)?
                };
                let run = execute_query_case(
                    &dataset,
                    query_case,
                    &RunExecutionContext {
                        run_id,
                        is_warmup: true,
                        fixed_range_seconds: config.fixed_range_seconds,
                        mode: config.mode,
                        debug_equivalence: config.debug_equivalence,
                        output_dir: &config.output_dir,
                    },
                )?;
                raw_rows.extend(run.0);
                run_outcomes.push(run.1);
            }

            for run_id in 0..config.runs {
                let dataset = if let Some(dataset) = &warm_dataset {
                    dataset.clone()
                } else {
                    prepare_dataset(dataset_size)?
                };
                let run = execute_query_case(
                    &dataset,
                    query_case,
                    &RunExecutionContext {
                        run_id,
                        is_warmup: false,
                        fixed_range_seconds: config.fixed_range_seconds,
                        mode: config.mode,
                        debug_equivalence: config.debug_equivalence,
                        output_dir: &config.output_dir,
                    },
                )?;
                raw_rows.extend(run.0);
                run_outcomes.push(run.1);
            }
        }
    }

    let summary_rows = summarize_rows(&raw_rows);
    let raw_jsonl = config.output_dir.join("paper_historical_range_comparison.raw.jsonl");
    let summary_csv = config.output_dir.join("paper_historical_range_comparison.summary.csv");
    let markdown_path = PathBuf::from(RESULT_MARKDOWN_PATH);
    let fixed_plot_path = PathBuf::from(FIXED_60S_PLOT_PATH);
    let full_plot_path = PathBuf::from(FULL_HISTORY_PLOT_PATH);

    write_jsonl(&raw_jsonl, &raw_rows)?;
    write_summary_csv(&summary_csv, &summary_rows)?;
    write_markdown(&markdown_path, &summary_rows)?;
    write_plot(
        &fixed_plot_path,
        &summary_rows,
        HistoricalRangeQueryCase::Fixed60sRange,
        "H2 Fixed 60s Janus vs Oxigraph",
    )?;
    write_plot(
        &full_plot_path,
        &summary_rows,
        HistoricalRangeQueryCase::FullHistoryRange,
        "H2 Full History Janus vs Oxigraph",
    )?;

    Ok(HistoricalRangeComparisonOutcome {
        changed_output_files: vec![
            raw_jsonl,
            summary_csv,
            markdown_path,
            fixed_plot_path,
            full_plot_path,
        ],
        raw_rows,
        summary_rows,
        run_outcomes,
    })
}

fn execute_query_case(
    dataset: &PreparedDataset,
    query_case: HistoricalRangeQueryCase,
    context: &RunExecutionContext<'_>,
) -> Result<(Vec<HistoricalRangeRawRow>, HistoricalRangeRunOutcome), BoxError> {
    let range = range_for_case(dataset, query_case, context.fixed_range_seconds)?;
    let query_text = oxigraph_timestamp_filter_query(range.start_ms, range.end_ms_exclusive);

    let janus_started = Instant::now();
    let janus_results =
        historical_range(&dataset.janus_storage, range.start_ms, range.end_ms_exclusive)?;
    let janus_latency_ms = janus_started.elapsed().as_secs_f64() * 1_000.0;
    let janus_canonical = canonicalize_janus_results(&janus_results);
    let janus_hash = result_hash(&janus_canonical)?;

    let oxigraph_started = Instant::now();
    let oxigraph_results = execute_oxigraph_range_query(&dataset.oxigraph_store, &query_text)?;
    let oxigraph_latency_ms = oxigraph_started.elapsed().as_secs_f64() * 1_000.0;
    let oxigraph_hash = result_hash(&oxigraph_results)?;

    let equivalent = janus_canonical.len() == oxigraph_results.len() && janus_hash == oxigraph_hash;

    if context.debug_equivalence {
        write_query_debug_artifact(
            context.output_dir,
            dataset.dataset_size_quads,
            query_case,
            context.run_id,
            context.is_warmup,
            &query_text,
        )?;
    }

    if !equivalent && context.debug_equivalence {
        write_equivalence_debug_artifacts(&EquivalenceDebugContext {
            output_dir: context.output_dir,
            dataset,
            query_case,
            run_id: context.run_id,
            is_warmup: context.is_warmup,
            range: &range,
            janus_rows: &janus_canonical,
            oxigraph_rows: &oxigraph_results,
            janus_hash: &janus_hash,
            oxigraph_hash: &oxigraph_hash,
        })?;
    }

    let range_width_ms = range.end_ms_exclusive.saturating_sub(range.start_ms);
    let janus_raw = HistoricalRangeRawRow {
        system: "janus".to_string(),
        dataset_size_quads: dataset.dataset_size_quads,
        event_count: dataset.event_count,
        query_case: query_case.as_str().to_string(),
        range_start_ms: range.start_ms,
        range_end_ms: range.end_ms_exclusive,
        range_width_ms,
        result_count: janus_canonical.len(),
        result_hash: janus_hash.clone(),
        equivalent_to_baseline: equivalent,
        latency_ms: janus_latency_ms,
        run_id: context.run_id,
        is_warmup: context.is_warmup,
        mode: context.mode.as_str().to_string(),
    };
    let oxigraph_raw = HistoricalRangeRawRow {
        system: "oxigraph".to_string(),
        dataset_size_quads: dataset.dataset_size_quads,
        event_count: dataset.event_count,
        query_case: query_case.as_str().to_string(),
        range_start_ms: range.start_ms,
        range_end_ms: range.end_ms_exclusive,
        range_width_ms,
        result_count: oxigraph_results.len(),
        result_hash: oxigraph_hash.clone(),
        equivalent_to_baseline: equivalent,
        latency_ms: oxigraph_latency_ms,
        run_id: context.run_id,
        is_warmup: context.is_warmup,
        mode: context.mode.as_str().to_string(),
    };
    let outcome = HistoricalRangeRunOutcome {
        dataset_size_quads: dataset.dataset_size_quads,
        event_count: dataset.event_count,
        query_case: query_case.as_str().to_string(),
        range_start_ms: range.start_ms,
        range_end_ms: range.end_ms_exclusive,
        range_width_ms,
        janus_result_count: janus_canonical.len(),
        oxigraph_result_count: oxigraph_results.len(),
        janus_result_hash: janus_hash,
        oxigraph_result_hash: oxigraph_hash,
        equivalent,
        run_id: context.run_id,
        is_warmup: context.is_warmup,
    };
    Ok((vec![janus_raw, oxigraph_raw], outcome))
}

fn prepare_dataset(dataset_size_quads: usize) -> Result<PreparedDataset, BoxError> {
    let janus_storage = Arc::new(StreamingSegmentedStorage::new(unique_config("paper_h2_range"))?);
    let oxigraph_store = Store::new()?;
    let timestamp_predicate = NamedNode::new(TIMESTAMP_PREDICATE)?;
    let timestamp_datatype = NamedNode::new(TIMESTAMP_DATATYPE)?;
    let timestamp_min = BASE_TIMESTAMP_MS;
    let mut timestamp_max = BASE_TIMESTAMP_MS;

    for index in 0..dataset_size_quads {
        let timestamp_ms = BASE_TIMESTAMP_MS + (index as u64 * TIMESTAMP_STEP_MS);
        timestamp_max = timestamp_ms;
        let event = RDFEvent::new(
            timestamp_ms,
            &format!("http://example.org/event/{index:08}"),
            TIMESTAMP_PREDICATE,
            &typed_timestamp_literal(timestamp_ms),
            "",
        );
        janus_storage.write_rdf_event(event.clone())?;
        oxigraph_store.insert(&Quad::new(
            NamedNode::new(event.subject.as_str())?,
            timestamp_predicate.clone(),
            Literal::new_typed_literal(timestamp_ms.to_string(), timestamp_datatype.clone()),
            GraphName::DefaultGraph,
        ))?;
    }
    janus_storage.flush()?;

    Ok(PreparedDataset {
        dataset_size_quads,
        event_count: dataset_size_quads,
        timestamp_min,
        timestamp_max,
        janus_storage,
        oxigraph_store,
    })
}

fn range_for_case(
    dataset: &PreparedDataset,
    query_case: HistoricalRangeQueryCase,
    fixed_range_seconds: u64,
) -> Result<RangeDefinition, BoxError> {
    let start_ms = dataset.timestamp_min;
    let end_ms_exclusive = match query_case {
        HistoricalRangeQueryCase::Fixed60sRange => start_ms
            .checked_add(fixed_range_seconds.saturating_mul(1_000))
            .ok_or("fixed range end overflowed u64")?,
        HistoricalRangeQueryCase::FullHistoryRange => dataset
            .timestamp_max
            .checked_add(1)
            .ok_or("full history range end overflowed u64")?,
    };
    Ok(RangeDefinition { start_ms, end_ms_exclusive })
}

fn historical_range(
    storage: &Arc<StreamingSegmentedStorage>,
    start_ms: u64,
    end_ms_exclusive: u64,
) -> Result<Vec<RDFEvent>, BoxError> {
    if end_ms_exclusive <= start_ms {
        return Ok(Vec::new());
    }
    let end_ms_inclusive = end_ms_exclusive.saturating_sub(1);
    Ok(storage.query_rdf(start_ms, end_ms_inclusive)?)
}

fn execute_oxigraph_range_query(
    store: &Store,
    query_text: &str,
) -> Result<Vec<CanonicalResultRow>, BoxError> {
    let evaluator = build_evaluator();
    let parsed_query = evaluator.parse_query(query_text)?;
    let results = parsed_query.on_store(store).execute()?;
    let mut rows = Vec::new();

    if let QueryResults::Solutions(solutions) = results {
        for solution in solutions {
            let solution = solution?;
            let event_term = solution.get("event").ok_or("missing ?event binding")?;
            let timestamp_term = solution.get("t").ok_or("missing ?t binding")?;

            let subject = match event_term {
                Term::NamedNode(node) => node.as_str().to_string(),
                other => return Err(format!("expected NamedNode for ?event, got {other}").into()),
            };
            let timestamp_ms = match timestamp_term {
                Term::Literal(literal) => literal.value().parse::<u64>()?,
                other => return Err(format!("expected Literal for ?t, got {other}").into()),
            };

            rows.push(CanonicalResultRow {
                subject,
                predicate: TIMESTAMP_PREDICATE.to_string(),
                object: typed_timestamp_literal(timestamp_ms),
                graph: String::new(),
                timestamp_ms,
            });
        }
    }

    rows.sort_by(canonical_row_cmp);
    Ok(rows)
}

fn oxigraph_timestamp_filter_query(range_start_ms: u64, range_end_ms: u64) -> String {
    format!(
        "PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>\n\
SELECT ?event ?t\n\
WHERE {{\n\
  ?event <{TIMESTAMP_PREDICATE}> ?t .\n\
  FILTER (?t >= \"{range_start_ms}\"^^xsd:long && ?t < \"{range_end_ms}\"^^xsd:long)\n\
}}\n"
    )
}

fn canonicalize_janus_results(results: &[RDFEvent]) -> Vec<CanonicalResultRow> {
    let mut rows = results
        .iter()
        .map(|event| CanonicalResultRow {
            subject: event.subject.clone(),
            predicate: event.predicate.clone(),
            object: typed_timestamp_literal(event.timestamp),
            graph: event.graph.clone(),
            timestamp_ms: event.timestamp,
        })
        .collect::<Vec<_>>();
    rows.sort_by(canonical_row_cmp);
    rows
}

fn canonical_row_cmp(left: &CanonicalResultRow, right: &CanonicalResultRow) -> Ordering {
    left.timestamp_ms
        .cmp(&right.timestamp_ms)
        .then_with(|| left.subject.cmp(&right.subject))
        .then_with(|| left.predicate.cmp(&right.predicate))
        .then_with(|| left.object.cmp(&right.object))
        .then_with(|| left.graph.cmp(&right.graph))
}

fn result_hash(rows: &[CanonicalResultRow]) -> Result<String, BoxError> {
    let payload = serde_json::to_vec(rows)?;
    Ok(format!("{:x}", Sha256::digest(payload)))
}

fn typed_timestamp_literal(timestamp_ms: u64) -> String {
    format!("\"{timestamp_ms}\"^^<{TIMESTAMP_DATATYPE}>")
}

fn summarize_rows(rows: &[HistoricalRangeRawRow]) -> Vec<HistoricalRangeSummaryRow> {
    let mut janus_rows = HashMap::<(usize, String), Vec<&HistoricalRangeRawRow>>::new();
    let mut oxigraph_rows = HashMap::<(usize, String), Vec<&HistoricalRangeRawRow>>::new();

    for row in rows.iter().filter(|row| !row.is_warmup) {
        let key = (row.dataset_size_quads, row.query_case.clone());
        if row.system == "janus" {
            janus_rows.entry(key).or_default().push(row);
        } else if row.system == "oxigraph" {
            oxigraph_rows.entry(key).or_default().push(row);
        }
    }

    let mut keys = janus_rows.keys().cloned().collect::<Vec<_>>();
    keys.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

    keys.into_iter()
        .filter_map(|key| {
            let janus = janus_rows.get(&key)?;
            let oxigraph = oxigraph_rows.get(&key)?;
            let janus_latencies = janus.iter().map(|row| row.latency_ms).collect::<Vec<_>>();
            let oxigraph_latencies = oxigraph.iter().map(|row| row.latency_ms).collect::<Vec<_>>();
            let janus_p50 = percentile(&janus_latencies, 50.0);
            let oxigraph_p50 = percentile(&oxigraph_latencies, 50.0);
            Some(HistoricalRangeSummaryRow {
                dataset_size_quads: key.0,
                query_case: key.1,
                range_width_ms: janus.first().map_or(0, |row| row.range_width_ms),
                result_count: janus.first().map_or(0, |row| row.result_count),
                janus_p50_ms: janus_p50,
                oxigraph_p50_ms: oxigraph_p50,
                janus_p95_ms: percentile(&janus_latencies, 95.0),
                oxigraph_p95_ms: percentile(&oxigraph_latencies, 95.0),
                janus_avg_ms: mean(&janus_latencies),
                oxigraph_avg_ms: mean(&oxigraph_latencies),
                ratio_oxigraph_over_janus: if janus_p50 > 0.0 {
                    oxigraph_p50 / janus_p50
                } else {
                    0.0
                },
                equivalent: janus.iter().all(|row| row.equivalent_to_baseline)
                    && oxigraph.iter().all(|row| row.equivalent_to_baseline),
            })
        })
        .collect()
}

fn write_summary_csv(path: &Path, rows: &[HistoricalRangeSummaryRow]) -> Result<(), BoxError> {
    let mut file = File::create(path)?;
    writeln!(
        file,
        "dataset_size_quads,query_case,range_width_ms,result_count,janus_p50_ms,oxigraph_p50_ms,janus_p95_ms,oxigraph_p95_ms,janus_avg_ms,oxigraph_avg_ms,ratio_oxigraph_over_janus,equivalent"
    )?;
    for row in rows {
        writeln!(
            file,
            "{},{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{}",
            row.dataset_size_quads,
            row.query_case,
            row.range_width_ms,
            row.result_count,
            row.janus_p50_ms,
            row.oxigraph_p50_ms,
            row.janus_p95_ms,
            row.oxigraph_p95_ms,
            row.janus_avg_ms,
            row.oxigraph_avg_ms,
            row.ratio_oxigraph_over_janus,
            row.equivalent
        )?;
    }
    Ok(())
}

fn write_markdown(path: &Path, rows: &[HistoricalRangeSummaryRow]) -> Result<(), BoxError> {
    let mut file = File::create(path)?;
    let sizes = DEFAULT_DATASET_SIZES;

    writeln!(file, "# H2 Historical Timestamp Range Comparison")?;
    writeln!(file)?;
    writeln!(file, "## Query")?;
    writeln!(file)?;
    writeln!(file, "Both systems answer the same timestamp range query:")?;
    writeln!(file)?;
    writeln!(file, "timestamp >= X")?;
    writeln!(file, "timestamp < Y")?;
    writeln!(file)?;
    writeln!(
        file,
        "Janus executes this as a historical range lookup over its event-log structure."
    )?;
    writeln!(file)?;
    writeln!(file, "Oxigraph executes this as SPARQL over the same RDF quads:")?;
    writeln!(file)?;
    writeln!(file, "SELECT ?event ?t")?;
    writeln!(file, "WHERE {{")?;
    writeln!(file, "  ?event <{TIMESTAMP_PREDICATE}> ?t .")?;
    writeln!(file, "  FILTER (?t >= X && ?t < Y)")?;
    writeln!(file, "}}")?;
    writeln!(file)?;
    writeln!(file, "## Result")?;
    writeln!(file)?;
    writeln!(
        file,
        "| Query Case | 10k quads | 50k quads | 100k quads | 500k quads | Takeaway |"
    )?;
    writeln!(file, "| --- | ---: | ---: | ---: | ---: | --- |")?;

    for query_case in [
        HistoricalRangeQueryCase::Fixed60sRange,
        HistoricalRangeQueryCase::FullHistoryRange,
    ] {
        let janus_cells = sizes
            .iter()
            .map(|size| metric_cell(rows, *size, query_case, "janus"))
            .collect::<Vec<_>>();
        writeln!(
            file,
            "| {} | {} | {} | {} | {} | {} |",
            query_case.janus_row_label(),
            janus_cells[0],
            janus_cells[1],
            janus_cells[2],
            janus_cells[3],
            query_case.takeaway()
        )?;

        let oxigraph_cells = sizes
            .iter()
            .map(|size| metric_cell(rows, *size, query_case, "oxigraph"))
            .collect::<Vec<_>>();
        writeln!(
            file,
            "| {} | {} | {} | {} | {} | {} |",
            query_case.oxigraph_row_label(),
            oxigraph_cells[0],
            oxigraph_cells[1],
            oxigraph_cells[2],
            oxigraph_cells[3],
            match query_case {
                HistoricalRangeQueryCase::Fixed60sRange => "SPARQL timestamp filter",
                HistoricalRangeQueryCase::FullHistoryRange => "Full timestamp-filter scan",
            }
        )?;
    }

    writeln!(file)?;
    writeln!(file, "| Query Case | 10k | 50k | 100k | 500k |")?;
    writeln!(file, "| --- | --- | --- | --- | --- |")?;
    for query_case in [
        HistoricalRangeQueryCase::Fixed60sRange,
        HistoricalRangeQueryCase::FullHistoryRange,
    ] {
        let cells = sizes
            .iter()
            .map(|size| equivalence_cell(rows, *size, query_case))
            .collect::<Vec<_>>();
        writeln!(
            file,
            "| {} | {} | {} | {} | {} |",
            query_case.as_str(),
            cells[0],
            cells[1],
            cells[2],
            cells[3]
        )?;
    }

    writeln!(file)?;
    writeln!(file, "## Interpretation")?;
    writeln!(file)?;
    writeln!(
        file,
        "- fixed_60s_range tests bounded lookup over a 60-second historical interval"
    )?;
    writeln!(file, "- full_history_range tests reading all historical data")?;
    writeln!(
        file,
        "- this directly compares Janus historical retrieval with Oxigraph SPARQL timestamp FILTER over the same RDF event log data"
    )?;
    Ok(())
}

fn metric_cell(
    rows: &[HistoricalRangeSummaryRow],
    dataset_size_quads: usize,
    query_case: HistoricalRangeQueryCase,
    system: &str,
) -> String {
    let Some(row) = rows.iter().find(|row| {
        row.dataset_size_quads == dataset_size_quads && row.query_case == query_case.as_str()
    }) else {
        return String::new();
    };
    let value = if system == "janus" {
        row.janus_p50_ms
    } else {
        row.oxigraph_p50_ms
    };
    format!("{value:.3}")
}

fn equivalence_cell(
    rows: &[HistoricalRangeSummaryRow],
    dataset_size_quads: usize,
    query_case: HistoricalRangeQueryCase,
) -> &'static str {
    rows.iter()
        .find(|row| {
            row.dataset_size_quads == dataset_size_quads && row.query_case == query_case.as_str()
        })
        .map_or("no", |row| if row.equivalent { "yes" } else { "no" })
}

fn write_plot(
    path: &Path,
    rows: &[HistoricalRangeSummaryRow],
    query_case: HistoricalRangeQueryCase,
    caption: &str,
) -> Result<(), BoxError> {
    let case_rows = rows
        .iter()
        .filter(|row| row.query_case == query_case.as_str())
        .cloned()
        .collect::<Vec<_>>();
    if case_rows.is_empty() {
        return Ok(());
    }

    let root = BitMapBackend::new(path, (960, 540)).into_drawing_area();
    root.fill(&WHITE)?;

    let y_max = case_rows
        .iter()
        .flat_map(|row| [row.janus_p50_ms, row.oxigraph_p50_ms])
        .fold(0.0_f64, f64::max)
        .max(1.0)
        * 1.1;

    let x_upper = i32::try_from(case_rows.len()).map_err(|_| "too many plot points for i32")?;
    let x_range = 0_i32..x_upper;
    let mut chart = ChartBuilder::on(&root)
        .caption(caption, ("sans-serif", 24))
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(x_range, 0.0_f64..y_max)?;

    let labels = case_rows
        .iter()
        .map(|row| dataset_label(row.dataset_size_quads))
        .collect::<Vec<_>>();

    chart
        .configure_mesh()
        .x_desc("dataset size")
        .y_desc("p50 latency ms")
        .x_labels(case_rows.len())
        .x_label_formatter(&|value| {
            usize::try_from(*value)
                .ok()
                .and_then(|index| labels.get(index).cloned())
                .unwrap_or_default()
        })
        .draw()?;

    let janus_points = case_rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            Ok((
                i32::try_from(index).map_err(|_| "too many plot points for i32")?,
                row.janus_p50_ms,
            ))
        })
        .collect::<Result<Vec<_>, BoxError>>()?;
    let oxigraph_points = case_rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            Ok((
                i32::try_from(index).map_err(|_| "too many plot points for i32")?,
                row.oxigraph_p50_ms,
            ))
        })
        .collect::<Result<Vec<_>, BoxError>>()?;

    chart
        .draw_series(LineSeries::new(janus_points.clone(), &BLUE))?
        .label("Janus")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], BLUE));
    chart
        .draw_series(janus_points.into_iter().map(|point| Circle::new(point, 4, BLUE.filled())))?;

    chart
        .draw_series(LineSeries::new(oxigraph_points.clone(), &RED))?
        .label("Oxigraph")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], RED));
    chart.draw_series(
        oxigraph_points
            .into_iter()
            .map(|point| TriangleMarker::new(point, 6, RED.filled())),
    )?;

    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()?;
    root.present()?;
    Ok(())
}

fn write_query_debug_artifact(
    output_dir: &Path,
    dataset_size_quads: usize,
    query_case: HistoricalRangeQueryCase,
    run_id: usize,
    is_warmup: bool,
    query_text: &str,
) -> Result<(), BoxError> {
    let run_dir = debug_run_dir(output_dir, dataset_size_quads, query_case, run_id, is_warmup);
    ensure_output_dir(&run_dir)?;
    fs::write(run_dir.join("oxigraph_timestamp_filter_query.rq"), query_text)?;
    Ok(())
}

fn write_equivalence_debug_artifacts(
    context: &EquivalenceDebugContext<'_>,
) -> Result<(), BoxError> {
    let run_dir = debug_run_dir(
        context.output_dir,
        context.dataset.dataset_size_quads,
        context.query_case,
        context.run_id,
        context.is_warmup,
    );
    ensure_output_dir(&run_dir)?;

    write_jsonl(&run_dir.join("janus_results.jsonl"), context.janus_rows)?;
    write_jsonl(&run_dir.join("oxigraph_results.jsonl"), context.oxigraph_rows)?;

    let mut canonical_janus = context.janus_rows.to_vec();
    canonical_janus.sort_by(canonical_row_cmp);
    write_jsonl(&run_dir.join("canonical_janus_results.jsonl"), &canonical_janus)?;

    let mut canonical_oxigraph = context.oxigraph_rows.to_vec();
    canonical_oxigraph.sort_by(canonical_row_cmp);
    write_jsonl(&run_dir.join("canonical_oxigraph_results.jsonl"), &canonical_oxigraph)?;

    let report = EquivalenceReport {
        dataset_size_quads: context.dataset.dataset_size_quads,
        event_count: context.dataset.event_count,
        query_case: context.query_case.as_str().to_string(),
        range_start_ms: context.range.start_ms,
        range_end_ms: context.range.end_ms_exclusive,
        range_width_ms: context.range.end_ms_exclusive.saturating_sub(context.range.start_ms),
        janus_result_count: context.janus_rows.len(),
        oxigraph_result_count: context.oxigraph_rows.len(),
        janus_result_hash: context.janus_hash.to_string(),
        oxigraph_result_hash: context.oxigraph_hash.to_string(),
        equivalent: false,
    };
    fs::write(run_dir.join("equivalence_report.json"), serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}

fn debug_run_dir(
    output_dir: &Path,
    dataset_size_quads: usize,
    query_case: HistoricalRangeQueryCase,
    run_id: usize,
    is_warmup: bool,
) -> PathBuf {
    output_dir
        .join("equivalence_debug")
        .join(dataset_size_quads.to_string())
        .join(query_case.as_str())
        .join(format!("run_{run_id:03}_{}", if is_warmup { "warmup" } else { "measured" }))
}

fn percentile(values: &[f64], pct: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    let rank = ((pct / 100.0) * (sorted.len().saturating_sub(1) as f64)).round() as usize;
    sorted[rank]
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn dataset_label(dataset_size_quads: usize) -> String {
    match dataset_size_quads {
        10_000 => "10k".to_string(),
        50_000 => "50k".to_string(),
        100_000 => "100k".to_string(),
        500_000 => "500k".to_string(),
        value if value % 1_000_000 == 0 => format!("{}M", value / 1_000_000),
        value if value % 1_000 == 0 => format!("{}k", value / 1_000),
        value => value.to_string(),
    }
}

fn unique_config(prefix: &str) -> StreamingConfig {
    let counter = CONFIG_COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
    StreamingConfig {
        segment_base_path: format!("/tmp/janus_{prefix}_{}_{}", now_ms(), counter),
        max_batch_events: 1_000_000,
        max_batch_age_seconds: 3_600,
        max_batch_bytes: 1_000_000_000,
        sparse_interval: 64,
        entries_per_index_block: 256,
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn ensure_output_dir(path: &Path) -> Result<(), BoxError> {
    fs::create_dir_all(path)?;
    Ok(())
}

fn write_jsonl<T: Serialize>(path: &Path, rows: &[T]) -> Result<(), BoxError> {
    let mut file = File::create(path)?;
    for row in rows {
        serde_json::to_writer(&mut file, row)?;
        writeln!(file)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_60s_range_is_bounded_at_six_thousand_results() {
        let dataset = prepare_dataset(10_000).expect("dataset should build");
        let range = range_for_case(&dataset, HistoricalRangeQueryCase::Fixed60sRange, 60)
            .expect("range should build");
        let results =
            historical_range(&dataset.janus_storage, range.start_ms, range.end_ms_exclusive)
                .expect("janus range should succeed");
        assert_eq!(results.len(), 6_000);
        assert_eq!(range.end_ms_exclusive - range.start_ms, 60_000);
    }

    #[test]
    fn canonical_hash_is_order_independent() {
        let left = vec![
            CanonicalResultRow {
                subject: "http://example.org/event/00000001".to_string(),
                predicate: TIMESTAMP_PREDICATE.to_string(),
                object: typed_timestamp_literal(1),
                graph: String::new(),
                timestamp_ms: 1,
            },
            CanonicalResultRow {
                subject: "http://example.org/event/00000002".to_string(),
                predicate: TIMESTAMP_PREDICATE.to_string(),
                object: typed_timestamp_literal(2),
                graph: String::new(),
                timestamp_ms: 2,
            },
        ];
        let mut right = left.clone();
        right.reverse();
        let mut sorted_left = left.clone();
        sorted_left.sort_by(canonical_row_cmp);
        right.sort_by(canonical_row_cmp);
        assert_eq!(result_hash(&sorted_left).expect("hash"), result_hash(&right).expect("hash"));
    }
}
