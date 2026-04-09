//! JanusQL Parser Integration Tests
//!
//! Tests for the JanusQL query parser, verifying parsing of window definitions,
//! R2S operators, and query generation.

use janus::parsing::janusql_parser::{
    BaselineBootstrapMode, JanusQLParser, SourceKind, WindowSpec,
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
            WINDOW :temperatureWindow {
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
        FROM NAMED WINDOW sensor:histWindow ON STREAM sensor:temperatureStream [START 1622505600 END 1622592000]
        FROM NAMED WINDOW sensor:histSlideWindow ON STREAM sensor:temperatureStream [OFFSET 1622505600 RANGE 10000 STEP 2000]
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
    assert_eq!(result.historical_windows[1].offset, Some(1_622_505_600));
    assert_eq!(result.historical_windows[1].width, 10000);
    assert_eq!(result.historical_windows[1].slide, 2000);
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
        FROM NAMED WINDOW sensor:histSlideWindow ON LOG sensor:historicalStore [OFFSET 500 RANGE 1000 STEP 100]
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
