use super::{
    baseline::{
        accumulate_bindings_into_baseline, baseline_statements_from_bindings,
        collect_query_defined_baseline_bindings, materialize_baseline_bindings_as_quads,
        materialize_bindings_as_static_baseline, JANUS_BASELINE_NS,
    },
    mqtt::parse_mqtt_uri,
    rdf::normalize_binding_term,
    validation::validate_baseline_graph_template,
};
use crate::{
    core::RDFEvent,
    extensions::query_options::build_evaluator,
    parsing::janusql_parser::{
        BaselineDefinition, BaselineGraphTemplate, GraphTermTemplate,
        HistoricalMaterializationKind, JanusQLParser, TripleTemplate,
    },
    registry::baseline_registry::{BaselineRegistry, BaselineSnapshot},
    storage::{segmented_storage::StreamingSegmentedStorage, util::StreamingConfig},
    stream::live_stream_processing::LiveStreamProcessing,
};
use oxigraph::{
    model::{GraphName, Literal, NamedNode, NamedOrBlankNode, Quad, Term},
    sparql::QueryResults,
    store::Store,
};
use std::{
    collections::HashMap,
    sync::{mpsc, Arc, RwLock},
    thread,
    time::Duration,
};
use tempfile::TempDir;

fn test_storage_config(prefix: &str) -> (TempDir, StreamingConfig) {
    let temp_dir = tempfile::Builder::new()
        .prefix(prefix)
        .tempdir()
        .expect("temporary test directory should be created");
    let config = StreamingConfig {
        segment_base_path: temp_dir.path().to_string_lossy().into_owned(),
        ..StreamingConfig::default()
    };
    (temp_dir, config)
}

#[test]
fn test_parse_mqtt_uri_with_port() {
    let (host, port, topic) = parse_mqtt_uri("mqtt://mybroker:1884/temperature");
    assert_eq!(host, "mybroker");
    assert_eq!(port, 1884);
    assert_eq!(topic, "temperature");
}

#[test]
fn test_parse_mqtt_uri_default_port() {
    let (host, port, topic) = parse_mqtt_uri("mqtt://mybroker/sensors");
    assert_eq!(host, "mybroker");
    assert_eq!(port, 1883);
    assert_eq!(topic, "sensors");
}

#[test]
fn test_parse_mqtts_uri() {
    let (host, port, topic) = parse_mqtt_uri("mqtts://secure-broker:8883/readings");
    assert_eq!(host, "secure-broker");
    assert_eq!(port, 8883);
    assert_eq!(topic, "readings");
}

#[test]
fn test_parse_http_uri_fallback() {
    let (host, port, topic) = parse_mqtt_uri("http://example.org/sensors");
    assert_eq!(host, "localhost");
    assert_eq!(port, 1883);
    assert_eq!(topic, "sensors");
}

#[test]
fn test_parse_http_uri_fallback_trailing_slash() {
    let (host, port, topic) = parse_mqtt_uri("http://example.org/sensors/");
    assert_eq!(host, "localhost");
    assert_eq!(port, 1883);
    assert_eq!(topic, "sensors");
}

#[test]
fn test_normalize_binding_term_strips_iri_and_literal_wrappers() {
    assert_eq!(
        normalize_binding_term("<http://example.org/sensor1>"),
        "http://example.org/sensor1"
    );
    assert_eq!(normalize_binding_term("\"42.5\""), "42.5");
    assert_eq!(
        normalize_binding_term("\"42.5\"^^<http://www.w3.org/2001/XMLSchema#decimal>"),
        "42.5"
    );
}

