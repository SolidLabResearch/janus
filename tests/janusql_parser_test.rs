//! JanusQL Parser Integration Tests
//!
//! Tests for the JanusQL query parser, verifying parsing of window definitions,
//! R2S operators, and query generation.

use janus::parsing::janusql_parser::{
    BaselineBootstrapMode, GraphTermTemplate, HistoricalWindowSpec, JanusQLParser,
    LogicalSubqueryPlan, PhysicalSubqueryPlan, SourceKind, SubqueryExecutionMode, WindowSpec,
};

#[test]
fn test_basic_live_window() {
    let parser = JanusQLParser::new().unwrap();
    let query = r"
        PREFIX sensor: <https://rsp.js/sensors/>
        PREFIX saref: <https://saref.org/core/>
        REGISTER RStream sensor:output AS
        SELECT ?temperature ?timestamp
        FROM NAMED WINDOW sensor:tempWindow ON STREAM sensor:temperatureStream [RANGE 5000 STEP 1000]
        WHERE {
            WINDOW sensor:tempWindow {
                ?event saref:hasValue ?temperature .
                ?event saref:hasTimestamp ?timestamp .
            }
        }
        ";

    let result = parser.parse(query).unwrap();
    assert_eq!(result.live_windows.len(), 1);
    assert_eq!(result.historical_windows.len(), 0);
    assert_eq!(result.live_windows[0].width, 5000);
    assert_eq!(result.live_windows[0].slide, 1000);
    assert!(!result.rspql_query.is_empty());
}

#[test]
fn test_mixed_windows() {
    let parser = JanusQLParser::new().unwrap();
    let query = r"
        PREFIX sensor: <https://rsp.js/sensors/>
        PREFIX saref: <https://saref.org/core/>
        REGISTER RStream sensor:output AS
        SELECT ?temperature ?timestamp
        FROM NAMED WINDOW sensor:tempWindow ON STREAM sensor:temperatureStream [RANGE 5000 STEP 1000]
        FROM NAMED WINDOW sensor:histWindow ON LOG sensor:temperatureStream [START 1622505600 END 1622592000]
        FROM NAMED WINDOW sensor:histSlideWindow ON LOG sensor:temperatureStream [OFFSET 1622505600 RANGE 10000 STEP 2000]
        WHERE {
            WINDOW sensor:tempWindow {
                ?event saref:hasValue ?temperature .
                ?event saref:hasTimestamp ?timestamp .
            }
            WINDOW sensor:histWindow {
                ?event saref:hasValue ?temperature .
                ?event saref:hasTimestamp ?timestamp .
            }
            WINDOW sensor:histSlideWindow {
                ?event saref:hasValue ?temperature .
                ?event saref:hasTimestamp ?timestamp .
            }
        }
        ";

    let result = parser.parse(query).unwrap();
    assert_eq!(result.live_windows.len(), 1);
    assert_eq!(result.historical_windows.len(), 2);
    assert_eq!(result.live_windows[0].width, 5000);
    assert_eq!(result.live_windows[0].slide, 1000);
    assert_eq!(result.historical_windows[0].start, Some(1_622_505_600));
    assert_eq!(result.historical_windows[0].end, Some(1_622_592_000));
    assert_eq!(result.historical_windows[0].source_kind, SourceKind::Log);
    assert_eq!(result.historical_windows[1].offset, Some(1_622_505_600));
    assert_eq!(result.historical_windows[1].width, 10000);
    assert_eq!(result.historical_windows[1].slide, 2000);
    assert_eq!(result.historical_windows[1].source_kind, SourceKind::Log);
    assert!(!result.rspql_query.is_empty());
    assert_eq!(result.sparql_queries.len(), 2);
}

#[test]
fn test_on_log_historical_windows_are_parsed_as_logs() {
    let parser = JanusQLParser::new().unwrap();
    let query = r"
        PREFIX sensor: <https://rsp.js/sensors/>
        SELECT ?temperature
        FROM NAMED WINDOW sensor:histWindow ON LOG sensor:historicalStore [START 1000 END 2000]
        FROM NAMED WINDOW sensor:histSlideWindow ON LOG sensor:historicalStore [OFFSET 1000 RANGE 1000 STEP 100]
        WHERE {
            WINDOW sensor:histWindow {
                ?event sensor:value ?temperature .
            }
            WINDOW sensor:histSlideWindow {
                ?event sensor:value ?temperature .
            }
        }
        ";

    let result = parser.parse(query).unwrap();
    assert_eq!(result.live_windows.len(), 0);
    assert_eq!(result.historical_windows.len(), 2);
    assert_eq!(result.historical_windows[0].source_kind, SourceKind::Log);
    assert_eq!(result.historical_windows[1].source_kind, SourceKind::Log);
    assert!(
        result
            .sparql_queries
            .iter()
            .all(|query| query.contains("GRAPH ?__janus_log_graph")),
        "ON LOG queries should target historical named graphs"
    );
}

#[test]
fn test_parse_ast_exposes_structured_window_specs() {
    let parser = JanusQLParser::new().unwrap();
    let query = r"
        PREFIX ex: <http://example.org/>
        REGISTER RStream ex:out AS
        SELECT ?sensor
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
        FROM NAMED WINDOW ex:hist ON LOG ex:store [START 1000 END 2000]
        WHERE {
            WINDOW ex:live { ?sensor ex:value ?value }
            WINDOW ex:hist { ?sensor ex:value ?value }
        }
    ";

    let ast = parser.parse_ast(query).unwrap();
    assert_eq!(ast.windows.len(), 2);
    assert_eq!(ast.where_windows.len(), 2);
    assert_eq!(ast.prefixes.len(), 1);

    assert!(matches!(ast.windows[0].spec, WindowSpec::LiveSliding { range: 500, step: 100 }));
    assert!(matches!(
        ast.windows[1].spec,
        WindowSpec::HistoricalFixed { start: 1000, end: 2000 }
    ));
}

