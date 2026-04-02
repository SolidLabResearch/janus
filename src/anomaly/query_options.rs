//! Oxigraph `SparqlEvaluator` builder that registers all Janus extension functions.
//!
//! # Discrepancy from task spec
//!
//! The task specifies `pub fn build_query_options() -> QueryOptions`.  Oxigraph 0.5
//! removed the `QueryOptions` struct entirely; custom functions are now registered
//! directly on `SparqlEvaluator` via `SparqlEvaluator::with_custom_function`.
//! This module therefore exposes `build_evaluator() -> SparqlEvaluator` and
//! re-exports it as `build_query_options` for API consistency.

use oxigraph::model::{Literal, NamedNode, Term};
use oxigraph::sparql::SparqlEvaluator;

use crate::anomaly::math::{abs_diff, relative_change, zscore};
use crate::anomaly::registry::{
    FunctionRegistry, FN_ABS_DIFF, FN_ABSOLUTE_THRESHOLD, FN_CATCH_UP, FN_IS_OUTLIER,
    FN_RELATIVE_CHANGE, FN_RELATIVE_THRESHOLD, FN_TREND_DIVERGENT, FN_VOLATILITY_INCREASE,
    FN_ZSCORE,
};

// ---------------------------------------------------------------------------
// Term helpers
// ---------------------------------------------------------------------------

fn term_to_f64(term: &Term) -> Option<f64> {
    if let Term::Literal(lit) = term {
        lit.value().parse::<f64>().ok()
    } else {
        None
    }
}

fn bool_term(b: bool) -> Term {
    Term::Literal(Literal::new_typed_literal(
        if b { "true" } else { "false" },
        NamedNode::new("http://www.w3.org/2001/XMLSchema#boolean").unwrap(),
    ))
}

fn decimal_term(v: f64) -> Term {
    Term::Literal(Literal::new_typed_literal(
        &v.to_string(),
        NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap(),
    ))
}

