use janus::api::janus_api::{ExecutionStatus, JanusApi, ResultSource};
use janus::parsing::janusql_parser::{JanusQLParser, SourceKind, WindowDefinition, WindowType};
use janus::registry::query_registry::QueryRegistry;
use janus::storage::segmented_storage::StreamingSegmentedStorage;
use janus::storage::util::StreamingConfig;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

const SPEC_CANONICAL_LIVE_HISTORICAL_NESTED_QUERY: &str = r#"
PREFIX ex: <http://example.org/>

REGISTER RStream ex:output AS
SELECT ?sensor ?value ?historicalAverage
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:stream [RANGE 60000 STEP 30000]
FROM NAMED WINDOW ex:historicalWindow ON LOG ex:stream [START 0 END 86400000]
WHERE {
  WINDOW ex:liveWindow {
    ?sensor ex:hasValue ?value .
  }

  {
    SELECT ?sensor (AVG(?oldValue) AS ?historicalAverage)
    WHERE {
      WINDOW ex:historicalWindow {
        ?sensor ex:hasValue ?oldValue .
      }
    }
    GROUP BY ?sensor
  }

  FILTER(?value > ?historicalAverage)
}
"#;

const SPEC_HISTORICAL_ONLY_QUERY_WITHOUT_REGISTER: &str = r#"
PREFIX ex: <http://example.org/>

SELECT ?sensor ?value
FROM NAMED WINDOW ex:historicalWindow ON LOG ex:stream [START 0 END 86400000]
WHERE {
  WINDOW ex:historicalWindow {
    ?sensor ex:hasValue ?value .
  }
}
"#;

const SPEC_HISTORICAL_SLIDING_LOG_WINDOW_QUERY: &str = r#"
PREFIX ex: <http://example.org/>

REGISTER RStream ex:output AS
SELECT ?sensor ?value
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:stream [RANGE 60000 STEP 30000]
FROM NAMED WINDOW ex:previousHour ON LOG ex:stream [OFFSET 86400000 RANGE 3600000 STEP 30000]
WHERE {
  WINDOW ex:liveWindow {
    ?sensor ex:hasValue ?value .
  }

  WINDOW ex:previousHour {
    ?sensor ex:hasValue ?oldValue .
  }
}
"#;

const SPEC_LIVE_ONLY_RSTREAM_QUERY: &str = r#"
PREFIX ex: <http://example.org/>

REGISTER RStream ex:output AS
SELECT ?sensor ?value
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:stream [RANGE 60000 STEP 30000]
WHERE {
  WINDOW ex:liveWindow {
    ?sensor ex:hasValue ?value .
  }
}
"#;

const SPEC_HYBRID_FIXED_QUERY: &str = r#"
PREFIX ex: <http://example.org/>

REGISTER RStream ex:output AS
SELECT ?sensor ?liveValue ?historicalValue
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:stream [RANGE 60000 STEP 30000]
FROM NAMED WINDOW ex:historicalWindow ON LOG ex:stream [START 0 END 86400000]
WHERE {
  WINDOW ex:liveWindow {
    ?sensor ex:hasValue ?liveValue .
  }

  WINDOW ex:historicalWindow {
    ?sensor ex:hasValue ?historicalValue .
  }
}
"#;

const SPEC_HISTORICAL_ONLY_NESTED_SUBQUERY_QUERY: &str = r#"
PREFIX ex: <http://example.org/>

SELECT ?sensor ?historicalAverage
FROM NAMED WINDOW ex:historicalWindow ON LOG ex:stream [START 0 END 86400000]
WHERE {
  {
    SELECT ?sensor (AVG(?oldValue) AS ?historicalAverage)
    WHERE {
      WINDOW ex:historicalWindow {
        ?sensor ex:hasValue ?oldValue .
      }
    }
    GROUP BY ?sensor
  }
}
"#;

const SPEC_INVALID_UNDECLARED_WINDOW_QUERY: &str = r#"
PREFIX ex: <http://example.org/>

SELECT ?sensor ?value
WHERE {
  WINDOW ex:missing {
    ?sensor ex:hasValue ?value .
  }
}
"#;

const SPEC_INVALID_STREAM_FIXED_WINDOW_QUERY: &str = r#"
PREFIX ex: <http://example.org/>