#[test]
fn test_parse_ast_register_clause_is_structured() {
    let parser = JanusQLParser::new().unwrap();
    let query = r"
        PREFIX ex: <http://example.org/>
        REGISTER RStream ex:out AS
        SELECT ?sensor
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
        WHERE {
            WINDOW ex:live { ?sensor ex:value ?value }
        }
    ";

    let ast = parser.parse_ast(query).unwrap();
    let register = ast.register.expect("expected register clause");
    assert_eq!(register.operator, "RStream");
    assert_eq!(register.name, "http://example.org/out");
}

#[test]
fn test_parse_ast_multiline_window_clause_is_supported() {
    let parser = JanusQLParser::new().unwrap();
    let query = r"
        PREFIX ex: <http://example.org/>
        SELECT ?sensor
        FROM NAMED WINDOW ex:hist ON LOG ex:store
            [START 1000 END 2000]
        WHERE {
            WINDOW ex:hist { ?sensor ex:value ?value }
        }
    ";

    let ast = parser.parse_ast(query).unwrap();
    assert_eq!(ast.windows.len(), 1);
    assert!(matches!(
        ast.windows[0].spec,
        WindowSpec::HistoricalFixed { start: 1000, end: 2000 }
    ));
}

#[test]
fn test_parse_ast_on_log_historical_sliding_window() {
    let parser = JanusQLParser::new().unwrap();
    let query = r"
        PREFIX ex: <http://example.org/>
        SELECT ?sensor
        FROM NAMED WINDOW ex:hist ON LOG ex:store [OFFSET 3000 RANGE 1000 STEP 250]
        WHERE {
            WINDOW ex:hist { ?sensor ex:value ?value }
        }
    ";

    let ast = parser.parse_ast(query).unwrap();
    assert_eq!(ast.windows.len(), 1);
    assert_eq!(ast.windows[0].source_kind, SourceKind::Log);
    assert!(matches!(
        ast.windows[0].spec,
        WindowSpec::HistoricalSliding { offset: 3000, range: 1000, step: 250 }
    ));
}

#[test]
fn test_historical_start_end_on_stream_is_rejected() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        SELECT ?sensor
        FROM NAMED WINDOW ex:hist ON STREAM ex:store [START 1000 END 2000]
        WHERE {
            WINDOW ex:hist { ?sensor ex:value ?value }
        }
    "#;

    let err = parser
        .parse(query)
        .expect_err("historical START/END on STREAM must be rejected");
    assert!(err.to_string().contains("Historical START/END windows must use ON LOG"));
}

#[test]
fn test_live_range_step_on_log_is_rejected() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        SELECT ?sensor
        FROM NAMED WINDOW ex:live ON LOG ex:store [RANGE 500 STEP 100]
        WHERE {
            WINDOW ex:live { ?sensor ex:value ?value }
        }
    "#;

    let err = parser.parse(query).expect_err("live RANGE/STEP on LOG must be rejected");
    assert!(err.to_string().contains("Live RANGE/STEP windows must use ON STREAM"));
}

#[test]
fn test_top_level_window_reference_must_exist() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        SELECT ?sensor ?value
        WHERE {
            WINDOW ex:missing {
                ?sensor ex:hasValue ?value .
            }
        }
    "#;

    let err = parser.parse(query).expect_err("undeclared top-level WINDOW must be rejected");
    assert!(err.to_string().contains("references undeclared window"));
}

#[test]
fn test_duplicate_stream_window_name_is_rejected() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream ex:out AS
        SELECT ?sensor
        FROM NAMED WINDOW ex:w ON STREAM ex:stream [RANGE 500 STEP 100]
        FROM NAMED WINDOW ex:w ON STREAM ex:other [RANGE 1000 STEP 200]
        WHERE {
            WINDOW ex:w { ?sensor ex:value ?value }
        }
    "#;

    let err = parser.parse(query).expect_err("duplicate stream window names must be rejected");
    assert!(err.to_string().contains("declared more than once"));
}

#[test]
fn test_duplicate_log_window_name_is_rejected() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        SELECT ?sensor
        FROM NAMED WINDOW ex:w ON LOG ex:stream [START 0 END 1000]
        FROM NAMED WINDOW ex:w ON LOG ex:stream [START 1000 END 2000]
        WHERE {
            WINDOW ex:w { ?sensor ex:value ?value }
        }
    "#;

    let err = parser.parse(query).expect_err("duplicate log window names must be rejected");
    assert!(err.to_string().contains("declared more than once"));
}

#[test]
fn test_duplicate_mixed_window_name_is_rejected() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream ex:out AS
        SELECT ?sensor
        FROM NAMED WINDOW ex:w ON STREAM ex:stream [RANGE 500 STEP 100]
        FROM NAMED WINDOW ex:w ON LOG ex:stream [START 0 END 1000]
        WHERE {
            WINDOW ex:w { ?sensor ex:value ?value }
        }
    "#;

    let err = parser.parse(query).expect_err("duplicate mixed window names must be rejected");
    assert!(err.to_string().contains("declared more than once"));
}

