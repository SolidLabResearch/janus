//! Oxigraph evaluator builder preloaded with Janus extension functions.

use oxigraph::model::{Literal, NamedNode, Term};
use oxigraph::sparql::SparqlEvaluator;

use crate::extensions::math::{abs_diff, relative_change, zscore};
use crate::extensions::registry::{
    FunctionRegistry, FN_ABSOLUTE_THRESHOLD, FN_ABS_DIFF, FN_CATCH_UP, FN_IS_OUTLIER,
    FN_RELATIVE_CHANGE, FN_RELATIVE_THRESHOLD, FN_TREND_DIVERGENT, FN_VOLATILITY_INCREASE,
    FN_ZSCORE,
};

fn term_to_f64(term: &Term) -> Option<f64> {
    if let Term::Literal(literal) = term {
        literal.value().parse::<f64>().ok()
    } else {
        None
    }
}

fn bool_term(value: bool) -> Term {
    Term::Literal(Literal::new_typed_literal(
        if value { "true" } else { "false" },
        NamedNode::new("http://www.w3.org/2001/XMLSchema#boolean").unwrap(),
    ))
}

fn decimal_term(value: f64) -> Term {
    Term::Literal(Literal::new_typed_literal(
        &value.to_string(),
        NamedNode::new("http://www.w3.org/2001/XMLSchema#decimal").unwrap(),
    ))
}

fn args_to_floats(args: &[Term]) -> Option<Vec<f64>> {
    args.iter().map(term_to_f64).collect()
}

/// Build a `SparqlEvaluator` with Janus extension functions registered.
pub fn build_evaluator() -> SparqlEvaluator {
    let registry = FunctionRegistry::new();
    let mut evaluator = SparqlEvaluator::new();

    for (uri, rule) in registry.all_rules() {
        let name = NamedNode::new(uri).expect("constant URI must be valid");
        evaluator = evaluator.with_custom_function(name, move |args| {
            let floats = args_to_floats(args)?;
            match rule.evaluate(&floats) {
                Ok(result) => Some(bool_term(result)),
                Err(_) => None,
            }
        });
    }

    evaluator = evaluator.with_custom_function(NamedNode::new(FN_ABS_DIFF).unwrap(), |args| {
        if args.len() != 2 {
            return None;
        }
        let left = term_to_f64(&args[0])?;
        let right = term_to_f64(&args[1])?;
        Some(decimal_term(abs_diff(left, right)))
    });

    evaluator =
        evaluator.with_custom_function(NamedNode::new(FN_RELATIVE_CHANGE).unwrap(), |args| {
            if args.len() != 2 {
                return None;
            }
            let left = term_to_f64(&args[0])?;
            let right = term_to_f64(&args[1])?;
            let result = relative_change(left, right);
            if result.is_finite() {
                Some(decimal_term(result))
            } else {
                None
            }
        });

    evaluator = evaluator.with_custom_function(NamedNode::new(FN_ZSCORE).unwrap(), |args| {
        if args.len() != 3 {
            return None;
        }
        let value = term_to_f64(&args[0])?;
        let mean = term_to_f64(&args[1])?;
        let sigma = term_to_f64(&args[2])?;
        Some(decimal_term(zscore(value, mean, sigma)))
    });

    evaluator
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::model::{GraphName, Quad};
    use oxigraph::sparql::QueryResults;
    use oxigraph::store::Store;

    fn named(uri: &str) -> NamedNode {
        NamedNode::new(uri).unwrap()
    }

    fn lit(value: &str) -> Term {
        Term::Literal(Literal::new_simple_literal(value))
    }

    fn insert(store: &Store, subject: &str, predicate: &str, object: &str) {
        store
            .insert(&Quad::new(
                named(subject),
                named(predicate),
                lit(object),
                GraphName::DefaultGraph,
            ))
            .unwrap();
    }

    #[test]
    fn absolute_threshold_filter_works_in_real_oxigraph_query() {
        let store = Store::new().unwrap();
        insert(&store, "https://janus.rs/test#sensor1", "https://janus.rs/test#live", "2.5");
        insert(&store, "https://janus.rs/test#sensor1", "https://janus.rs/test#hist", "1.0");
        insert(&store, "https://janus.rs/test#sensor2", "https://janus.rs/test#live", "1.1");
        insert(&store, "https://janus.rs/test#sensor2", "https://janus.rs/test#hist", "1.0");

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
        let parsed_query = evaluator.parse_query(query).unwrap();
        let results = parsed_query.on_store(&store).execute().unwrap();

        if let QueryResults::Solutions(solutions) = results {
            let rows: Vec<_> = solutions.collect::<Result<Vec<_>, _>>().unwrap();
            assert_eq!(rows.len(), 1);
            let sensor = rows[0].get("sensor").unwrap();
            assert_eq!(sensor.to_string(), "<https://janus.rs/test#sensor1>");
        } else {
            panic!("expected SELECT solutions");
        }
    }
}