#[test]
fn test_materialize_query_defined_baseline_bindings_as_quads() {
    let definition = BaselineDefinition {
        name: "http://example.org/dayBaseline".to_string(),
        source_window: "http://example.org/historyDay".to_string(),
        source_windows: vec!["http://example.org/historyDay".to_string()],
        raw_query: String::new(),
        select_clause: "SELECT ?sensor (AVG(?value) AS ?dayAvgValue)".to_string(),
        where_clause: "WHERE { ?sensor <http://example.org/hasValue> ?value . }".to_string(),
        group_by_clause: Some("GROUP BY ?sensor".to_string()),
        having_clause: None,
        output_variables: vec!["?sensor".to_string(), "?dayAvgValue".to_string()],
        materialization_kind: HistoricalMaterializationKind::ExplicitBaseline,
    };
    let bindings = vec![HashMap::from([
        ("sensor".to_string(), "<http://example.org/s1>".to_string()),
        (
            "dayAvgValue".to_string(),
            "\"42.0\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
        ),
    ])];
    let template = BaselineGraphTemplate {
        baseline_name: "http://example.org/dayBaseline".to_string(),
        triples: vec![TripleTemplate {
            subject: GraphTermTemplate::Variable("sensor".to_string()),
            predicate: GraphTermTemplate::Iri("http://example.org/dayAvgValue".to_string()),
            object: GraphTermTemplate::Variable("dayAvgValue".to_string()),
        }],
    };

    let quads = materialize_baseline_bindings_as_quads(
        "http://example.org/dayBaseline",
        &definition,
        &template,
        &bindings,
    )
    .expect("baseline quads should materialize");

    assert_eq!(
        quads,
        vec![Quad::new(
            NamedOrBlankNode::NamedNode(NamedNode::new("http://example.org/s1").unwrap()),
            NamedNode::new("http://example.org/dayAvgValue").unwrap(),
            Term::Literal(Literal::new_typed_literal(
                "42.0",
                NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap(),
            )),
            GraphName::NamedNode(NamedNode::new("http://example.org/dayBaseline").unwrap()),
        )]
    );
}

#[test]
fn test_materialize_query_defined_baseline_preserves_string_and_language_literals() {
    let definition = BaselineDefinition {
        name: "http://example.org/dayBaseline".to_string(),
        source_window: "http://example.org/historyDay".to_string(),
        source_windows: vec!["http://example.org/historyDay".to_string()],
        raw_query: String::new(),
        select_clause: "SELECT ?sensor ?label ?note".to_string(),
        where_clause: "WHERE { ?sensor ?p ?o . }".to_string(),
        group_by_clause: None,
        having_clause: None,
        output_variables: vec!["?sensor".to_string(), "?label".to_string(), "?note".to_string()],
        materialization_kind: HistoricalMaterializationKind::ExplicitBaseline,
    };
    let bindings = vec![HashMap::from([
        ("sensor".to_string(), "<http://example.org/s1>".to_string()),
        ("label".to_string(), "\"pump\"".to_string()),
        ("note".to_string(), "\"bonjour\"@fr".to_string()),
    ])];
    let template = BaselineGraphTemplate {
        baseline_name: "http://example.org/dayBaseline".to_string(),
        triples: vec![
            TripleTemplate {
                subject: GraphTermTemplate::Variable("sensor".to_string()),
                predicate: GraphTermTemplate::Iri("http://example.org/label".to_string()),
                object: GraphTermTemplate::Variable("label".to_string()),
            },
            TripleTemplate {
                subject: GraphTermTemplate::Variable("sensor".to_string()),
                predicate: GraphTermTemplate::Iri("http://example.org/note".to_string()),
                object: GraphTermTemplate::Variable("note".to_string()),
            },
        ],
    };

    let quads = materialize_baseline_bindings_as_quads(
        "http://example.org/dayBaseline",
        &definition,
        &template,
        &bindings,
    )
    .expect("baseline quads should materialize");

    assert_eq!(quads.len(), 2);
    assert!(quads.iter().any(|quad| quad.predicate.as_str() == "http://example.org/label"
        && quad.object == Term::Literal(Literal::new_simple_literal("pump"))));
    assert!(quads.iter().any(|quad| quad.predicate.as_str() == "http://example.org/note"
        && quad.object
            == Term::Literal(Literal::new_language_tagged_literal("bonjour", "fr").unwrap())));
}