#[test]
fn test_register_istream_is_rejected() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER IStream ex:out AS
        SELECT ?sensor
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
        WHERE {
            WINDOW ex:live { ?sensor ex:value ?value }
        }
    "#;

    let err = parser.parse(query).expect_err("IStream must be rejected");
    assert!(err.to_string().contains("only supports REGISTER RStream"));
}

#[test]
fn test_register_dstream_is_rejected() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER DStream ex:out AS
        SELECT ?sensor
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
        WHERE {
            WINDOW ex:live { ?sensor ex:value ?value }
        }
    "#;

    let err = parser.parse(query).expect_err("DStream must be rejected");
    assert!(err.to_string().contains("only supports REGISTER RStream"));
}

#[test]
fn test_fixed_historical_log_window_rejects_equal_start_end() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        SELECT ?sensor
        FROM NAMED WINDOW ex:hist ON LOG ex:store [START 1000 END 1000]
        WHERE {
            WINDOW ex:hist { ?sensor ex:value ?value }
        }
    "#;

    let err = parser.parse(query).expect_err("equal START/END must be rejected");
    assert!(err.to_string().contains("START less than END"));
}

#[test]
fn test_fixed_historical_log_window_rejects_start_after_end() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        SELECT ?sensor
        FROM NAMED WINDOW ex:hist ON LOG ex:store [START 2000 END 1000]
        WHERE {
            WINDOW ex:hist { ?sensor ex:value ?value }
        }
    "#;

    let err = parser.parse(query).expect_err("START greater than END must be rejected");
    assert!(err.to_string().contains("START less than END"));
}

#[test]
fn test_live_window_rejects_zero_range() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream ex:out AS
        SELECT ?sensor
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 0 STEP 100]
        WHERE {
            WINDOW ex:live { ?sensor ex:value ?value }
        }
    "#;

    let err = parser.parse(query).expect_err("zero RANGE must be rejected");
    assert!(err.to_string().contains("RANGE greater than 0"));
}

#[test]
fn test_live_window_rejects_zero_step() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream ex:out AS
        SELECT ?sensor
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 0]
        WHERE {
            WINDOW ex:live { ?sensor ex:value ?value }
        }
    "#;

    let err = parser.parse(query).expect_err("zero STEP must be rejected");
    assert!(err.to_string().contains("STEP greater than 0"));
}

#[test]
fn test_historical_sliding_window_rejects_zero_range() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        SELECT ?sensor
        FROM NAMED WINDOW ex:hist ON LOG ex:store [OFFSET 1000 RANGE 0 STEP 100]
        WHERE {
            WINDOW ex:hist { ?sensor ex:value ?value }
        }
    "#;

    let err = parser.parse(query).expect_err("historical sliding RANGE 0 must be rejected");
    assert!(err.to_string().contains("RANGE greater than 0"));
}

#[test]
fn test_historical_sliding_window_rejects_zero_step() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        SELECT ?sensor
        FROM NAMED WINDOW ex:hist ON LOG ex:store [OFFSET 1000 RANGE 100 STEP 0]
        WHERE {
            WINDOW ex:hist { ?sensor ex:value ?value }
        }
    "#;

    let err = parser.parse(query).expect_err("historical sliding STEP 0 must be rejected");
    assert!(err.to_string().contains("STEP greater than 0"));
}

#[test]
fn test_fixed_historical_log_window_parses_to_fixed_spec() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX : <http://example.org/>
        SELECT ?sensor
        FROM NAMED WINDOW :historyDay ON LOG :stream [START 0 END 86400000]
        WHERE {
            WINDOW :historyDay { ?sensor :hasValue ?value . }
        }
    "#;

    let parsed = parser.parse(query).unwrap();
    assert_eq!(parsed.historical_windows.len(), 1);
    let window = &parsed.historical_windows[0];
    assert_eq!(
        window.historical_window_spec(),
        Some(HistoricalWindowSpec::Fixed { start: 0, end: 86_400_000 })
    );
}

#[test]
fn test_sliding_historical_log_window_parses_to_sliding_spec() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX : <http://example.org/>
        SELECT ?sensor
        FROM NAMED WINDOW :sameMinuteYesterday ON LOG :stream [OFFSET 86400000 RANGE 60000 STEP 60000]
        WHERE {
            WINDOW :sameMinuteYesterday { ?sensor :hasValue ?value . }
        }
    "#;

    let parsed = parser.parse(query).unwrap();
    assert_eq!(parsed.historical_windows.len(), 1);
    let window = &parsed.historical_windows[0];
    assert_eq!(
        window.historical_window_spec(),
        Some(HistoricalWindowSpec::Sliding { offset: 86_400_000, range: 60_000, step: 60_000 })
    );
}

#[test]
fn test_sliding_historical_log_window_rejects_range_greater_than_offset() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX : <http://example.org/>
        SELECT ?sensor
        FROM NAMED WINDOW :futureCrossing ON LOG :stream [OFFSET 1000 RANGE 1001 STEP 250]
        WHERE {
            WINDOW :futureCrossing { ?sensor :hasValue ?value . }
        }
    "#;

    let err = parser.parse(query).expect_err("range greater than offset should be rejected");
    assert!(err.to_string().contains("first window would extend beyond the evaluation time"));
}

