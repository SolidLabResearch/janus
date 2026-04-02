//! End-to-end integration tests for the two-pass historical executor.
//!
//! Exercises the full path:
//!   write events → StreamingSegmentedStorage → HistoricalExecutor
//!   → materialise_stats (Pass 1, always on) → user SPARQL query (Pass 2) → bindings
//!
//! Two-pass is now always on — no configuration required.  `JanusApi::start_query`
//! creates `HistoricalExecutor::new(storage, engine)` which automatically runs
//! both passes for every historical window.

use janus::anomaly::{HIST_MEAN_IRI, HIST_STD_DEV_IRI};
use janus::core::RDFEvent;
use janus::execution::HistoricalExecutor;
use janus::parsing::janusql_parser::{WindowDefinition, WindowType};
use janus::querying::oxigraph_adapter::OxigraphAdapter;
use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use std::sync::Arc;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_storage(tmp: &TempDir) -> Arc<StreamingSegmentedStorage> {
    let config = StreamingConfig {
        segment_base_path: tmp.path().to_str().unwrap().to_string(),
        max_batch_bytes: 10 * 1024 * 1024,
        max_batch_age_seconds: 3600,
        max_batch_events: 1_000_000,
        sparse_interval: 1000,
        entries_per_index_block: 1024,
    };
    Arc::new(StreamingSegmentedStorage::new(config).expect("storage creation failed"))
}

fn write_obs(storage: &StreamingSegmentedStorage, sensor: &str, ts: u64, val: &str) {
    storage
        .write_rdf_event(RDFEvent::new(ts, sensor, "http://ex.org/val", val, "http://ex.org/graph"))
        .expect("write_rdf_event failed");
}

fn make_window(start: u64, end: u64) -> WindowDefinition {
    WindowDefinition {
        window_name: "test_hist_window".to_string(),
        stream_name: "http://ex.org/stream".to_string(),
        width: end - start,
        slide: end - start,
        offset: None,
        start: Some(start),
        end: Some(end),
        window_type: WindowType::HistoricalFixed,
    }
}

/// Extract a float from an Oxigraph-serialised literal binding
/// (`"2.0"^^<http://www.w3.org/2001/XMLSchema#decimal>` → 2.0).
fn extract_f64(row: &std::collections::HashMap<String, String>, key: &str) -> f64 {
    let raw = row.get(key).unwrap_or_else(|| panic!("binding '{}' missing", key));
    // Oxigraph serialises as `"value"^^<datatype>` — grab the part between the quotes.
    if raw.starts_with('"') {
        raw.trim_start_matches('"').split('"').next().unwrap_or("").parse().unwrap_or(f64::NAN)
    } else {
        raw.parse().unwrap_or(f64::NAN)
    }
}

// ---------------------------------------------------------------------------
// Test 1: materialised stats are accessible from Pass 2 with no config needed
// ---------------------------------------------------------------------------

/// Write observations for two sensors, run the executor (two-pass, always on),
/// and assert that `janus:histMean` and `janus:histStdDev` facts are returned.
///
/// sensor1: [1.0, 2.0, 3.0]    → histMean = 2.0,  histStdDev ≈ 0.8165
/// sensor2: [10.0, 20.0, 30.0] → histMean = 20.0, histStdDev ≈ 8.165
#[test]
fn test_two_pass_materialises_stats_through_storage() {
    let tmp = TempDir::new().unwrap();
    let storage = make_storage(&tmp);

    write_obs(&storage, "http://ex.org/sensor1", 1_000, "1.0");
    write_obs(&storage, "http://ex.org/sensor1", 2_000, "2.0");
    write_obs(&storage, "http://ex.org/sensor1", 3_000, "3.0");

    write_obs(&storage, "http://ex.org/sensor2", 4_000, "10.0");
    write_obs(&storage, "http://ex.org/sensor2", 5_000, "20.0");
    write_obs(&storage, "http://ex.org/sensor2", 6_000, "30.0");

    // No extra configuration — two-pass is always on.
    let executor = HistoricalExecutor::new(Arc::clone(&storage), OxigraphAdapter::new());

    // Pass 2 query: read the stats materialised by Pass 1.
    let sparql = format!(
        r#"
        SELECT ?sensor ?mean ?sigma WHERE {{
            ?sensor <{mean}> ?mean ;
                    <{sigma}> ?sigma .
        }}
        ORDER BY ?sensor
        "#,
        mean = HIST_MEAN_IRI,
        sigma = HIST_STD_DEV_IRI,
    );

    let bindings = executor
        .execute_fixed_window(&make_window(0, 10_000), &sparql)
        .expect("execute_fixed_window must succeed");

    assert_eq!(bindings.len(), 2, "expected one row per sensor, got: {:?}", bindings);

    // sensor1 (comes first with ORDER BY)
    let mean1 = extract_f64(&bindings[0], "mean");
    let sigma1 = extract_f64(&bindings[0], "sigma");
    assert!((mean1 - 2.0).abs() < 1e-6, "sensor1 histMean want 2.0, got {}", mean1);
    let want_sigma1 = (2.0_f64 / 3.0_f64).sqrt();
    assert!(
        (sigma1 - want_sigma1).abs() < 1e-6,
        "sensor1 histStdDev want {:.6}, got {}",
        want_sigma1,
        sigma1
    );

    // sensor2
    let mean2 = extract_f64(&bindings[1], "mean");
    let sigma2 = extract_f64(&bindings[1], "sigma");
    assert!((mean2 - 20.0).abs() < 1e-6, "sensor2 histMean want 20.0, got {}", mean2);
    let want_sigma2 = (200.0_f64 / 3.0_f64).sqrt();
    assert!(
        (sigma2 - want_sigma2).abs() < 1e-6,
        "sensor2 histStdDev want {:.6}, got {}",
        want_sigma2,
        sigma2
    );
}