#[test]
fn test_materialize_query_defined_baseline_rejects_non_iri_subject() {
    let definition = BaselineDefinition {
        name: "http://example.org/dayBaseline".to_string(),
        source_window: "http://example.org/historyDay".to_string(),
        source_windows: vec!["http://example.org/historyDay".to_string()],
        raw_query: String::new(),
        select_clause: "SELECT ?sensor (AVG(?value) AS ?dayAvgValue)".to_string(),
        where_clause: "WHERE { ?sensor ?p ?value . }".to_string(),
        group_by_clause: None,
        having_clause: None,
        output_variables: vec!["?sensor".to_string(), "?dayAvgValue".to_string()],
        materialization_kind: HistoricalMaterializationKind::ExplicitBaseline,
    };
    let template = BaselineGraphTemplate {
        baseline_name: "http://example.org/dayBaseline".to_string(),
        triples: vec![TripleTemplate {
            subject: GraphTermTemplate::Variable("sensor".to_string()),
            predicate: GraphTermTemplate::Iri("http://example.org/dayAvgValue".to_string()),
            object: GraphTermTemplate::Variable("dayAvgValue".to_string()),
        }],
    };
    let bindings = vec![HashMap::from([
        ("sensor".to_string(), "\"not-an-iri\"".to_string()),
        (
            "dayAvgValue".to_string(),
            "\"42.0\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
        ),
    ])];

    let err = materialize_baseline_bindings_as_quads(
        "http://example.org/dayBaseline",
        &definition,
        &template,
        &bindings,
    )
    .expect_err("materialization should fail for non-IRI subject");

    assert!(err.to_string().contains("expected IRI or blank node subject"));
}

#[test]
fn test_validate_query_defined_baseline_rejects_missing_template_variable() {
    let definition = BaselineDefinition {
        name: "http://example.org/dayBaseline".to_string(),
        source_window: "http://example.org/historyDay".to_string(),
        source_windows: vec!["http://example.org/historyDay".to_string()],
        raw_query: String::new(),
        select_clause: "SELECT ?sensor (AVG(?value) AS ?dayAvgValue)".to_string(),
        where_clause: "WHERE { ?sensor ?p ?value . }".to_string(),
        group_by_clause: Some("GROUP BY ?sensor".to_string()),
        having_clause: None,
        output_variables: vec!["?sensor".to_string(), "?dayAvgValue".to_string()],
        materialization_kind: HistoricalMaterializationKind::ExplicitBaseline,
    };
    let template = BaselineGraphTemplate {
        baseline_name: "http://example.org/dayBaseline".to_string(),
        triples: vec![TripleTemplate {
            subject: GraphTermTemplate::Variable("sensor".to_string()),
            predicate: GraphTermTemplate::Iri("http://example.org/dayCount".to_string()),
            object: GraphTermTemplate::Variable("dayCount".to_string()),
        }],
    };

    let err = validate_baseline_graph_template(&definition, &template)
        .expect_err("validation should fail when template variable is absent");
    assert!(err.to_string().contains("references variable '?dayCount' that is not produced"));
}

#[test]
fn test_validate_query_defined_baseline_rejects_variable_predicate() {
    let definition = BaselineDefinition {
        name: "http://example.org/dayBaseline".to_string(),
        source_window: "http://example.org/historyDay".to_string(),
        source_windows: vec!["http://example.org/historyDay".to_string()],
        raw_query: String::new(),
        select_clause: "SELECT ?sensor ?pred ?dayAvgValue".to_string(),
        where_clause: "WHERE { ?sensor ?pred ?dayAvgValue . }".to_string(),
        group_by_clause: None,
        having_clause: None,
        output_variables: vec![
            "?sensor".to_string(),
            "?pred".to_string(),
            "?dayAvgValue".to_string(),
        ],
        materialization_kind: HistoricalMaterializationKind::ExplicitBaseline,
    };
    let template = BaselineGraphTemplate {
        baseline_name: "http://example.org/dayBaseline".to_string(),
        triples: vec![TripleTemplate {
            subject: GraphTermTemplate::Variable("sensor".to_string()),
            predicate: GraphTermTemplate::Variable("pred".to_string()),
            object: GraphTermTemplate::Variable("dayAvgValue".to_string()),
        }],
    };

    let err = validate_baseline_graph_template(&definition, &template)
        .expect_err("variable predicate should be rejected");
    assert!(err.to_string().contains("predicates must be concrete IRIs"));
}