#[test]
fn test_parse_ast_extracts_window_body_with_nested_braces() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        SELECT ?sensor
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
        WHERE {
            WINDOW ex:live {
                ?sensor ex:value ?value .
                FILTER(EXISTS {
                    ?sensor ex:meta ?meta .
                })
            }
        }
    "#;

    let ast = parser.parse_ast(query).unwrap();
    assert_eq!(ast.where_windows.len(), 1);
    assert!(ast.where_windows[0].body.contains("FILTER(EXISTS"));
    assert!(ast.where_windows[0].body.contains("?sensor ex:meta ?meta"));
}

#[test]
fn test_service_in_window_block_is_rejected() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream ex:out AS
        SELECT ?sensor
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
        WHERE {
            WINDOW ex:live {
                SERVICE <https://example.org/sparql> {
                    ?sensor ex:value ?value .
                }
            }
        }
    "#;

    let err = parser.parse(query).expect_err("SERVICE must be rejected");
    assert!(err.to_string().contains("does not support SERVICE"));
}

#[test]
fn test_property_path_with_slash_is_rejected() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream ex:out AS
        SELECT ?sensor
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
        WHERE {
            WINDOW ex:live {
                ?sensor ex:p/ex:q ?value .
            }
        }
    "#;

    let err = parser.parse(query).expect_err("slash property path must be rejected");
    assert!(err.to_string().contains("does not support property paths"));
}

#[test]
fn test_property_path_with_star_is_rejected() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream ex:out AS
        SELECT ?sensor
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
        WHERE {
            WINDOW ex:live {
                ?sensor ex:p* ?value .
            }
        }
    "#;

    let err = parser.parse(query).expect_err("star property path must be rejected");
    assert!(err.to_string().contains("does not support property paths"));
}

#[test]
fn test_property_path_with_plus_is_rejected() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream ex:out AS
        SELECT ?sensor
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
        WHERE {
            WINDOW ex:live {
                ?sensor ex:p+ ?value .
            }
        }
    "#;

    let err = parser.parse(query).expect_err("plus property path must be rejected");
    assert!(err.to_string().contains("does not support property paths"));
}

#[test]
fn test_property_path_with_optional_is_rejected() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream ex:out AS
        SELECT ?sensor
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
        WHERE {
            WINDOW ex:live {
                ?sensor ex:p? ?value .
            }
        }
    "#;

    let err = parser.parse(query).expect_err("optional property path must be rejected");
    assert!(err.to_string().contains("does not support property paths"));
}

#[test]
fn test_property_path_with_inverse_is_rejected() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream ex:out AS
        SELECT ?sensor
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
        WHERE {
            WINDOW ex:live {
                ?sensor ^ex:p ?value .
            }
        }
    "#;

    let err = parser.parse(query).expect_err("inverse property path must be rejected");
    assert!(err.to_string().contains("does not support property paths"));
}

#[test]
fn test_property_path_with_alternative_is_rejected() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream ex:out AS
        SELECT ?sensor
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
        WHERE {
            WINDOW ex:live {
                ?sensor ex:p|ex:q ?value .
            }
        }
    "#;

    let err = parser.parse(query).expect_err("alternative property path must be rejected");
    assert!(err.to_string().contains("does not support property paths"));
}

#[test]
fn test_live_query_preserves_non_window_patterns_for_static_joins() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        PREFIX janus: <https://janus.rs/fn#>
        PREFIX baseline: <https://janus.rs/baseline#>
        REGISTER RStream ex:out AS
        SELECT ?sensor ?reading
        FROM NAMED WINDOW ex:hist ON LOG ex:store [START 1000 END 2000]
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
        WHERE {
            WINDOW ex:hist {
                ?sensor ex:reading ?histReading .
            }
            WINDOW ex:live {
                ?sensor ex:reading ?reading .
            }
            ?sensor baseline:mean ?mean .
            ?sensor baseline:sigma ?sigma .
            FILTER(janus:is_outlier(?reading, ?mean, ?sigma, 3))
        }
    "#;

    let parsed = parser.parse(query).unwrap();
    assert!(parsed.rspql_query.contains("?sensor baseline:mean ?mean"));
    assert!(parsed.rspql_query.contains("?sensor baseline:sigma ?sigma"));
    assert!(parsed
        .rspql_query
        .contains("FILTER(janus:is_outlier(?reading, ?mean, ?sigma, 3))"));
    assert!(parsed.rspql_query.contains("WINDOW ex:live"));
    assert!(!parsed.rspql_query.contains("WINDOW ex:hist"));
}

#[test]
fn test_parse_using_baseline_clause() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream ex:out AS
        SELECT ?sensor ?reading
        FROM NAMED WINDOW ex:hist ON LOG ex:store [START 1000 END 2000]
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
        USING BASELINE ex:hist AGGREGATE
        WHERE {
            WINDOW ex:hist { ?sensor ex:mean ?mean }
            WINDOW ex:live { ?sensor ex:reading ?reading }
        }
    "#;

    let parsed = parser.parse(query).unwrap();
    let baseline = parsed.baseline.expect("expected baseline clause");
    assert_eq!(baseline.window_name, "http://example.org/hist");
    assert_eq!(baseline.mode, BaselineBootstrapMode::Aggregate);
}

#[test]
fn test_using_baseline_requires_known_historical_window() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        SELECT ?sensor
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
        USING BASELINE ex:missing LAST
        WHERE {
            WINDOW ex:live { ?sensor ex:value ?value }
        }
    "#;

    let result = parser.parse(query);
    assert!(result.is_err());
}

