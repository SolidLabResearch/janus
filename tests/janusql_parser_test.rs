//! JanusQL Parser Integration Tests
//!
//! Tests for the JanusQL query parser, verifying parsing of window definitions,
//! R2S operators, and query generation.

use janus::parsing::janusql_parser::JanusQLParser;

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