#[test]
fn test_materialize_query_defined_baseline_uses_multiple_template_triples() {
    let definition = BaselineDefinition {
        name: "http://example.org/dayBaseline".to_string(),
        source_window: "http://example.org/historyDay".to_string(),
        source_windows: vec!["http://example.org/historyDay".to_string()],
        raw_query: String::new(),
        select_clause: "SELECT ?sensor ?dayAvgValue ?dayCount".to_string(),
        where_clause: "WHERE { ?sensor ?p ?o . }".to_string(),
        group_by_clause: None,
        having_clause: None,
        output_variables: vec![
            "?sensor".to_string(),
            "?dayAvgValue".to_string(),
            "?dayCount".to_string(),
        ],
        materialization_kind: HistoricalMaterializationKind::ExplicitBaseline,
    };
    let template = BaselineGraphTemplate {
        baseline_name: "http://example.org/dayBaseline".to_string(),
        triples: vec![
            TripleTemplate {
                subject: GraphTermTemplate::Variable("sensor".to_string()),
                predicate: GraphTermTemplate::Iri("http://example.org/dayAvgValue".to_string()),
                object: GraphTermTemplate::Variable("dayAvgValue".to_string()),
            },
            TripleTemplate {
                subject: GraphTermTemplate::Variable("sensor".to_string()),
                predicate: GraphTermTemplate::Iri("http://example.org/dayCount".to_string()),
                object: GraphTermTemplate::Variable("dayCount".to_string()),
            },
        ],
    };
    let bindings = vec![HashMap::from([
        ("sensor".to_string(), "<http://example.org/s1>".to_string()),
        (
            "dayAvgValue".to_string(),
            "\"42.0\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
        ),
        (
            "dayCount".to_string(),
            "\"10\"^^<http://www.w3.org/2001/XMLSchema#integer>".to_string(),
        ),
    ])];

    let quads = materialize_baseline_bindings_as_quads(
        "http://example.org/dayBaseline",
        &definition,
        &template,
        &bindings,
    )
    .expect("template materialization should succeed");

    assert_eq!(quads.len(), 2);
    assert!(quads
        .iter()
        .any(|quad| quad.predicate.as_str() == "http://example.org/dayAvgValue"));
    assert!(quads
        .iter()
        .any(|quad| quad.predicate.as_str() == "http://example.org/dayCount"));
}

#[test]
fn test_materialize_query_defined_baseline_uses_template_predicate_not_variable_name() {
    let definition = BaselineDefinition {
        name: "http://example.org/dayBaseline".to_string(),
        source_window: "http://example.org/historyDay".to_string(),
        source_windows: vec!["http://example.org/historyDay".to_string()],
        raw_query: String::new(),
        select_clause: "SELECT ?sensor ?dayAvgValue".to_string(),
        where_clause: "WHERE { ?sensor ?p ?o . }".to_string(),
        group_by_clause: None,
        having_clause: None,
        output_variables: vec!["?sensor".to_string(), "?dayAvgValue".to_string()],
        materialization_kind: HistoricalMaterializationKind::ExplicitBaseline,
    };
    let template = BaselineGraphTemplate {
        baseline_name: "http://example.org/dayBaseline".to_string(),
        triples: vec![TripleTemplate {
            subject: GraphTermTemplate::Variable("sensor".to_string()),
            predicate: GraphTermTemplate::Iri("http://example.org/customBaselineValue".to_string()),
            object: GraphTermTemplate::Variable("dayAvgValue".to_string()),
        }],
    };
    let bindings = vec![HashMap::from([
        ("sensor".to_string(), "<http://example.org/s1>".to_string()),
        (
            "dayAvgValue".to_string(),
            "\"42.0\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
        ),
    ])];

    let quads = materialize_baseline_bindings_as_quads(
        "http://example.org/dayBaseline",
        &definition,
        &template,
        &bindings,
    )
    .expect("template materialization should succeed");

    assert_eq!(quads.len(), 1);
    assert_eq!(quads[0].predicate.as_str(), "http://example.org/customBaselineValue");
}