#[test]
fn parse_define_baseline_with_avg_count() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60 STEP 5]
        FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [OFFSET 86400 RANGE 86400 STEP 5]
        DEFINE BASELINE ex:dayBaseline ON WINDOW ex:historyDay AS
        SELECT ?sensor
               (AVG(?value) AS ?dayAvgValue)
               (COUNT(?value) AS ?dayCount)
        WHERE {
          ?sensor ex:hasValue ?value .
        }
        GROUP BY ?sensor
        REGISTER RStream ex:output AS
        USING BASELINE ex:dayBaseline
        SELECT ?sensor ?dayAvgValue
        WHERE {
          WINDOW ex:liveMinute {
            ?sensor ex:hasValue ?value .
          }
        }
        GROUP BY ?sensor ?dayAvgValue
    "#;

    let parsed = parser.parse(query).unwrap();
    assert_eq!(parsed.ast.baseline_definitions.len(), 1);
    let definition = &parsed.ast.baseline_definitions[0];
    assert_eq!(definition.name, "http://example.org/dayBaseline");
    assert_eq!(definition.source_window, "http://example.org/historyDay");
    assert_eq!(definition.output_variables, vec!["?sensor", "?dayAvgValue", "?dayCount"]);
    assert_eq!(parsed.baseline_graph_templates.len(), 0);
    assert_eq!(parsed.generated_baseline_queries.len(), 1);
}

#[test]
fn test_define_baseline_parser_exposes_name_window_and_projection() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX : <http://example.org/>
        FROM NAMED WINDOW :sameMinuteYesterday ON LOG :stream [OFFSET 86400000 RANGE 60000 STEP 60000]
        DEFINE BASELINE :yesterdayBaseline ON WINDOW :sameMinuteYesterday AS
        SELECT ?sensor
               (AVG(?value) AS ?yesterdayAvgValue)
        WHERE {
          ?sensor :hasValue ?value .
        }
        GROUP BY ?sensor
        REGISTER RStream :output AS
        SELECT ?sensor
        WHERE { }
    "#;

    let parsed = parser.parse(query).unwrap();
    let definition = &parsed.ast.baseline_definitions[0];
    assert_eq!(definition.name, "http://example.org/yesterdayBaseline");
    assert_eq!(definition.source_window, "http://example.org/sameMinuteYesterday");
    assert!(definition.output_variables.contains(&"?sensor".to_string()));
    assert!(definition.output_variables.contains(&"?yesterdayAvgValue".to_string()));
    assert!(parsed.generated_baseline_queries[0].sparql_query.contains("AVG(?value)"));
}

#[test]
fn test_using_baseline_parser_tracks_named_baseline_use() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
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
        SELECT ?sensor ?yesterdayAvgValue
        WHERE {
          WINDOW :liveMinute {
            ?sensor :hasValue ?value .
          }
          GRAPH :yesterdayBaseline {
            ?sensor :yesterdayAvgValue ?yesterdayAvgValue .
          }
        }
    "#;

    let parsed = parser.parse(query).unwrap();
    assert_eq!(parsed.ast.baseline_uses.len(), 1);
    assert_eq!(parsed.ast.baseline_uses[0].name, "http://example.org/yesterdayBaseline");
}

#[test]
fn baseline_select_does_not_override_main_select() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60 STEP 5]
        FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [OFFSET 86400 RANGE 86400 STEP 5]
        DEFINE BASELINE ex:dayBaseline ON WINDOW ex:historyDay AS
        SELECT ?sensor (AVG(?value) AS ?dayAvgValue)
        WHERE {
          ?sensor ex:hasValue ?value .
        }
        GROUP BY ?sensor
        REGISTER RStream ex:output AS
        USING BASELINE ex:dayBaseline
        SELECT ?sensor (AVG(?value) AS ?minuteAvgValue) ?dayAvgValue
        WHERE {
          WINDOW ex:liveMinute {
            ?sensor ex:hasValue ?value .
          }
        }
        GROUP BY ?sensor ?dayAvgValue
    "#;

    let parsed = parser.parse(query).unwrap();
    assert!(parsed.select_clause.contains("?minuteAvgValue"));
    assert!(!parsed.select_clause.contains("?dayCount"));
    assert_eq!(
        parsed.ast.baseline_definitions[0].select_clause,
        "SELECT ?sensor (AVG(?value) AS ?dayAvgValue)"
    );
}

#[test]
fn using_baseline_is_parsed_after_register() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60 STEP 5]
        FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [OFFSET 86400 RANGE 86400 STEP 5]
        DEFINE BASELINE ex:dayBaseline ON WINDOW ex:historyDay AS
        SELECT ?sensor
        WHERE {
          ?sensor ex:hasValue ?value .
        }
        GROUP BY ?sensor
        REGISTER RStream ex:output AS
        USING BASELINE ex:dayBaseline
        USING BASELINE ex:weekBaseline
        SELECT ?sensor
        WHERE {
          WINDOW ex:liveMinute {
            ?sensor ex:hasValue ?value .
          }
        }
    "#;

    let result = parser.parse(query);
    assert!(result.is_err());

    let query = query.replace(
        "DEFINE BASELINE ex:dayBaseline ON WINDOW ex:historyDay AS\n        SELECT ?sensor\n        WHERE {\n          ?sensor ex:hasValue ?value .\n        }\n        GROUP BY ?sensor",
        "DEFINE BASELINE ex:dayBaseline ON WINDOW ex:historyDay AS\n        SELECT ?sensor\n        WHERE {\n          ?sensor ex:hasValue ?value .\n        }\n        GROUP BY ?sensor\n        DEFINE BASELINE ex:weekBaseline ON WINDOW ex:historyDay AS\n        SELECT ?sensor\n        WHERE {\n          ?sensor ex:hasValue ?value .\n        }\n        GROUP BY ?sensor",
    );
    let parsed = parser.parse(&query).unwrap();
    assert_eq!(parsed.ast.baseline_uses.len(), 2);
    assert_eq!(parsed.ast.baseline_uses[0].name, "http://example.org/dayBaseline");
    assert_eq!(parsed.ast.baseline_uses[1].name, "http://example.org/weekBaseline");
}