SELECT ?sensor ?value
FROM NAMED WINDOW ex:historicalWindow ON STREAM ex:stream [START 0 END 86400000]
WHERE {
  WINDOW ex:historicalWindow {
    ?sensor ex:hasValue ?value .
  }
}
"#;

const SPEC_INVALID_STREAM_HISTORICAL_SLIDING_WINDOW_QUERY: &str = r#"
PREFIX ex: <http://example.org/>

SELECT ?sensor ?value
FROM NAMED WINDOW ex:historicalWindow ON STREAM ex:stream [OFFSET 86400000 RANGE 3600000 STEP 30000]
WHERE {
  WINDOW ex:historicalWindow {
    ?sensor ex:hasValue ?value .
  }
}
"#;

const SPEC_INVALID_LOG_LIVE_WINDOW_QUERY: &str = r#"
PREFIX ex: <http://example.org/>

REGISTER RStream ex:output AS
SELECT ?sensor ?value
FROM NAMED WINDOW ex:liveWindow ON LOG ex:stream [RANGE 60000 STEP 30000]
WHERE {
  WINDOW ex:liveWindow {
    ?sensor ex:hasValue ?value .
  }
}
"#;

const SPEC_INVALID_ISTREAM_QUERY: &str = r#"
PREFIX ex: <http://example.org/>

REGISTER IStream ex:output AS
SELECT ?sensor
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:stream [RANGE 60000 STEP 30000]
WHERE {
  WINDOW ex:liveWindow {
    ?sensor ex:value ?value .
  }
}
"#;

const SPEC_INVALID_DSTREAM_QUERY: &str = r#"
PREFIX ex: <http://example.org/>

REGISTER DStream ex:output AS
SELECT ?sensor
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:stream [RANGE 60000 STEP 30000]
WHERE {
  WINDOW ex:liveWindow {
    ?sensor ex:value ?value .
  }
}
"#;

const SPEC_INVALID_PROPERTY_PATH_QUERY: &str = r#"
PREFIX ex: <http://example.org/>

REGISTER RStream ex:output AS
SELECT ?sensor
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:stream [RANGE 60000 STEP 30000]
WHERE {
  WINDOW ex:liveWindow {
    ?sensor ex:connectedTo/ex:value ?value .
  }
}
"#;

const SPEC_INVALID_SERVICE_QUERY: &str = r#"
PREFIX ex: <http://example.org/>

REGISTER RStream ex:output AS
SELECT ?sensor
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:stream [RANGE 60000 STEP 30000]
WHERE {
  WINDOW ex:liveWindow {
    SERVICE <https://example.org/sparql> {
      ?sensor ex:value ?value .
    }
  }
}
"#;

fn create_test_storage_with_data(
) -> Result<(TempDir, Arc<StreamingSegmentedStorage>), std::io::Error> {
    let temp_dir = TempDir::new()?;
    let config = StreamingConfig {
        segment_base_path: temp_dir.path().to_string_lossy().into_owned(),
        max_batch_events: 10,
        max_batch_age_seconds: 1,
        max_batch_bytes: 1024,
        sparse_interval: 10,
        entries_per_index_block: 100,
    };

    let storage = StreamingSegmentedStorage::new(config)?;

    for i in 1..=50 {
        let timestamp = i * 100;
        storage.write_rdf(
            timestamp,
            &format!("http://example.org/sensor{}", i % 5),
            "http://example.org/hasValue",
            &format!("{}", 20 + (i % 10)),
            "http://example.org/sensors",
        )?;
    }

    storage.flush()?;

    Ok((temp_dir, Arc::new(storage)))
}

fn create_test_api(storage: Arc<StreamingSegmentedStorage>) -> JanusApi {
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let registry = Arc::new(QueryRegistry::new());
    JanusApi::new(parser, registry, storage).expect("Failed to create API")
}