#[test]
fn test_materialized_baseline_static_data_can_drive_live_extension_functions() {
    let query = format!(
        r#"
            PREFIX ex: <http://example.org/>
            PREFIX janus: <https://janus.rs/fn#>
            PREFIX baseline: <{}>
            REGISTER RStream <output> AS
            SELECT ?sensor ?reading
            FROM NAMED WINDOW ex:w1 ON STREAM ex:stream1 [RANGE 1000 STEP 500]
            WHERE {{
                WINDOW ex:w1 {{
                    ?sensor ex:hasReading ?reading .
                }}
                ?sensor baseline:mean ?mean .
                ?sensor baseline:sigma ?sigma .
                FILTER(janus:is_outlier(?reading, ?mean, ?sigma, 3))
            }}
        "#,
        JANUS_BASELINE_NS
    );

    let mut processor = LiveStreamProcessing::new(query).unwrap();
    processor.register_stream("http://example.org/stream1").unwrap();

    let mut binding = HashMap::new();
    binding.insert("sensor".to_string(), "<http://example.org/sensor1>".to_string());
    binding.insert(
        "mean".to_string(),
        "\"25\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
    );
    binding.insert(
        "sigma".to_string(),
        "\"2\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
    );

    materialize_bindings_as_static_baseline(&mut processor, &[binding]).unwrap();
    processor.start_processing().unwrap();
    processor
        .add_event(
            "http://example.org/stream1",
            RDFEvent::new(
                0,
                "http://example.org/sensor1",
                "http://example.org/hasReading",
                "40",
                "",
            ),
        )
        .unwrap();
    processor.close_stream("http://example.org/stream1", 3000).unwrap();
    thread::sleep(Duration::from_millis(300));

    let results = processor.collect_results(None).unwrap();
    assert!(
        results.iter().any(|result| result.bindings.contains("sensor1")),
        "expected live result to join with materialized baseline static data, got {:?}",
        results
    );
}

#[test]
fn test_baseline_statements_from_bindings_aggregate_numeric_values() {
    let bindings = vec![
        HashMap::from([
            ("sensor".to_string(), "<http://example.org/s1>".to_string()),
            (
                "mean".to_string(),
                "\"10\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
            ),
        ]),
        HashMap::from([
            ("sensor".to_string(), "<http://example.org/s1>".to_string()),
            (
                "mean".to_string(),
                "\"20\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
            ),
        ]),
    ];

    let statements = baseline_statements_from_bindings(&bindings);
    assert_eq!(
        statements,
        vec![(
            "http://example.org/s1".to_string(),
            format!("{JANUS_BASELINE_NS}mean"),
            "15".to_string()
        )]
    );
}

#[test]
fn test_last_window_mode_overwrites_previous_window_values() {
    let mut accumulator = HashMap::new();
    accumulate_bindings_into_baseline(
        &mut accumulator,
        &[HashMap::from([
            ("sensor".to_string(), "<http://example.org/s1>".to_string()),
            (
                "mean".to_string(),
                "\"10\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
            ),
        ])],
    );
    accumulator.clear();
    accumulate_bindings_into_baseline(
        &mut accumulator,
        &[HashMap::from([
            ("sensor".to_string(), "<http://example.org/s1>".to_string()),
            (
                "mean".to_string(),
                "\"30\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
            ),
        ])],
    );

    let statements = super::baseline::baseline_statements_from_accumulator(&accumulator);
    assert_eq!(
        statements,
        vec![(
            "http://example.org/s1".to_string(),
            format!("{JANUS_BASELINE_NS}mean"),
            "30".to_string()
        )]
    );
}

#[test]
fn test_query_defined_baselines_are_evaluated_over_historical_windows() {
    let (_temp_dir, config) = test_storage_config("janus_api_query_defined_baselines_");
    let storage =
        StreamingSegmentedStorage::new(config).expect("Failed to create segmented storage");

    for i in 1..=10 {
        storage
            .write_rdf(
                i * 100,
                "http://example.org/sensor1",
                "http://example.org/temperature",
                &(20 + i).to_string(),
                "http://example.org/sensors",
            )
            .expect("Failed to write RDF event");
    }
    storage.flush().expect("Failed to flush storage");

    let parser = JanusQLParser::new().expect("Failed to create parser");
    let parsed = parser
        .parse(
            r#"
PREFIX ex: <http://example.org/>

FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60 STEP 5]
FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [START 0 END 6000]

DEFINE BASELINE ex:dayBaseline ON WINDOW ex:historyDay AS
SELECT ?sensor
       (AVG(?value) AS ?dayAvgValue)
       (COUNT(?value) AS ?dayCount)
WHERE {
  ?sensor ex:temperature ?value .
}
GROUP BY ?sensor

REGISTER RStream ex:output AS
USING BASELINE ex:dayBaseline
SELECT ?sensor
WHERE {
  WINDOW ex:liveMinute {
    ?sensor ex:temperature ?value .
  }
}
GROUP BY ?sensor
            "#,
        )
        .expect("Failed to parse JanusQL query");

    let (_shutdown_tx, shutdown_rx) = mpsc::channel::<()>();
    let bindings =
        collect_query_defined_baseline_bindings(&Arc::new(storage), &parsed, &shutdown_rx)
            .expect("Failed to evaluate query-defined baselines");

    let day_baseline = bindings
        .get("http://example.org/dayBaseline")
        .expect("expected dayBaseline bindings");
    assert_eq!(day_baseline.len(), 1);
    assert!(day_baseline[0].contains_key("sensor"));
    assert!(day_baseline[0].contains_key("dayAvgValue"));
    assert!(day_baseline[0].contains_key("dayCount"));
}