#[test]
fn generated_baseline_query_wraps_where_body_in_log_graph() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [OFFSET 86400 RANGE 86400 STEP 5]
        DEFINE BASELINE ex:dayBaseline ON WINDOW ex:historyDay AS
        SELECT ?sensor (AVG(?value) AS ?dayAvgValue) (COUNT(?value) AS ?dayCount)
        WHERE {
          ?sensor ex:hasValue ?value .
        }
        GROUP BY ?sensor
        REGISTER RStream ex:output AS
        SELECT ?sensor
        WHERE { }
    "#;

    let parsed = parser.parse(query).unwrap();
    let generated = &parsed.generated_baseline_queries[0];
    assert!(generated.sparql_query.contains("GRAPH ?__janus_log_graph"));
    assert!(generated.sparql_query.contains("?sensor ex:hasValue ?value ."));
    assert!(generated.sparql_query.contains("GROUP BY ?sensor"));
}

#[test]
fn historical_only_query_preserves_group_by_and_having_in_generated_sparql() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        SELECT ?sensor (AVG(?value) AS ?avgValue)
        FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [START 0 END 86400000]
        WHERE {
          WINDOW ex:historyDay {
            ?sensor ex:hasValue ?value .
          }
        }
        GROUP BY ?sensor
        HAVING(AVG(?value) > 10)
    "#;

    let parsed = parser.parse(query).unwrap();
    assert_eq!(parsed.live_windows.len(), 0);
    assert_eq!(parsed.historical_windows.len(), 1);
    assert_eq!(parsed.sparql_queries.len(), 1);
    assert!(parsed.sparql_queries[0].contains("GROUP BY ?sensor"));
    assert!(parsed.sparql_queries[0].contains("HAVING(AVG(?value) > 10)"));
}

#[test]
fn nested_historical_subquery_is_lowered_to_historical_materialization() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX : <http://example.org/>
        FROM NAMED WINDOW :liveMinute ON STREAM :stream [RANGE 60000 STEP 1000]
        FROM NAMED WINDOW :historyDay ON LOG :stream [START 0 END 86400000]
        REGISTER RStream :output AS
        SELECT ?sensor
               (AVG(?liveValue) AS ?minuteAvgValue)
               ?dayAvgValue
        WHERE {
          WINDOW :liveMinute {
            ?sensor :hasValue ?liveValue .
          }
          {
            SELECT ?sensor (AVG(?histValue) AS ?dayAvgValue)
            WHERE {
              WINDOW :historyDay {
                ?sensor :hasValue ?histValue .
              }
            }
            GROUP BY ?sensor
            HAVING(AVG(?histValue) > 0)
          }
        }
        GROUP BY ?sensor ?dayAvgValue
    "#;

    let parsed = parser.parse(query).unwrap();
    assert_eq!(parsed.historical_materialized_subqueries.len(), 1);
    assert_eq!(parsed.planned_subqueries.len(), 1);
    assert_eq!(parsed.ast.nested_subqueries.len(), 1);
    assert_eq!(parsed.ast.baseline_definitions.len(), 1);
    assert_eq!(parsed.ast.baseline_uses.len(), 1);
    assert_eq!(parsed.generated_baseline_queries.len(), 1);
    assert_eq!(parsed.planning_statistics.historical_materialized_subqueries, 1);
    assert_eq!(parsed.planning_statistics.live_subqueries, 0);
    assert_eq!(parsed.planning_statistics.live_historical_joins, 0);

    let planned = &parsed.planned_subqueries[0];
    assert_eq!(planned.execution_mode, SubqueryExecutionMode::HistoricalMaterializedOnce);
    assert_eq!(planned.physical_plan, PhysicalSubqueryPlan::MaterializeHistoricalResult);
    assert!(matches!(
        &planned.logical_plan,
        LogicalSubqueryPlan::HistoricalMaterialized { windows } if windows.len() == 1
    ));
    assert_eq!(parsed.subquery_planning_diagnostics.len(), 1);
    let diag = &parsed.subquery_planning_diagnostics[0];
    assert!(diag.summary.contains("Nested subquery #0"));
    assert!(diag.summary.contains("Execution mode: HistoricalMaterializedOnce"));
    assert!(diag.summary.contains("Logical plan:"));
    assert!(diag.summary.contains("Physical plan:"));

    let baseline = &parsed.ast.baseline_definitions[0];
    assert_eq!(baseline.source_window, "http://example.org/historyDay");
    assert_eq!(baseline.output_variables, vec!["?sensor", "?dayAvgValue"]);
    assert_eq!(baseline.having_clause.as_deref(), Some("HAVING(AVG(?histValue) > 0)"));
    assert_eq!(baseline.source_windows, vec!["http://example.org/historyDay".to_string()]);
    assert!(baseline.name.contains("__hist_mat_subquery_0"));
    assert!(parsed
        .where_clause
        .contains("GRAPH <https://janus.rs/materialized-history/__hist_mat_subquery_0>"));
    assert!(parsed.generated_baseline_queries[0].sparql_query.contains("AVG(?histValue)"));
    assert!(parsed.generated_baseline_queries[0].sparql_query.contains("GRAPH :historyDay"));
    assert!(parsed.generated_baseline_queries[0]
        .sparql_query
        .contains("HAVING(AVG(?histValue) > 0)"));
}