#[test]
fn spec_canonical_live_historical_nested_query_parses() {
    let parser = JanusQLParser::new().expect("Failed to create parser");

    let parsed = parser.parse(SPEC_CANONICAL_LIVE_HISTORICAL_NESTED_QUERY).unwrap();

    assert_eq!(parsed.live_windows.len(), 1);
    assert_eq!(parsed.historical_windows.len(), 1);
    assert_eq!(parsed.historical_materialized_subqueries.len(), 1);
    assert_eq!(parsed.ast.nested_subqueries.len(), 1);
    assert_eq!(parsed.ast.baseline_definitions.len(), 1);
    assert_eq!(parsed.ast.baseline_uses.len(), 1);
}

#[test]
fn spec_live_only_rstream_query_parses() {
    let parser = JanusQLParser::new().expect("Failed to create parser");

    let parsed = parser.parse(SPEC_LIVE_ONLY_RSTREAM_QUERY).unwrap();

    assert_eq!(parsed.live_windows.len(), 1);
    assert_eq!(parsed.historical_windows.len(), 0);
    assert!(parsed.rspql_query.contains("REGISTER RStream ex:output AS"));
}

#[test]
fn spec_canonical_live_historical_nested_query_registers_and_starts_via_api() {
    let (_temp_dir, storage) = create_test_storage_with_data().expect("Failed to create storage");
    let api = create_test_api(storage);

    let metadata = api
        .register_query(
            "spec_live_historical_nested".into(),
            SPEC_CANONICAL_LIVE_HISTORICAL_NESTED_QUERY,
        )
        .expect("Failed to register query");

    assert_eq!(metadata.parsed.historical_materialized_subqueries.len(), 1);

    let _handle = api
        .start_query(&"spec_live_historical_nested".into())
        .expect("spec canonical live+histoical nested query should start");

    let bindings = api
        .get_query_defined_baseline_bindings(&"spec_live_historical_nested".into())
        .expect("materialized subquery bindings should be available");
    assert_eq!(bindings.len(), 1);
    assert_eq!(
        api.get_query_status(&"spec_live_historical_nested".into()),
        Some(ExecutionStatus::Running)
    );
}

#[test]
fn spec_historical_only_query_without_register_parses() {
    let parser = JanusQLParser::new().expect("Failed to create parser");

    let parsed = parser.parse(SPEC_HISTORICAL_ONLY_QUERY_WITHOUT_REGISTER).unwrap();

    assert_eq!(parsed.live_windows.len(), 0);
    assert_eq!(parsed.historical_windows.len(), 1);
    assert!(parsed.ast.register.is_none());
    assert_eq!(parsed.historical_windows[0].source_kind, SourceKind::Log);
}

#[test]
fn spec_hybrid_fixed_query_parses() {
    let parser = JanusQLParser::new().expect("Failed to create parser");

    let parsed = parser.parse(SPEC_HYBRID_FIXED_QUERY).unwrap();

    assert_eq!(parsed.live_windows.len(), 1);
    assert_eq!(parsed.historical_windows.len(), 1);
    assert_eq!(parsed.sparql_queries.len(), 1);
}

#[test]
fn spec_historical_only_query_without_register_executes_via_historical_query_path() {
    let (_temp_dir, storage) = create_test_storage_with_data().expect("Failed to create storage");
    let api = create_test_api(storage);

    api.register_query("spec_historical_only".into(), SPEC_HISTORICAL_ONLY_QUERY_WITHOUT_REGISTER)
        .expect("Failed to register query");

    let handle = api
        .start_query(&"spec_historical_only".into())
        .expect("historical-only public spec query should start");

    let mut results = Vec::new();
    for _ in 0..100 {
        if let Some(result) = handle.try_receive() {
            results.push(result);
        } else {
            thread::sleep(Duration::from_millis(10));
        }
    }

    assert!(!results.is_empty(), "historical-only public spec query should emit results");
    assert!(results.iter().all(|result| matches!(result.source, ResultSource::Historical)));
    assert!(results.iter().any(|result| !result.bindings.is_empty()));
}

#[test]
fn spec_hybrid_historical_sliding_query_parses() {
    let parser = JanusQLParser::new().expect("Failed to create parser");

    let parsed = parser.parse(SPEC_HISTORICAL_SLIDING_LOG_WINDOW_QUERY).unwrap();

    assert_eq!(parsed.live_windows.len(), 1);
    assert_eq!(parsed.historical_windows.len(), 1);
}

