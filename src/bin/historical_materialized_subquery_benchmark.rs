use clap::Parser;
use janus::api::janus_api::JanusApi;
use janus::execution::historical_executor::HistoricalExecutor;
use janus::parsing::janusql_parser::{
    HistoricalMaterializationKind, JanusQLParser, ParsedJanusQuery, WindowDefinition, WindowType,
};
use janus::querying::oxigraph_adapter::OxigraphAdapter;
use janus::registry::query_registry::QueryRegistry;
use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use janus::stream::live_stream_processing::LiveStreamProcessing;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value_t = 3)]
    runs: usize,
    #[arg(long, default_value_t = 5000)]
    historical_events: usize,
    #[arg(long, default_value_t = 5)]
    entity_count: usize,
    #[arg(long)]
    output_markdown: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct ScenarioMetrics {
    parse_ast_ms: f64,
    parse_total_ms: f64,
    planning_lowering_ms: f64,
    register_ms: f64,
    historical_materialization_ms: f64,
    live_startup_ms: f64,
    baseline_bindings: usize,
    planning_summary: String,
}

#[derive(Debug, Clone)]
struct ScenarioAggregate {
    label: &'static str,
    parse_ast_ms_avg: f64,
    parse_total_ms_avg: f64,
    planning_lowering_ms_avg: f64,
    register_ms_avg: f64,
    historical_materialization_ms_avg: f64,
    live_startup_ms_avg: f64,
    baseline_bindings_avg: f64,
    planning_summary: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let output_markdown = args.output_markdown.clone().unwrap_or_else(default_output_markdown);

    let mut explicit_runs = Vec::with_capacity(args.runs);
    let mut nested_runs = Vec::with_capacity(args.runs);

    for run_index in 0..args.runs {
        explicit_runs.push(run_scenario(
            "explicit_define_baseline",
            &build_explicit_define_baseline_query(args.historical_events, args.entity_count),
            run_index,
            &args,
        )?);
        nested_runs.push(run_scenario(
            "nested_historical_subquery",
            &build_nested_historical_subquery_query(args.historical_events, args.entity_count),
            run_index,
            &args,
        )?);
    }

    let explicit = aggregate("explicit_define_baseline", &explicit_runs);
    let nested = aggregate("nested_historical_subquery", &nested_runs);

    let summary =
        render_summary(args.historical_events, args.entity_count, args.runs, &explicit, &nested);
    println!("{summary}");

    if let Some(parent) = output_markdown.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_markdown, render_markdown(&summary, &explicit, &nested))?;
    println!("markdown={}", output_markdown.display());

    Ok(())
}

fn run_scenario(
    label: &'static str,
    query: &str,
    run_index: usize,
    args: &Args,
) -> Result<ScenarioMetrics, Box<dyn std::error::Error>> {
    let parser = JanusQLParser::new()?;

    let parse_ast_started = Instant::now();
    let _ast = parser.parse_ast(query)?;
    let parse_ast_ms = elapsed_ms(parse_ast_started);

    let parse_total_started = Instant::now();
    let parsed = parser.parse(query)?;
    let parse_total_ms = elapsed_ms(parse_total_started);
    let planning_lowering_ms = (parse_total_ms - parse_ast_ms).max(0.0);

    let storage =
        create_benchmark_storage(label, args.historical_events, args.entity_count, run_index)?;
    let registry = Arc::new(QueryRegistry::new());
    let api = JanusApi::new(JanusQLParser::new()?, registry, Arc::clone(&storage))?;
    let query_id = format!("{label}_{run_index}");

    let register_started = Instant::now();
    let metadata = api.register_query(query_id.clone(), query)?;
    let register_ms = elapsed_ms(register_started);

    let historical_materialization_started = Instant::now();
    let baseline_bindings = execute_generated_materialization(&storage, &metadata.parsed)?.len();
    let historical_materialization_ms = elapsed_ms(historical_materialization_started);

    let live_startup_ms = measure_live_startup(&metadata.parsed)?;

    Ok(ScenarioMetrics {
        parse_ast_ms,
        parse_total_ms,
        planning_lowering_ms,
        register_ms,
        historical_materialization_ms,
        live_startup_ms,
        baseline_bindings,
        planning_summary: planning_summary(&parsed),
    })
}