#[test]
fn test_query_defined_baseline_static_graph_can_drive_live_group_by_having() {
    let query = r#"
        PREFIX : <http://example.org/>
        REGISTER RStream :output AS
        SELECT ?sensor
               (AVG(?value) AS ?minuteAvgValue)
               ?dayAvgValue
               ((AVG(?value) - ?dayAvgValue) AS ?difference)
        FROM NAMED WINDOW :liveMinute ON STREAM :stream [RANGE 60 STEP 5]
        WHERE {
            WINDOW :liveMinute {
                ?sensor :hasValue ?value .
            }
            GRAPH :dayBaseline {
                ?sensor :dayAvgValue ?dayAvgValue .
            }
        }
        GROUP BY ?sensor ?dayAvgValue
        HAVING(AVG(?value) > ?dayAvgValue)
    "#;
    let definition = BaselineDefinition {
        name: "http://example.org/dayBaseline".to_string(),
        source_window: "http://example.org/historyDay".to_string(),
        source_windows: vec!["http://example.org/historyDay".to_string()],
        raw_query: String::new(),
        select_clause: "SELECT ?sensor (AVG(?value) AS ?dayAvgValue)".to_string(),
        where_clause: "WHERE { ?sensor :hasValue ?value . }".to_string(),
        group_by_clause: Some("GROUP BY ?sensor".to_string()),
        having_clause: None,
        output_variables: vec!["?sensor".to_string(), "?dayAvgValue".to_string()],
        materialization_kind: HistoricalMaterializationKind::ExplicitBaseline,
    };
    let template = BaselineGraphTemplate {
        baseline_name: "http://example.org/dayBaseline".to_string(),
        triples: vec![TripleTemplate {
            subject: GraphTermTemplate::Variable("sensor".to_string()),
            predicate: GraphTermTemplate::Iri("http://example.org/dayAvgValue".to_string()),
            object: GraphTermTemplate::Variable("dayAvgValue".to_string()),
        }],
    };
    let bindings = vec![HashMap::from([
        ("sensor".to_string(), "<http://example.org/s1>".to_string()),
        (
            "dayAvgValue".to_string(),
            "\"25\"^^<http://www.w3.org/2001/XMLSchema#decimal>".to_string(),
        ),
    ])];

    let quads = materialize_baseline_bindings_as_quads(
        "http://example.org/dayBaseline",
        &definition,
        &template,
        &bindings,
    )
    .expect("baseline quads should materialize");

    let mut processor = LiveStreamProcessing::new(query.to_string()).unwrap();
    processor.register_stream("http://example.org/stream").unwrap();
    for quad in quads {
        processor.add_static_quad(quad);
    }
    processor.start_processing().unwrap();
    processor
        .add_events(
            "http://example.org/stream",
            vec![
                RDFEvent::new(1, "http://example.org/s1", "http://example.org/hasValue", "30", ""),
                RDFEvent::new(2, "http://example.org/s1", "http://example.org/hasValue", "32", ""),
            ],
        )
        .unwrap();
    processor.close_stream("http://example.org/stream", 100).unwrap();
    thread::sleep(Duration::from_millis(300));

    let results = processor.collect_results(None).unwrap();
    assert!(!results.is_empty(), "expected live result with baseline graph join");
    let rendered = format!("{:?}", results);
    assert!(rendered.contains("dayAvgValue"));
    assert!(rendered.contains("difference"));
}