// ---------------------------------------------------------------------------
// Test 2: raw observation queries still work (Pass 2 sees the raw triples too)
// ---------------------------------------------------------------------------

/// The raw observation triples are still in the store during Pass 2, so a query
/// that only matches on `ex:val` (not on materialised stats) still works.
#[test]
fn test_raw_observations_still_queryable_in_pass2() {
    let tmp = TempDir::new().unwrap();
    let storage = make_storage(&tmp);

    write_obs(&storage, "http://ex.org/sensor1", 1_000, "42.0");
    write_obs(&storage, "http://ex.org/sensor1", 2_000, "43.0");

    let executor = HistoricalExecutor::new(Arc::clone(&storage), OxigraphAdapter::new());

    let sparql = r#"
        SELECT ?sensor ?val WHERE {
            GRAPH ?g { ?sensor <http://ex.org/val> ?val . }
        }
        ORDER BY ?val
    "#;

    let bindings = executor
        .execute_fixed_window(&make_window(0, 10_000), sparql)
        .expect("must succeed");

    assert_eq!(bindings.len(), 2, "expected 2 raw observation rows, got: {:?}", bindings);
}

// ---------------------------------------------------------------------------
// Test 3: Janus extension function in Pass 2 uses materialised histMean
// ---------------------------------------------------------------------------

/// Full two-pass run using `janus:absolute_threshold_exceeded` in Pass 2.
///
/// sensor1: [1.0, 2.0, 3.0]    → histMean = 2.0  → |2.0 − 10.0| = 8.0 > 5.0 → anomaly
/// sensor2: [8.0, 10.0, 12.0]  → histMean = 10.0 → |10.0 − 10.0| = 0.0 < 5.0 → normal
#[test]
fn test_two_pass_extension_function_filter() {
    let tmp = TempDir::new().unwrap();
    let storage = make_storage(&tmp);

    write_obs(&storage, "http://ex.org/sensor1", 1_000, "1.0");
    write_obs(&storage, "http://ex.org/sensor1", 2_000, "2.0");
    write_obs(&storage, "http://ex.org/sensor1", 3_000, "3.0");

    write_obs(&storage, "http://ex.org/sensor2", 4_000, "8.0");
    write_obs(&storage, "http://ex.org/sensor2", 5_000, "10.0");
    write_obs(&storage, "http://ex.org/sensor2", 6_000, "12.0");

    let executor = HistoricalExecutor::new(Arc::clone(&storage), OxigraphAdapter::new());

    let sparql = format!(
        r#"
        PREFIX janus: <https://janus.rs/fn#>
        PREFIX xsd:   <http://www.w3.org/2001/XMLSchema#>
        SELECT ?sensor ?mean WHERE {{
            ?sensor <{mean}> ?mean .
            FILTER(janus:absolute_threshold_exceeded(
                ?mean,
                "10.0"^^xsd:decimal,
                "5.0"^^xsd:decimal
            ))
        }}
        "#,
        mean = HIST_MEAN_IRI,
    );

    let bindings = executor
        .execute_fixed_window(&make_window(0, 10_000), &sparql)
        .expect("must succeed");

    assert_eq!(bindings.len(), 1, "expected only sensor1 anomalous, got: {:?}", bindings);

    let sensor = bindings[0].get("sensor").expect("?sensor binding missing");
    assert!(sensor.contains("sensor1"), "sensor1 should be anomalous, got: {}", sensor);
}

// ---------------------------------------------------------------------------
// Test 4: empty window is a no-op, not an error
// ---------------------------------------------------------------------------

/// An empty window (no events in range) must return empty bindings, not an error.
#[test]
fn test_empty_window_returns_empty_not_error() {
    let tmp = TempDir::new().unwrap();
    let storage = make_storage(&tmp);

    // Write events outside the query window
    write_obs(&storage, "http://ex.org/sensor1", 999_999, "1.0");

    let executor = HistoricalExecutor::new(Arc::clone(&storage), OxigraphAdapter::new());

    let sparql =
        format!("SELECT ?sensor WHERE {{ ?sensor <{}> ?mean . }}", HIST_MEAN_IRI);

    // Window [0, 100] — no events fall in this range
    let bindings = executor
        .execute_fixed_window(&make_window(0, 100), &sparql)
        .expect("empty window must not return an error");

    assert!(bindings.is_empty(), "expected empty results for empty window, got: {:?}", bindings);
}