#[test]
fn nested_historical_subquery_must_reference_historical_log_window() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX : <http://example.org/>
        FROM NAMED WINDOW :liveMinute ON STREAM :stream [RANGE 60000 STEP 1000]
        REGISTER RStream :output AS
        SELECT ?sensor
        WHERE {
          WINDOW :liveMinute {
            ?sensor :hasValue ?liveValue .
          }
          {
            SELECT ?sensor (AVG(?liveValue) AS ?minuteAvgValue)
            WHERE {
              WINDOW :liveMinute {
                ?sensor :hasValue ?liveValue .
              }
            }
            GROUP BY ?sensor
          }
        }
    "#;

    let err = parser.parse(query).unwrap_err().to_string();
    assert!(err.contains("Live-only nested subqueries require LiveSubquery planning"));
}

#[test]
fn nested_historical_subquery_supports_multiple_historical_windows() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX : <http://example.org/>
        FROM NAMED WINDOW :liveMinute ON STREAM :stream [RANGE 60000 STEP 1000]
        FROM NAMED WINDOW :historyDay ON LOG :stream [START 0 END 86400000]
        FROM NAMED WINDOW :historyWeek ON LOG :stream [START 0 END 604800000]
        REGISTER RStream :output AS
        SELECT ?sensor ?dayAvgValue
        WHERE {
          WINDOW :liveMinute {
            ?sensor :hasValue ?liveValue .
          }
          {
            SELECT ?sensor (AVG(?histValue) AS ?dayAvgValue)
            WHERE {
              WINDOW :historyDay {
                ?sensor :hasValue ?histValue .
              }
              WINDOW :historyWeek {
                ?sensor :hasValue ?histValue .
              }
            }
            GROUP BY ?sensor
          }
        }
    "#;

    let parsed = parser.parse(query).unwrap();
    assert_eq!(parsed.historical_materialized_subqueries.len(), 1);
    assert_eq!(parsed.planning_statistics.historical_materialized_subqueries, 1);
    let materialized = &parsed.historical_materialized_subqueries[0];
    assert_eq!(materialized.execution_mode, SubqueryExecutionMode::HistoricalMaterializedOnce);
    assert_eq!(materialized.dependencies.historical_windows.len(), 2);
    assert_eq!(
        parsed.planned_subqueries[0].physical_plan,
        PhysicalSubqueryPlan::MaterializeHistoricalResult
    );
    assert_eq!(parsed.ast.baseline_definitions.len(), 1);
    let definition = &parsed.ast.baseline_definitions[0];
    assert_eq!(definition.source_windows.len(), 2);
    assert!(definition.source_windows.contains(&"http://example.org/historyDay".to_string()));
    assert!(definition
        .source_windows
        .contains(&"http://example.org/historyWeek".to_string()));
    assert!(parsed.generated_baseline_queries[0].sparql_query.contains("GRAPH :historyDay"));
    assert!(parsed.generated_baseline_queries[0].sparql_query.contains("GRAPH :historyWeek"));
}

#[test]
fn mixed_live_historical_nested_subquery_is_rejected_cleanly() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX : <http://example.org/>
        FROM NAMED WINDOW :liveMinute ON STREAM :stream [RANGE 60000 STEP 1000]
        FROM NAMED WINDOW :historyDay ON LOG :stream [START 0 END 86400000]
        REGISTER RStream :output AS
        SELECT ?sensor ?liveValue ?histAvg
        WHERE {
          {
            SELECT ?sensor ?liveValue (AVG(?histValue) AS ?histAvg)
            WHERE {
              WINDOW :liveMinute {
                ?sensor :hasValue ?liveValue .
              }
              WINDOW :historyDay {
                ?sensor :hasValue ?histValue .
              }
            }
            GROUP BY ?sensor ?liveValue
          }
        }
    "#;

    let err = parser.parse(query).unwrap_err().to_string();
    assert!(err.contains("LiveHistoricalJoin planning"));
}

#[test]
fn nested_subquery_without_window_reference_is_rejected_cleanly() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX : <http://example.org/>
        REGISTER RStream :output AS
        SELECT ?sensor
        WHERE {
          {
            SELECT ?sensor
            WHERE {
              ?sensor :hasValue ?histValue .
            }
          }
        }
    "#;

    let err = parser.parse(query).unwrap_err().to_string();
    assert!(err.contains("must reference at least one known WINDOW block"));
}

#[test]
fn unknown_window_reference_in_nested_subquery_is_rejected_cleanly() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX : <http://example.org/>
        REGISTER RStream :output AS
        SELECT ?sensor
        WHERE {
          {
            SELECT ?sensor
            WHERE {
              WINDOW :missing {
                ?sensor :hasValue ?histValue .
              }
            }
          }
        }
    "#;

    let err = parser.parse(query).unwrap_err().to_string();
    assert!(
        err.contains("references undeclared window") || err.contains("references unknown window")
    );
}