#[test]
fn test_sliding_query_defined_baseline_rejects_mismatched_step() {
    let parser = JanusQLParser::new().expect("Failed to create parser");
    let parsed = parser
        .parse(
            r#"
PREFIX : <http://example.org/>
FROM NAMED WINDOW :liveMinute ON STREAM :stream [RANGE 60000 STEP 1000]
FROM NAMED WINDOW :sameMinuteYesterday ON LOG :stream [OFFSET 86400000 RANGE 60000 STEP 60000]
DEFINE BASELINE :yesterdayBaseline ON WINDOW :sameMinuteYesterday AS
SELECT ?sensor (AVG(?value) AS ?yesterdayAvgValue)
WHERE {
  ?sensor :hasValue ?value .
}
GROUP BY ?sensor
REGISTER RStream :output AS
USING BASELINE :yesterdayBaseline
SELECT ?sensor ?yesterdayAvgValue
WHERE {
  WINDOW :liveMinute { ?sensor :hasValue ?value . }
  GRAPH :yesterdayBaseline {
    ?sensor :yesterdayAvgValue ?yesterdayAvgValue .
  }
}
            "#,
        )
        .expect("query should parse");

    let err = super::validation::validate_query_defined_baseline_step_alignment(&parsed)
        .expect_err("mismatched steps should be rejected");
    assert!(err.to_string().contains("must match live STEP"));
}