#[test]
fn spec_historical_sliding_bounds_follow_t_minus_offset_plus_range_formula() {
    let window = WindowDefinition {
        window_name: "http://example.org/previousHour".to_string(),
        source_kind: SourceKind::Log,
        source_name: "http://example.org/stream".to_string(),
        width: 3_600_000,
        slide: 30_000,
        offset: Some(86_400_000),
        start: None,
        end: None,
        window_type: WindowType::HistoricalSliding,
    };

    let evaluation_time = 200_000_000;
    let expected_start = evaluation_time - 86_400_000;
    let expected_end = expected_start + 3_600_000;

    assert_eq!(
        window.resolve_historical_bounds(evaluation_time),
        Some((expected_start, expected_end))
    );
}

#[test]
fn spec_historical_only_nested_subquery_query_parses() {
    let parser = JanusQLParser::new().expect("Failed to create parser");

    let parsed = parser.parse(SPEC_HISTORICAL_ONLY_NESTED_SUBQUERY_QUERY).unwrap();

    assert_eq!(parsed.live_windows.len(), 0);
    assert_eq!(parsed.historical_windows.len(), 1);
    assert_eq!(parsed.historical_materialized_subqueries.len(), 1);
}

#[test]
fn spec_historical_sliding_log_window_rejects_range_greater_than_offset() {
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let invalid_query = SPEC_HISTORICAL_SLIDING_LOG_WINDOW_QUERY
        .replace("OFFSET 86400000 RANGE 3600000 STEP 30000", "OFFSET 1000 RANGE 1001 STEP 30000");

    let err = parser
        .parse(&invalid_query)
        .expect_err("historical sliding log window should reject RANGE > OFFSET");

    assert!(err.to_string().contains("first window would extend beyond the evaluation time"));
}

#[test]
fn spec_invalid_undeclared_window_is_rejected() {
    let parser = JanusQLParser::new().expect("Failed to create parser");

    let err = parser
        .parse(SPEC_INVALID_UNDECLARED_WINDOW_QUERY)
        .expect_err("undeclared WINDOW blocks must be rejected");

    assert!(err.to_string().contains("references undeclared window"));
}

#[test]
fn spec_invalid_stream_fixed_window_is_rejected() {
    let parser = JanusQLParser::new().expect("Failed to create parser");

    let err = parser
        .parse(SPEC_INVALID_STREAM_FIXED_WINDOW_QUERY)
        .expect_err("historical START/END on STREAM must be rejected");

    assert!(err.to_string().contains("Historical START/END windows must use ON LOG"));
}

#[test]
fn spec_invalid_stream_historical_sliding_window_is_rejected() {
    let parser = JanusQLParser::new().expect("Failed to create parser");

    let err = parser
        .parse(SPEC_INVALID_STREAM_HISTORICAL_SLIDING_WINDOW_QUERY)
        .expect_err("historical OFFSET/RANGE/STEP on STREAM must be rejected");

    assert!(err.to_string().contains("Historical OFFSET/RANGE/STEP windows must use ON LOG"));
}

#[test]
fn spec_invalid_log_live_window_is_rejected() {
    let parser = JanusQLParser::new().expect("Failed to create parser");

    let err = parser
        .parse(SPEC_INVALID_LOG_LIVE_WINDOW_QUERY)
        .expect_err("live RANGE/STEP on LOG must be rejected");

    assert!(err.to_string().contains("Live RANGE/STEP windows must use ON STREAM"));
}

#[test]
fn spec_historical_only_nested_subquery_with_single_projected_variable_is_rejected() {
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let query = r#"
PREFIX ex: <http://example.org/>

REGISTER RStream ex:output AS
SELECT ?sensor ?value
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:stream [RANGE 60000 STEP 30000]
FROM NAMED WINDOW ex:historicalWindow ON LOG ex:stream [START 0 END 86400000]
WHERE {
  WINDOW ex:liveWindow {
    ?sensor ex:hasValue ?value .
  }

  {
    SELECT ?sensor
    WHERE {
      WINDOW ex:historicalWindow {
        ?sensor ex:hasValue ?oldValue .
      }
    }
    GROUP BY ?sensor
  }
}
"#;

    let err = parser
        .parse(query)
        .expect_err("historical-only nested subquery with one projected variable must be rejected");

    assert!(err
        .to_string()
        .contains("must project at least one join value besides the anchor variable"));
}