fn args_to_floats(args: &[Term]) -> Option<Vec<f64>> {
    args.iter().map(term_to_f64).collect()
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Build a `SparqlEvaluator` pre-loaded with all Janus extension functions.
///
/// Wire this evaluator in place of `SparqlEvaluator::new()` wherever SPARQL
/// queries are executed against the Oxigraph store in the historical path.
pub fn build_evaluator() -> SparqlEvaluator {
    let registry = FunctionRegistry::new();
    let mut evaluator = SparqlEvaluator::new();

    // Register the six boolean rules from the registry.
    for (uri, rule) in registry.all_rules() {
        let fn_name = NamedNode::new(uri).expect("constant URI must be valid");
        evaluator = evaluator.with_custom_function(fn_name, move |args| {
            let floats = args_to_floats(args)?;
            match rule.evaluate(&floats) {
                Ok(b) => Some(bool_term(b)),
                Err(_) => None,
            }
        });
    }

    // --- Scalar functions (not in AnomalyRule registry) ---

    evaluator = evaluator.with_custom_function(
        NamedNode::new(FN_ABS_DIFF).unwrap(),
        |args| {
            if args.len() != 2 {
                return None;
            }
            let a = term_to_f64(&args[0])?;
            let b = term_to_f64(&args[1])?;
            Some(decimal_term(abs_diff(a, b)))
        },
    );

    evaluator = evaluator.with_custom_function(
        NamedNode::new(FN_RELATIVE_CHANGE).unwrap(),
        |args| {
            if args.len() != 2 {
                return None;
            }
            let a = term_to_f64(&args[0])?;
            let b = term_to_f64(&args[1])?;
            let rc = relative_change(a, b);
            if rc.is_finite() { Some(decimal_term(rc)) } else { None }
        },
    );

    evaluator = evaluator.with_custom_function(
        NamedNode::new(FN_ZSCORE).unwrap(),
        |args| {
            if args.len() != 3 {
                return None;
            }
            let value = term_to_f64(&args[0])?;
            let mean = term_to_f64(&args[1])?;
            let sigma = term_to_f64(&args[2])?;
            Some(decimal_term(zscore(value, mean, sigma)))
        },
    );

    evaluator
}

/// Alias kept for API consistency with the task specification.
///
/// See [`build_evaluator`].
pub use build_evaluator as build_query_options;

// ---------------------------------------------------------------------------
// Integration tests — one per extension function through real Oxigraph eval
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::model::{GraphName, Quad};
    use oxigraph::sparql::QueryResults;
    use oxigraph::store::Store;

    fn xsd_decimal() -> NamedNode {
        NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap()
    }

    fn lit(v: &str) -> Term {
        Term::Literal(Literal::new_typed_literal(v, xsd_decimal()))
    }

    fn named(uri: &str) -> NamedNode {
        NamedNode::new(uri).unwrap()
    }

    fn insert(store: &Store, s: &str, p: &str, o: &str) {
        store
            .insert(&Quad::new(named(s), named(p), lit(o), GraphName::DefaultGraph))
            .unwrap();
    }

    /// Execute query, collect ?sensor bindings, return sorted URIs.
    fn sensor_uris(store: &Store, query: &str) -> Vec<String> {
        let evaluator = build_evaluator();
        let parsed = evaluator.parse_query(query).expect("query must parse");
        let results = parsed.on_store(store).execute().expect("query must execute");
        if let QueryResults::Solutions(solutions) = results {
            let mut uris: Vec<String> = solutions
                .filter_map(|s| s.ok())
                .filter_map(|s| s.get("sensor").map(|t| t.to_string()))
                .collect();
            uris.sort();
            uris
        } else {
            vec![]
        }
    }

    /// Execute a scalar BIND query and return the first value of ?result as f64.
    fn scalar_result(store: &Store, query: &str) -> f64 {
        let evaluator = build_evaluator();
        let parsed = evaluator.parse_query(query).expect("query must parse");
        let results = parsed.on_store(store).execute().expect("query must execute");
        if let QueryResults::Solutions(solutions) = results {
            solutions
                .filter_map(|s| s.ok())
                .filter_map(|s| {
                    s.get("result").and_then(|t| {
                        if let oxigraph::model::Term::Literal(l) = t {
                            l.value().parse::<f64>().ok()
                        } else {
                            None
                        }
                    })
                })
                .next()
                .expect("expected at least one result row")
        } else {
            panic!("expected SELECT solutions");
        }
    }

    // -----------------------------------------------------------------------
    // absolute_threshold_exceeded
    // -----------------------------------------------------------------------

    /// Step 5 integration test: FILTER with absolute_threshold_exceeded.
    #[test]
    fn integration_absolute_threshold_filter() {
        let store = Store::new().unwrap();
        let live = "https://janus.rs/test#live";
        let hist = "https://janus.rs/test#hist";

        // sensor1: |2.5 - 1.0| = 1.5 > 0.2  → should appear
        insert(&store, "https://janus.rs/test#sensor1", live, "2.5");
        insert(&store, "https://janus.rs/test#sensor1", hist, "1.0");
        // sensor2: |1.1 - 1.0| = 0.1 < 0.2  → should not appear
        insert(&store, "https://janus.rs/test#sensor2", live, "1.1");
        insert(&store, "https://janus.rs/test#sensor2", hist, "1.0");

        let query = r#"
            PREFIX janus: <https://janus.rs/fn#>
            PREFIX test:  <https://janus.rs/test#>
            SELECT ?sensor WHERE {
                ?sensor test:live ?live ;
                        test:hist ?hist .
                FILTER(janus:absolute_threshold_exceeded(?live, ?hist, 0.2))
            }
        "#;

        let evaluator = build_evaluator();
        let parsed = evaluator.parse_query(query).expect("query must parse");
        let results = parsed.on_store(&store).execute().expect("query must execute");

        if let QueryResults::Solutions(solutions) = results {
            let rows: Vec<_> =
                solutions.collect::<Result<Vec<_>, _>>().expect("solutions must be readable");
            assert_eq!(rows.len(), 1, "expected exactly one result (sensor1)");
            let sensor = rows[0].get("sensor").expect("binding 'sensor' must exist");
            assert_eq!(
                sensor.to_string(),
                "<https://janus.rs/test#sensor1>",
                "only sensor1 exceeds the threshold"
            );
        } else {
            panic!("expected SELECT solutions");
        }
    }

    // -----------------------------------------------------------------------
    // relative_threshold_exceeded
    // -----------------------------------------------------------------------

    /// sensor1: (2.0-1.0)/1.0 = 1.0 > 0.5 → appears
    /// sensor2: (1.05-1.0)/1.0 = 0.05 < 0.5 → filtered out
    #[test]
    fn integration_relative_threshold_filter() {
        let store = Store::new().unwrap();
        let live = "https://janus.rs/test#live";
        let hist = "https://janus.rs/test#hist";
        insert(&store, "https://janus.rs/test#sensor1", live, "2.0");
        insert(&store, "https://janus.rs/test#sensor1", hist, "1.0");
        insert(&store, "https://janus.rs/test#sensor2", live, "1.05");
        insert(&store, "https://janus.rs/test#sensor2", hist, "1.0");

        let query = r#"
            PREFIX janus: <https://janus.rs/fn#>
            PREFIX test:  <https://janus.rs/test#>
            SELECT ?sensor WHERE {
                ?sensor test:live ?live ; test:hist ?hist .
                FILTER(janus:relative_threshold_exceeded(?live, ?hist, 0.5))
            }
        "#;

        let uris = sensor_uris(&store, query);
        assert_eq!(uris, vec!["<https://janus.rs/test#sensor1>"]);
    }

    // -----------------------------------------------------------------------
    // catch_up
    // -----------------------------------------------------------------------

    /// sensor1: hist(5.0) - live(1.0) = 4.0 > 3.0 → appears
    /// sensor2: hist(1.5) - live(1.0) = 0.5 < 3.0 → filtered out
    #[test]
    fn integration_catch_up_filter() {
        let store = Store::new().unwrap();
        let live = "https://janus.rs/test#live";
        let hist = "https://janus.rs/test#hist";
        insert(&store, "https://janus.rs/test#sensor1", hist, "5.0");
        insert(&store, "https://janus.rs/test#sensor1", live, "1.0");
        insert(&store, "https://janus.rs/test#sensor2", hist, "1.5");
        insert(&store, "https://janus.rs/test#sensor2", live, "1.0");

        // catch_up(hist, live, threshold): args are (hist, live, threshold)
        let query = r#"
            PREFIX janus: <https://janus.rs/fn#>
            PREFIX test:  <https://janus.rs/test#>
            SELECT ?sensor WHERE {
                ?sensor test:hist ?hist ; test:live ?live .
                FILTER(janus:catch_up(?hist, ?live, 3.0))
            }
        "#;

        let uris = sensor_uris(&store, query);
        assert_eq!(uris, vec!["<https://janus.rs/test#sensor1>"]);
    }

    // -----------------------------------------------------------------------
    // volatility_increase
    // -----------------------------------------------------------------------

    /// sensor1: live_sigma(2.0) > hist_sigma(0.5) + buffer(1.0) → appears
    /// sensor2: live_sigma(1.0) ≤ hist_sigma(0.5) + buffer(1.0) → filtered out
    #[test]
    fn integration_volatility_increase_filter() {
        let store = Store::new().unwrap();
        let ls = "https://janus.rs/test#liveSigma";
        let hs = "https://janus.rs/test#histSigma";
        insert(&store, "https://janus.rs/test#sensor1", ls, "2.0");
        insert(&store, "https://janus.rs/test#sensor1", hs, "0.5");
        insert(&store, "https://janus.rs/test#sensor2", ls, "1.0");
        insert(&store, "https://janus.rs/test#sensor2", hs, "0.5");

        let query = r#"
            PREFIX janus: <https://janus.rs/fn#>
            PREFIX test:  <https://janus.rs/test#>
            SELECT ?sensor WHERE {
                ?sensor test:liveSigma ?ls ; test:histSigma ?hs .
                FILTER(janus:volatility_increase(?ls, ?hs, 1.0))
            }
        "#;

        let uris = sensor_uris(&store, query);
        assert_eq!(uris, vec!["<https://janus.rs/test#sensor1>"]);
    }

    // -----------------------------------------------------------------------
    // is_outlier
    // -----------------------------------------------------------------------

    /// zscore(10.0, 3.0, 2.0) = 3.5 > 3.0 → sensor1 appears
    /// zscore(4.0, 3.0, 2.0)  = 0.5 < 3.0 → sensor2 filtered out
    #[test]
    fn integration_is_outlier_filter() {
        let store = Store::new().unwrap();
        let val  = "https://janus.rs/test#value";
        let mean = "https://janus.rs/test#mean";
        let sig  = "https://janus.rs/test#sigma";
        insert(&store, "https://janus.rs/test#sensor1", val,  "10.0");
        insert(&store, "https://janus.rs/test#sensor1", mean, "3.0");
        insert(&store, "https://janus.rs/test#sensor1", sig,  "2.0");
        insert(&store, "https://janus.rs/test#sensor2", val,  "4.0");
        insert(&store, "https://janus.rs/test#sensor2", mean, "3.0");
        insert(&store, "https://janus.rs/test#sensor2", sig,  "2.0");

        let query = r#"
            PREFIX janus: <https://janus.rs/fn#>
            PREFIX test:  <https://janus.rs/test#>
            SELECT ?sensor WHERE {
                ?sensor test:value ?v ; test:mean ?m ; test:sigma ?s .
                FILTER(janus:is_outlier(?v, ?m, ?s, 3.0))
            }
        "#;

        let uris = sensor_uris(&store, query);
        assert_eq!(uris, vec!["<https://janus.rs/test#sensor1>"]);
    }

    // -----------------------------------------------------------------------
    // trend_divergent
    // -----------------------------------------------------------------------

    /// |0.1 - (-0.1)| = 0.2 > 0.05 → sensor1 appears
    /// |0.1 - 0.09|   = 0.01 < 0.05 → sensor2 filtered out
    #[test]
    fn integration_trend_divergent_filter() {
        let store = Store::new().unwrap();
        let ls = "https://janus.rs/test#liveSlope";
        let hs = "https://janus.rs/test#histSlope";
        insert(&store, "https://janus.rs/test#sensor1", ls, "0.1");
        insert(&store, "https://janus.rs/test#sensor1", hs, "-0.1");
        insert(&store, "https://janus.rs/test#sensor2", ls, "0.1");
        insert(&store, "https://janus.rs/test#sensor2", hs, "0.09");

        let query = r#"
            PREFIX janus: <https://janus.rs/fn#>
            PREFIX test:  <https://janus.rs/test#>
            SELECT ?sensor WHERE {
                ?sensor test:liveSlope ?ls ; test:histSlope ?hs .
                FILTER(janus:trend_divergent(?ls, ?hs, 0.05))
            }
        "#;

        let uris = sensor_uris(&store, query);
        assert_eq!(uris, vec!["<https://janus.rs/test#sensor1>"]);
    }

    // -----------------------------------------------------------------------
    // Scalar functions via BIND
    // -----------------------------------------------------------------------

    /// abs_diff(3.0, 1.0) = 2.0
    #[test]
    fn integration_scalar_abs_diff() {
        let store = Store::new().unwrap();
        insert(&store, "https://janus.rs/test#s1",
               "https://janus.rs/test#live", "3.0");
        insert(&store, "https://janus.rs/test#s1",
               "https://janus.rs/test#hist", "1.0");

        let query = r#"
            PREFIX janus: <https://janus.rs/fn#>
            PREFIX test:  <https://janus.rs/test#>
            SELECT ?result WHERE {
                ?s test:live ?live ; test:hist ?hist .
                BIND(janus:abs_diff(?live, ?hist) AS ?result)
            }
        "#;

        let v = scalar_result(&store, query);
        assert!((v - 2.0).abs() < 1e-9, "expected 2.0, got {v}");
    }

    /// zscore(5.0, 3.0, 2.0) = 1.0
    #[test]
    fn integration_scalar_zscore() {
        let store = Store::new().unwrap();
        insert(&store, "https://janus.rs/test#s1",
               "https://janus.rs/test#value", "5.0");
        insert(&store, "https://janus.rs/test#s1",
               "https://janus.rs/test#mean",  "3.0");
        insert(&store, "https://janus.rs/test#s1",
               "https://janus.rs/test#sigma", "2.0");

        let query = r#"
            PREFIX janus: <https://janus.rs/fn#>
            PREFIX test:  <https://janus.rs/test#>
            SELECT ?result WHERE {
                ?s test:value ?v ; test:mean ?m ; test:sigma ?sig .
                BIND(janus:zscore(?v, ?m, ?sig) AS ?result)
            }
        "#;

        let v = scalar_result(&store, query);
        assert!((v - 1.0).abs() < 1e-9, "expected 1.0, got {v}");
    }

    /// relative_change(1.1, 1.0) ≈ 0.1
    #[test]
    fn integration_scalar_relative_change() {
        let store = Store::new().unwrap();
        insert(&store, "https://janus.rs/test#s1",
               "https://janus.rs/test#live", "1.1");
        insert(&store, "https://janus.rs/test#s1",
               "https://janus.rs/test#hist", "1.0");

        let query = r#"
            PREFIX janus: <https://janus.rs/fn#>
            PREFIX test:  <https://janus.rs/test#>
            SELECT ?result WHERE {
                ?s test:live ?live ; test:hist ?hist .
                BIND(janus:relative_change(?live, ?hist) AS ?result)
            }
        "#;

        let v = scalar_result(&store, query);
        assert!((v - 0.1).abs() < 1e-9, "expected 0.1, got {v}");
    }

    // -----------------------------------------------------------------------
    // No false positives — nothing triggered when all values are normal
    // -----------------------------------------------------------------------

    #[test]
    fn integration_no_anomaly_when_values_identical() {
        let store = Store::new().unwrap();
        // live == hist for both sensors → no threshold should fire
        insert(&store, "https://janus.rs/test#s1",
               "https://janus.rs/test#live", "5.0");
        insert(&store, "https://janus.rs/test#s1",
               "https://janus.rs/test#hist", "5.0");
        insert(&store, "https://janus.rs/test#s2",
               "https://janus.rs/test#live", "5.0");
        insert(&store, "https://janus.rs/test#s2",
               "https://janus.rs/test#hist", "5.0");

        let query = r#"
            PREFIX janus: <https://janus.rs/fn#>
            PREFIX test:  <https://janus.rs/test#>
            SELECT ?sensor WHERE {
                ?sensor test:live ?live ; test:hist ?hist .
                FILTER(
                    janus:absolute_threshold_exceeded(?live, ?hist, 0.1) ||
                    janus:relative_threshold_exceeded(?live, ?hist, 0.1) ||
                    janus:catch_up(?hist, ?live, 0.1)
                )
            }
        "#;

        let uris = sensor_uris(&store, query);
        assert!(uris.is_empty(), "expected no anomalies when live == hist");
    }
}