#[test]
fn test_sliding_query_defined_baseline_snapshots_change_with_live_evaluation_time() {
    let (_temp_dir, config) = test_storage_config("janus_api_sliding_baselines_");
    let storage = Arc::new(
        StreamingSegmentedStorage::new(config).expect("Failed to create segmented storage"),
    );

    for (timestamp, value) in [(86_400_002, "10"), (86_460_000, "20")] {
        storage
            .write_rdf(
                timestamp,
                "http://example.org/sensor1",
                "http://example.org/hasValue",
                value,
                "http://example.org/history",
            )
            .expect("Failed to write historical RDF event");
    }
    storage.flush().expect("Failed to flush storage");
    for (timestamp, value) in [(86_460_002, "30"), (86_520_000, "50")] {
        storage
            .write_rdf(
                timestamp,
                "http://example.org/sensor1",
                "http://example.org/hasValue",
                value,
                "http://example.org/history",
            )
            .expect("Failed to write historical RDF event");
    }
    storage.flush().expect("Failed to flush storage");

    let parser = JanusQLParser::new().expect("Failed to create parser");
    let parsed = parser
        .parse(
            r#"
PREFIX : <http://example.org/>

FROM NAMED WINDOW :liveMinute ON STREAM :stream [RANGE 60000 STEP 60000]
FROM NAMED WINDOW :sameMinuteYesterday ON LOG :stream [OFFSET 86400000 RANGE 60000 STEP 60000]

DEFINE BASELINE :yesterdayBaseline ON WINDOW :sameMinuteYesterday AS
SELECT ?sensor
       (AVG(?value) AS ?yesterdayAvgValue)
WHERE {
  ?sensor :hasValue ?value .
}
GROUP BY ?sensor

REGISTER RStream :output AS
USING BASELINE :yesterdayBaseline
SELECT ?sensor
       (AVG(?value) AS ?currentAvgValue)
       ?yesterdayAvgValue
       ((AVG(?value) - ?yesterdayAvgValue) AS ?difference)
WHERE {
  WINDOW :liveMinute {
    ?sensor :hasValue ?value .
  }
  GRAPH :yesterdayBaseline {
    ?sensor :yesterdayAvgValue ?yesterdayAvgValue .
  }
}
GROUP BY ?sensor ?yesterdayAvgValue
HAVING(AVG(?value) > ?yesterdayAvgValue)
            "#,
        )
        .expect("Failed to parse JanusQL query");

    let baseline_registry = Arc::new(BaselineRegistry::new());
    let latest_rows = Arc::new(RwLock::new(HashMap::new()));
    assert_eq!(
        storage
            .query_rdf(86_400_001, 86_460_001)
            .expect("first historical range should query")
            .len(),
        2
    );
    assert_eq!(
        storage
            .query_rdf(86_460_001, 86_520_001)
            .expect("second historical range should query")
            .len(),
        2
    );
    let definition = parsed
        .ast
        .baseline_definitions
        .iter()
        .find(|definition| definition.name == "http://example.org/yesterdayBaseline")
        .expect("baseline definition should exist");
    let template = parsed
        .baseline_graph_templates
        .iter()
        .find(|template| template.baseline_name == "http://example.org/yesterdayBaseline")
        .expect("baseline graph template should exist");
    let first_snapshot = super::baseline::load_or_compute_baseline_snapshot(
        &storage,
        &parsed,
        definition,
        172_800_001,
        &baseline_registry,
    )
    .expect("first baseline snapshot should resolve");
    let second_snapshot = super::baseline::load_or_compute_baseline_snapshot(
        &storage,
        &parsed,
        definition,
        172_860_001,
        &baseline_registry,
    )
    .expect("second baseline snapshot should resolve");

    let live_query = r#"
PREFIX : <http://example.org/>
SELECT ?sensor
       (AVG(?value) AS ?currentAvgValue)
       ?yesterdayAvgValue
       ((AVG(?value) - ?yesterdayAvgValue) AS ?difference)
WHERE {
  GRAPH :yesterdayBaseline {
    ?sensor :yesterdayAvgValue ?yesterdayAvgValue .
  }
  GRAPH :liveMinute {
    ?sensor :hasValue ?value .
  }
}
GROUP BY ?sensor ?yesterdayAvgValue
HAVING(AVG(?value) > ?yesterdayAvgValue)
    "#;

    let run_live_evaluation = |events: Vec<(u64, &str)>,
                               snapshot: &BaselineSnapshot|
     -> HashMap<String, String> {
        super::baseline::store_latest_baseline_rows(&latest_rows, snapshot);

        let baseline_quads =
            super::baseline::materialize_baseline_snapshot_as_quads(definition, template, snapshot)
                .expect("baseline quads should materialize");

        let store = Store::new().expect("store should be created");
        for (_timestamp, value) in events {
            store
                .insert(&Quad::new(
                    NamedNode::new("http://example.org/sensor1").unwrap(),
                    NamedNode::new("http://example.org/hasValue").unwrap(),
                    Literal::new_typed_literal(
                        value,
                        NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap(),
                    ),
                    GraphName::NamedNode(NamedNode::new("http://example.org/liveMinute").unwrap()),
                ))
                .expect("live quad should insert");
        }
        for quad in baseline_quads {
            store.insert(&quad).expect("baseline quad should insert");
        }

        let parsed_query =
            build_evaluator().parse_query(live_query).expect("live SPARQL should parse");
        let results = parsed_query.on_store(&store).execute().expect("query should execute");

        let QueryResults::Solutions(solutions) = results else {
            panic!("expected SELECT solutions");
        };
        let solution = solutions
            .collect::<Result<Vec<_>, _>>()
            .expect("solutions should evaluate")
            .into_iter()
            .next()
            .expect("expected live row joined with baseline snapshot");

        let mut row = HashMap::new();
        for (variable, term) in solution.iter() {
            row.insert(variable.as_str().to_string(), normalize_binding_term(&term.to_string()));
        }
        row
    };

    let first =
        run_live_evaluation(vec![(172_740_002, "25"), (172_800_000, "35")], &first_snapshot);
    assert_eq!(first.get("yesterdayAvgValue"), Some(&"15".to_string()));
    assert_eq!(first.get("difference"), Some(&"15".to_string()));
    assert_eq!(first.get("currentAvgValue"), Some(&"30".to_string()));
    assert_ne!(first.get("currentAvgValue"), Some(&"25".to_string()));

    let second =
        run_live_evaluation(vec![(172_800_002, "60"), (172_860_000, "80")], &second_snapshot);
    assert_eq!(second.get("yesterdayAvgValue"), Some(&"40".to_string()));
    assert_eq!(second.get("difference"), Some(&"30".to_string()));
    assert_eq!(second.get("currentAvgValue"), Some(&"70".to_string()));
    assert_ne!(second.get("yesterdayAvgValue"), Some(&"15".to_string()));

    let first_snapshot = baseline_registry
        .get_snapshot("http://example.org/yesterdayBaseline", 172_800_001)
        .expect("expected snapshot at first evaluation time");
    let second_snapshot = baseline_registry
        .get_snapshot("http://example.org/yesterdayBaseline", 172_860_001)
        .expect("expected snapshot at second evaluation time");
    assert_eq!(first_snapshot.window_start, 86_400_001);
    assert_eq!(first_snapshot.window_end, 86_460_001);
    assert_eq!(second_snapshot.window_start, 86_460_001);
    assert_eq!(second_snapshot.window_end, 86_520_001);
}
