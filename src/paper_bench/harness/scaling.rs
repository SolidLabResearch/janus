use super::data_gen::generate_citybench_dataset;
use super::helpers::{
    canonical_result_hash, collect_live_results, historical_lookup_query,
    materialize_bindings_as_static_baseline,
};
use super::system_info::current_rss_bytes;
use super::types::{
    DatasetSpec, ExecutionMode, HistoricalDataset, ScalingQueryType, ScalingRow, ScalingRunConfig,
    GRAPH_URI, LIVE_STREAM_URI, TRAFFIC_PREDICATE,
};
use crate::{
    core::RDFEvent, execution::HistoricalExecutor, parsing::janusql_parser::JanusQLParser,
    querying::oxigraph_adapter::OxigraphAdapter,
    storage::segmented_storage::StreamingSegmentedStorage,
    stream::live_stream_processing::LiveStreamProcessing,
};
use std::{collections::HashMap, sync::Arc, time::{Duration, Instant}};

pub fn run_scaling_query(
    config: ScalingRunConfig<'_>,
) -> Result<ScalingRow, Box<dyn std::error::Error>> {
    let owned_dataset;
    let dataset = if config.mode == ExecutionMode::Warm {
        config.warm_dataset.ok_or("warm mode requires prepared dataset")?
    } else {
        owned_dataset = generate_citybench_dataset(config.dataset_size_quads, config.output_dir)?;
        &owned_dataset
    };

    let started = Instant::now();
    let query_result = match config.query_type {
        ScalingQueryType::PointLookup => run_historical_query(
            Arc::clone(&dataset.storage),
            historical_lookup_query(
                dataset.spec.point_ts,
                dataset.spec.point_ts,
                Some(&dataset.spec.point_subject),
            ),
        )?,
        ScalingQueryType::FixedWindow => run_historical_query(
            Arc::clone(&dataset.storage),
            historical_lookup_query(dataset.spec.fixed_start, dataset.spec.fixed_end, None),
        )?,
        ScalingQueryType::ProportionalRange10 => run_historical_query(
            Arc::clone(&dataset.storage),
            historical_lookup_query(dataset.spec.start_ts, dataset.spec.proportional_10_end, None),
        )?,
        ScalingQueryType::ProportionalRange50 => run_historical_query(
            Arc::clone(&dataset.storage),
            historical_lookup_query(dataset.spec.start_ts, dataset.spec.proportional_50_end, None),
        )?,
        ScalingQueryType::FullRange => run_historical_query(
            Arc::clone(&dataset.storage),
            historical_lookup_query(dataset.spec.start_ts, dataset.spec.end_ts, None),
        )?,
        ScalingQueryType::HybridBaselineLookup => {
            run_hybrid_baseline_lookup(Arc::clone(&dataset.storage), &dataset.spec)?
        }
    };

    let latency_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let throughput_quads_per_sec = if latency_ms == 0.0 {
        0.0
    } else {
        query_result.logical_quads_scanned as f64 / (latency_ms / 1_000.0)
    };

    Ok(ScalingRow {
        dataset_size_quads: dataset.spec.size_quads,
        query_type: config.query_type.as_str().to_string(),
        mode: config.mode.as_str().to_string(),
        is_warmup: config.is_warmup,
        run_index: config.run_index,
        logical_quads_scanned: query_result.logical_quads_scanned,
        selectivity: if dataset.spec.size_quads == 0 {
            0.0
        } else {
            query_result.result_rows.len() as f64 / dataset.spec.size_quads as f64
        },
        result_count: query_result.result_rows.len(),
        result_hash: canonical_result_hash(&query_result.result_rows)?,
        latency_ms,
        p50_latency_ms: 0.0,
        p95_latency_ms: 0.0,
        throughput_quads_per_sec,
        peak_rss_mb: current_rss_bytes().map(|bytes| bytes as f64 / (1024.0 * 1024.0)),
        metadata: config.metadata.clone(),
    })
}

pub struct QueryExecutionResult {
    pub logical_quads_scanned: usize,
    pub result_rows: Vec<HashMap<String, String>>,
}

pub fn run_historical_query(
    storage: Arc<StreamingSegmentedStorage>,
    query: String,
) -> Result<QueryExecutionResult, Box<dyn std::error::Error>> {
    let parser = JanusQLParser::new()?;
    let parsed = parser.parse(&query)?;
    let window = parsed.historical_windows.first().ok_or("missing historical window")?;
    let logical_quads_scanned = storage
        .query(window.start.ok_or("missing start")?, window.end.ok_or("missing end")?)?
        .len();
    let executor = HistoricalExecutor::new(storage, OxigraphAdapter::new());
    let result_rows = executor.execute_fixed_window(
        window,
        parsed.sparql_queries.first().ok_or("missing historical query")?,
    )?;
    Ok(QueryExecutionResult { logical_quads_scanned, result_rows })
}

pub fn run_hybrid_baseline_lookup(
    storage: Arc<StreamingSegmentedStorage>,
    dataset: &DatasetSpec,
) -> Result<QueryExecutionResult, Box<dyn std::error::Error>> {
    let query = format!(
        r#"
        PREFIX ex: <http://example.org/>
        PREFIX baseline: <{}>

        REGISTER RStream <output> AS
        SELECT ?sensor ?liveFlow ?baselineFlow
        FROM NAMED WINDOW ex:hist ON STREAM <{}> [START {} END {}]
        FROM NAMED WINDOW ex:live ON STREAM <{}> [RANGE 5000 STEP 1000]
        USING BASELINE ex:hist AGGREGATE
        WHERE {{
            WINDOW ex:hist {{
                ?sensor ex:trafficFlow ?baselineFlow .
            }}
            WINDOW ex:live {{
                ?sensor ex:trafficFlow ?liveFlow .
            }}
            ?sensor baseline:baselineFlow ?baselineFlow .
        }}
        "#,
        super::types::BASELINE_NS, GRAPH_URI, dataset.start_ts, dataset.end_ts, LIVE_STREAM_URI
    );
    let parser = JanusQLParser::new()?;
    let parsed = parser.parse(&query)?;
    let window = parsed.historical_windows.first().ok_or("missing historical window")?;
    let logical_quads_scanned = storage.query(dataset.start_ts, dataset.end_ts)?.len();
    let executor = HistoricalExecutor::new(Arc::clone(&storage), OxigraphAdapter::new());
    let bindings = executor.execute_fixed_window(
        window,
        parsed.sparql_queries.first().ok_or("missing historical query")?,
    )?;
    let mut processor = LiveStreamProcessing::new(parsed.rspql_query.clone())?;
    processor.register_stream(LIVE_STREAM_URI)?;
    materialize_bindings_as_static_baseline(&mut processor, &bindings)?;
    processor.start_processing()?;
    processor.add_event(
        LIVE_STREAM_URI,
        RDFEvent::new(
            dataset.end_ts + 1,
            &dataset.point_subject,
            TRAFFIC_PREDICATE,
            "77",
            GRAPH_URI,
        ),
    )?;
    processor.close_stream(
        LIVE_STREAM_URI,
        i64::try_from(dataset.end_ts + 10_000).unwrap_or(i64::MAX),
    )?;
    let result_rows =
        collect_live_results(&processor, Duration::from_secs(10), Duration::from_millis(10))?
            .all_rows;
    Ok(QueryExecutionResult { logical_quads_scanned, result_rows })
}