#[test]
fn baseline_source_window_must_exist() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60 STEP 5]
        DEFINE BASELINE ex:dayBaseline ON WINDOW ex:missing AS
        SELECT ?sensor
        WHERE {
          ?sensor ex:hasValue ?value .
        }
        REGISTER RStream ex:output AS
        SELECT ?sensor
        WHERE {
          WINDOW ex:liveMinute { ?sensor ex:hasValue ?value . }
        }
    "#;

    assert!(parser.parse(query).is_err());
}

#[test]
fn baseline_source_window_must_be_historical_log() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60 STEP 5]
        DEFINE BASELINE ex:dayBaseline ON WINDOW ex:liveMinute AS
        SELECT ?sensor
        WHERE {
          ?sensor ex:hasValue ?value .
        }
        REGISTER RStream ex:output AS
        SELECT ?sensor
        WHERE {
          WINDOW ex:liveMinute { ?sensor ex:hasValue ?value . }
        }
    "#;

    assert!(parser.parse(query).is_err());
}

#[test]
fn using_baseline_must_reference_defined_baseline() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60 STEP 5]
        FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [OFFSET 86400 RANGE 86400 STEP 5]
        REGISTER RStream ex:output AS
        USING BASELINE ex:dayBaseline
        SELECT ?sensor
        WHERE {
          WINDOW ex:liveMinute { ?sensor ex:hasValue ?value . }
        }
    "#;

    assert!(parser.parse(query).is_err());
}

#[test]
fn old_using_baseline_aggregate_syntax_still_passes() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        REGISTER RStream ex:out AS
        SELECT ?sensor ?reading
        FROM NAMED WINDOW ex:hist ON LOG ex:store [START 1000 END 2000]
        FROM NAMED WINDOW ex:live ON STREAM ex:stream [RANGE 500 STEP 100]
        USING BASELINE ex:hist AGGREGATE
        WHERE {
            WINDOW ex:hist { ?sensor ex:mean ?mean }
            WINDOW ex:live { ?sensor ex:reading ?reading }
        }
    "#;

    let parsed = parser.parse(query).unwrap();
    assert_eq!(parsed.baseline.unwrap().mode, BaselineBootstrapMode::Aggregate);
}

#[test]
fn main_select_can_compute_difference_between_live_avg_and_baseline_avg() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60000 STEP 1000]
        FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [OFFSET 86400000 RANGE 86400000 STEP 1000]
        DEFINE BASELINE ex:dayBaseline ON WINDOW ex:historyDay AS
        SELECT ?sensor
               (AVG(?value) AS ?dayAvgValue)
        WHERE {
          ?sensor ex:hasValue ?value .
        }
        GROUP BY ?sensor
        REGISTER RStream ex:output AS
        USING BASELINE ex:dayBaseline
        SELECT ?sensor
               (AVG(?value) AS ?minuteAvgValue)
               ?dayAvgValue
               ((AVG(?value) - ?dayAvgValue) AS ?difference)
        WHERE {
          WINDOW ex:liveMinute {
            ?sensor ex:hasValue ?value .
          }
          GRAPH ex:dayBaseline {
            ?sensor ex:dayAvgValue ?dayAvgValue .
          }
        }
        GROUP BY ?sensor ?dayAvgValue
        HAVING(AVG(?value) > ?dayAvgValue)
    "#;

    let parsed = parser.parse(query).unwrap();
    assert!(parsed.select_clause.contains("?difference"));
    assert_eq!(parsed.ast.baseline_uses.len(), 1);
    assert!(parsed.where_clause.contains("GROUP BY ?sensor ?dayAvgValue"));
    assert!(parsed.where_clause.contains("HAVING(AVG(?value) > ?dayAvgValue)"));
}

#[test]
fn query_defined_baseline_graph_template_is_extracted_structurally() {
    let parser = JanusQLParser::new().unwrap();
    let query = r#"
        PREFIX ex: <http://example.org/>
        FROM NAMED WINDOW ex:liveMinute ON STREAM ex:stream [RANGE 60 STEP 5]
        FROM NAMED WINDOW ex:historyDay ON LOG ex:stream [OFFSET 86400 RANGE 86400 STEP 5]
        DEFINE BASELINE ex:dayBaseline ON WINDOW ex:historyDay AS
        SELECT ?sensor (AVG(?value) AS ?dayAvgValue) (COUNT(?value) AS ?dayCount)
        WHERE {
          ?sensor ex:hasValue ?value .
        }
        GROUP BY ?sensor
        REGISTER RStream ex:output AS
        USING BASELINE ex:dayBaseline
        SELECT ?sensor ?dayAvgValue ?dayCount
        WHERE {
          WINDOW ex:liveMinute {
            ?sensor ex:hasValue ?value .
          }
          GRAPH ex:dayBaseline {
            ?sensor ex:dayAvgValue ?dayAvgValue .
            ?sensor ex:dayCount ?dayCount .
          }
        }
    "#;

    let parsed = parser.parse(query).unwrap();
    assert_eq!(parsed.baseline_graph_templates.len(), 1);
    let template = &parsed.baseline_graph_templates[0];
    assert_eq!(template.baseline_name, "http://example.org/dayBaseline");
    assert_eq!(template.triples.len(), 2);
    assert_eq!(
        template.triples[0].predicate,
        GraphTermTemplate::Iri("http://example.org/dayAvgValue".to_string())
    );
    assert_eq!(
        template.triples[1].predicate,
        GraphTermTemplate::Iri("http://example.org/dayCount".to_string())
    );
}