#[test]
fn spec_live_only_nested_subqueries_are_rejected() {
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let query = r#"
PREFIX ex: <http://example.org/>

REGISTER RStream ex:output AS
SELECT ?sensor ?value
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:stream [RANGE 60000 STEP 30000]
WHERE {
  WINDOW ex:liveWindow {
    ?sensor ex:hasValue ?value .
  }

  {
    SELECT ?sensor (AVG(?value) AS ?liveAverage)
    WHERE {
      WINDOW ex:liveWindow {
        ?sensor ex:hasValue ?value .
      }
    }
    GROUP BY ?sensor
  }
}
"#;

    let err = parser.parse(query).expect_err("live-only nested subqueries must be rejected");

    assert!(err.to_string().contains("Live-only nested subqueries"));
}

#[test]
fn spec_mixed_live_historical_nested_subqueries_are_rejected() {
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let query = r#"
PREFIX ex: <http://example.org/>

REGISTER RStream ex:output AS
SELECT ?sensor ?value
FROM NAMED WINDOW ex:liveWindow ON STREAM ex:stream [RANGE 60000 STEP 30000]
FROM NAMED WINDOW ex:historicalWindow ON LOG ex:stream [START 0 END 86400000]
WHERE {
  WINDOW ex:liveWindow {
    ?sensor ex:hasValue ?value .
  }

  {
    SELECT ?sensor (AVG(?oldValue) AS ?mixedAverage)
    WHERE {
      WINDOW ex:liveWindow {
        ?sensor ex:hasValue ?value .
      }
      WINDOW ex:historicalWindow {
        ?sensor ex:hasValue ?oldValue .
      }
    }
    GROUP BY ?sensor
  }
}
"#;

    let err = parser
        .parse(query)
        .expect_err("mixed live/historical nested subqueries must be rejected");

    assert!(err.to_string().contains("LiveHistoricalJoin"));
}

#[test]
fn spec_invalid_istream_query_is_rejected() {
    let parser = JanusQLParser::new().expect("Failed to create parser");

    let err = parser
        .parse(SPEC_INVALID_ISTREAM_QUERY)
        .expect_err("IStream must be rejected for Janus-QL Core");

    assert!(err.to_string().contains("only supports REGISTER RStream"));
}

#[test]
fn spec_invalid_dstream_query_is_rejected() {
    let parser = JanusQLParser::new().expect("Failed to create parser");

    let err = parser
        .parse(SPEC_INVALID_DSTREAM_QUERY)
        .expect_err("DStream must be rejected for Janus-QL Core");

    assert!(err.to_string().contains("only supports REGISTER RStream"));
}

#[test]
fn spec_invalid_property_path_query_is_rejected() {
    let parser = JanusQLParser::new().expect("Failed to create parser");

    let err = parser
        .parse(SPEC_INVALID_PROPERTY_PATH_QUERY)
        .expect_err("property paths must be rejected for Janus-QL Core");

    assert!(err.to_string().contains("does not support property paths"));
}

#[test]
fn spec_invalid_service_query_is_rejected() {
    let parser = JanusQLParser::new().expect("Failed to create parser");

    let err = parser
        .parse(SPEC_INVALID_SERVICE_QUERY)
        .expect_err("SERVICE must be rejected for Janus-QL Core");

    assert!(err.to_string().contains("does not support SERVICE"));
}

#[test]
fn spec_public_example_fixtures_do_not_contain_public_baseline_syntax() {
    for fixture in [
        SPEC_LIVE_ONLY_RSTREAM_QUERY,
        SPEC_HYBRID_FIXED_QUERY,
        SPEC_CANONICAL_LIVE_HISTORICAL_NESTED_QUERY,
        SPEC_HISTORICAL_ONLY_QUERY_WITHOUT_REGISTER,
        SPEC_HISTORICAL_SLIDING_LOG_WINDOW_QUERY,
        SPEC_HISTORICAL_ONLY_NESTED_SUBQUERY_QUERY,
    ] {
        assert!(!fixture.contains("DEFINE BASELINE"));
        assert!(!fixture.contains("USING BASELINE"));
    }
}