fn execute_generated_materialization(
    storage: &Arc<StreamingSegmentedStorage>,
    parsed: &ParsedJanusQuery,
) -> Result<Vec<std::collections::HashMap<String, String>>, Box<dyn std::error::Error>> {
    let definition = parsed
        .ast
        .baseline_definitions
        .first()
        .ok_or("expected one baseline definition")?;
    let generated = parsed
        .generated_baseline_queries
        .iter()
        .find(|query| query.name == definition.name)
        .ok_or("expected generated baseline query")?;

    let source_windows = definition
        .source_windows
        .iter()
        .map(|window_name| {
            parsed
                .historical_windows
                .iter()
                .find(|window| window.window_name == *window_name)
                .ok_or_else(|| format!("missing historical source window '{window_name}'"))
        })
        .collect::<Result<Vec<&WindowDefinition>, _>>()?;

    let executor = HistoricalExecutor::new(Arc::clone(storage), OxigraphAdapter::new());
    let rows = match definition.materialization_kind {
        HistoricalMaterializationKind::ExplicitBaseline if source_windows.len() == 1 => {
            executor.execute_fixed_window(source_windows[0], &generated.sparql_query)?
        }
        HistoricalMaterializationKind::NestedSubquery => {
            let evaluation_time = source_windows
                .iter()
                .filter_map(|window| match window.window_type {
                    WindowType::HistoricalFixed => window.end,
                    WindowType::HistoricalSliding => {
                        Some(window.offset.unwrap_or(0) + window.width)
                    }
                    WindowType::Live => None,
                })
                .max()
                .unwrap_or_default();
            executor.execute_materialized_historical_subquery(
                &source_windows,
                &generated.sparql_query,
                evaluation_time,
            )?
        }
        _ => executor.execute_fixed_window(source_windows[0], &generated.sparql_query)?,
    };
    Ok(rows)
}

fn measure_live_startup(parsed: &ParsedJanusQuery) -> Result<f64, Box<dyn std::error::Error>> {
    if parsed.live_windows.is_empty() || parsed.rspql_query.is_empty() {
        return Ok(0.0);
    }

    let started = Instant::now();
    let mut processor = LiveStreamProcessing::new(parsed.rspql_query.clone())?;
    for window in &parsed.live_windows {
        processor.register_stream(&window.source_name)?;
    }
    processor.start_processing()?;
    Ok(elapsed_ms(started))
}

fn aggregate(label: &'static str, runs: &[ScenarioMetrics]) -> ScenarioAggregate {
    ScenarioAggregate {
        label,
        parse_ast_ms_avg: mean(runs.iter().map(|run| run.parse_ast_ms)),
        parse_total_ms_avg: mean(runs.iter().map(|run| run.parse_total_ms)),
        planning_lowering_ms_avg: mean(runs.iter().map(|run| run.planning_lowering_ms)),
        register_ms_avg: mean(runs.iter().map(|run| run.register_ms)),
        historical_materialization_ms_avg: mean(
            runs.iter().map(|run| run.historical_materialization_ms),
        ),
        live_startup_ms_avg: mean(runs.iter().map(|run| run.live_startup_ms)),
        baseline_bindings_avg: mean(runs.iter().map(|run| run.baseline_bindings as f64)),
        planning_summary: runs.first().map(|run| run.planning_summary.clone()).unwrap_or_default(),
    }
}

fn render_summary(
    historical_events: usize,
    entity_count: usize,
    runs: usize,
    explicit: &ScenarioAggregate,
    nested: &ScenarioAggregate,
) -> String {
    format!(
        "benchmark=nested_historical_subquery\nhistorical_events={historical_events}\nentity_count={entity_count}\nruns={runs}\n\n{}\n\n{}\n\ndelta_nested_minus_explicit\n  parse_total_ms_avg={:.3}\n  planning_lowering_ms_avg={:.3}\n  historical_materialization_ms_avg={:.3}\n  live_startup_ms_avg={:.3}",
        render_aggregate_block(explicit),
        render_aggregate_block(nested),
        nested.parse_total_ms_avg - explicit.parse_total_ms_avg,
        nested.planning_lowering_ms_avg - explicit.planning_lowering_ms_avg,
        nested.historical_materialization_ms_avg - explicit.historical_materialization_ms_avg,
        nested.live_startup_ms_avg - explicit.live_startup_ms_avg,
    )
}

fn render_aggregate_block(aggregate: &ScenarioAggregate) -> String {
    format!(
        "query={}\n  parse_ast_ms_avg={:.3}\n  parse_total_ms_avg={:.3}\n  planning_lowering_ms_avg={:.3}\n  register_ms_avg={:.3}\n  historical_materialization_ms_avg={:.3}\n  live_startup_ms_avg={:.3}\n  baseline_bindings={:.1}\n  first_result_latency_ms_avg=n/a\n  planning={}",
        aggregate.label,
        aggregate.parse_ast_ms_avg,
        aggregate.parse_total_ms_avg,
        aggregate.planning_lowering_ms_avg,
        aggregate.register_ms_avg,
        aggregate.historical_materialization_ms_avg,
        aggregate.live_startup_ms_avg,
        aggregate.baseline_bindings_avg,
        aggregate.planning_summary.replace('\n', " | "),
    )
}

fn render_markdown(
    summary: &str,
    explicit: &ScenarioAggregate,
    nested: &ScenarioAggregate,
) -> String {
    format!(
        "# Nested Historical Subquery Benchmark\n\n```text\n{summary}\n```\n\n## Planner Snapshots\n\n### {explicit_label}\n\n```text\n{explicit_plan}\n```\n\n### {nested_label}\n\n```text\n{nested_plan}\n```\n",
        explicit_label = explicit.label,
        explicit_plan = explicit.planning_summary,
        nested_label = nested.label,
        nested_plan = nested.planning_summary,
    )
}

fn planning_summary(parsed: &ParsedJanusQuery) -> String {
    parsed
        .subquery_planning_diagnostics
        .first()
        .map(|diag| diag.summary.clone())
        .unwrap_or_else(|| "No nested subquery planning diagnostics".to_string())
}

fn create_benchmark_storage(
    label: &str,
    historical_events: usize,
    entity_count: usize,
    run_index: usize,
) -> Result<Arc<StreamingSegmentedStorage>, Box<dyn std::error::Error>> {
    let config = StreamingConfig {
        segment_base_path: format!(
            "./test_data/nested_historical_subquery_benchmark_{}_{}_{}",
            label,
            timestamp_ms(),
            run_index
        ),
        max_batch_events: 128,
        max_batch_age_seconds: 1,
        max_batch_bytes: 8 * 1024,
        sparse_interval: 32,
        entries_per_index_block: 128,
    };
    let storage = StreamingSegmentedStorage::new(config)?;
    for index in 0..historical_events {
        let sensor = format!("http://example.org/sensor{}", index % entity_count.max(1));
        let value = 20 + (index % 10);
        let timestamp = ((index + 1) * 100) as u64;
        storage.write_rdf(
            timestamp,
            &sensor,
            "http://example.org/temperature",
            &value.to_string(),
            "http://example.org/stream",
        )?;
    }
    storage.flush()?;
    Ok(Arc::new(storage))
}

fn build_explicit_define_baseline_query(historical_events: usize, entity_count: usize) -> String {
    let end = history_end_ms(historical_events);
    let _ = entity_count;
    format!(
        "PREFIX ex: <http://example.org/>\n\nFROM NAMED WINDOW ex:liveMinute ON STREAM mqtt://localhost:1883/janus-bench [RANGE 60000 STEP 1000]\nFROM NAMED WINDOW ex:historyDay ON LOG ex:stream [START 0 END {end}]\n\nDEFINE BASELINE ex:dayBaseline ON WINDOW ex:historyDay AS\nSELECT ?sensor\n       (AVG(?value) AS ?dayAvgValue)\nWHERE {{\n  ?sensor ex:temperature ?value .\n}}\nGROUP BY ?sensor\n\nREGISTER RStream ex:output AS\nUSING BASELINE ex:dayBaseline\nSELECT ?sensor\n       (AVG(?value) AS ?minuteAvgValue)\n       ?dayAvgValue\n       ((AVG(?value) - ?dayAvgValue) AS ?difference)\nWHERE {{\n  WINDOW ex:liveMinute {{\n    ?sensor ex:temperature ?value .\n  }}\n  GRAPH ex:dayBaseline {{\n    ?sensor ex:dayAvgValue ?dayAvgValue .\n  }}\n}}\nGROUP BY ?sensor ?dayAvgValue\n"
    )
}

fn build_nested_historical_subquery_query(historical_events: usize, entity_count: usize) -> String {
    let end = history_end_ms(historical_events);
    let _ = entity_count;
    format!(
        "PREFIX ex: <http://example.org/>\n\nFROM NAMED WINDOW ex:liveMinute ON STREAM mqtt://localhost:1883/janus-bench [RANGE 60000 STEP 1000]\nFROM NAMED WINDOW ex:historyDay ON LOG ex:stream [START 0 END {end}]\n\nREGISTER RStream ex:output AS\nSELECT ?sensor\n       (AVG(?value) AS ?minuteAvgValue)\n       ?dayAvgValue\n       ((AVG(?value) - ?dayAvgValue) AS ?difference)\nWHERE {{\n  WINDOW ex:liveMinute {{\n    ?sensor ex:temperature ?value .\n  }}\n  {{\n    SELECT ?sensor\n           (AVG(?histValue) AS ?dayAvgValue)\n    WHERE {{\n      WINDOW ex:historyDay {{\n        ?sensor ex:temperature ?histValue .\n      }}\n    }}\n    GROUP BY ?sensor\n  }}\n}}\nGROUP BY ?sensor ?dayAvgValue\n"
    )
}

fn history_end_ms(historical_events: usize) -> u64 {
    ((historical_events + 1) * 100) as u64
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let collected = values.collect::<Vec<_>>();
    if collected.is_empty() {
        return 0.0;
    }
    collected.iter().sum::<f64>() / collected.len() as f64
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
}

fn timestamp_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
}

fn default_output_markdown() -> PathBuf {
    Path::new("logs")
        .join("benchmark")
        .join("nested_historical_subquery")
        .join(format!("{}.md", timestamp_ms()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_args() {
        let args = Args::try_parse_from(["historical_materialized_subquery_benchmark"])
            .expect("args should parse");
        assert_eq!(args.runs, 3);
        assert_eq!(args.historical_events, 5000);
        assert_eq!(args.entity_count, 5);
    }

    #[test]
    fn nested_query_contains_subquery_block() {
        let query = build_nested_historical_subquery_query(100, 2);
        assert!(query.contains("SELECT ?sensor"));
        assert!(query.contains("WINDOW ex:historyDay"));
    }
}
