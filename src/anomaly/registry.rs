//! Function name registry mapping `https://janus.rs/fn#` URIs to rule objects.
//!
//! The registry is the single source of truth for which boolean rules exist.
//! Scalar functions (`abs_diff`, `relative_change`, `zscore`) are pure helpers
//! and do not implement `AnomalyRule`; they are registered directly in
//! [`crate::anomaly::query_options`].

use std::collections::HashMap;
use std::sync::Arc;

use crate::anomaly::rules::{
    AbsoluteThreshold, AnomalyRule, CatchUp, IsOutlier, RelativeThreshold, TrendDivergent,
    VolatilityIncrease,
};

/// Namespace shared by all Janus extension functions.
pub const JANUS_NS: &str = "https://janus.rs/fn#";

/// Fully-qualified URIs for every registered function.
pub const FN_ABS_DIFF: &str = "https://janus.rs/fn#abs_diff";
pub const FN_RELATIVE_CHANGE: &str = "https://janus.rs/fn#relative_change";
pub const FN_ZSCORE: &str = "https://janus.rs/fn#zscore";
pub const FN_ABSOLUTE_THRESHOLD: &str = "https://janus.rs/fn#absolute_threshold_exceeded";
pub const FN_RELATIVE_THRESHOLD: &str = "https://janus.rs/fn#relative_threshold_exceeded";
pub const FN_CATCH_UP: &str = "https://janus.rs/fn#catch_up";
pub const FN_VOLATILITY_INCREASE: &str = "https://janus.rs/fn#volatility_increase";
pub const FN_IS_OUTLIER: &str = "https://janus.rs/fn#is_outlier";
pub const FN_TREND_DIVERGENT: &str = "https://janus.rs/fn#trend_divergent";

/// Registry that maps function URIs to their boolean [`AnomalyRule`] implementations.
///
/// Scalar functions are not included here; see [`crate::anomaly::query_options`].
pub struct FunctionRegistry {
    rules: HashMap<&'static str, Arc<dyn AnomalyRule>>,
}

impl FunctionRegistry {
    /// Build the default registry with all six boolean rules pre-populated.
    pub fn new() -> Self {
        let mut rules: HashMap<&'static str, Arc<dyn AnomalyRule>> = HashMap::new();
        rules.insert(FN_ABSOLUTE_THRESHOLD, Arc::new(AbsoluteThreshold));
        rules.insert(FN_RELATIVE_THRESHOLD, Arc::new(RelativeThreshold));
        rules.insert(FN_CATCH_UP, Arc::new(CatchUp));
        rules.insert(FN_VOLATILITY_INCREASE, Arc::new(VolatilityIncrease));
        rules.insert(FN_IS_OUTLIER, Arc::new(IsOutlier));
        rules.insert(FN_TREND_DIVERGENT, Arc::new(TrendDivergent));
        Self { rules }
    }

    /// Look up a boolean rule by its fully-qualified URI.
    ///
    /// Returns `None` if the name is unknown or is a scalar function.
    pub fn lookup(&self, name: &str) -> Option<&dyn AnomalyRule> {
        self.rules.get(name).map(Arc::as_ref)
    }

    /// Iterate over all `(uri, Arc<dyn AnomalyRule>)` entries.
    ///
    /// Primarily used by [`crate::anomaly::query_options`] when wiring closures
    /// into the Oxigraph `SparqlEvaluator`.
    pub fn all_rules(&self) -> impl Iterator<Item = (&'static str, Arc<dyn AnomalyRule>)> + '_ {
        self.rules.iter().map(|(k, v)| (*k, Arc::clone(v)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_known_rule_returns_some() {
        let reg = FunctionRegistry::new();
        assert!(reg.lookup(FN_ABSOLUTE_THRESHOLD).is_some());
        assert!(reg.lookup(FN_TREND_DIVERGENT).is_some());
    }

    #[test]
    fn lookup_unknown_name_returns_none() {
        let reg = FunctionRegistry::new();
        assert!(reg.lookup("https://janus.rs/fn#nonexistent").is_none());
    }

    #[test]
    fn lookup_scalar_uri_returns_none() {
        // Scalar functions are not in the boolean registry
        let reg = FunctionRegistry::new();
        assert!(reg.lookup(FN_ABS_DIFF).is_none());
    }

    #[test]
    fn all_six_boolean_rules_present() {
        let reg = FunctionRegistry::new();
        let names: Vec<&str> = reg.all_rules().map(|(k, _)| k).collect();
        assert!(names.contains(&FN_ABSOLUTE_THRESHOLD));
        assert!(names.contains(&FN_RELATIVE_THRESHOLD));
        assert!(names.contains(&FN_CATCH_UP));
        assert!(names.contains(&FN_VOLATILITY_INCREASE));
        assert!(names.contains(&FN_IS_OUTLIER));
        assert!(names.contains(&FN_TREND_DIVERGENT));
    }

    #[test]
    fn evaluate_via_registry_lookup() {
        let reg = FunctionRegistry::new();
        let rule = reg.lookup(FN_ABSOLUTE_THRESHOLD).unwrap();
        assert_eq!(rule.evaluate(&[3.0, 1.0, 1.5]).unwrap(), true);
        assert_eq!(rule.evaluate(&[1.0, 1.0, 0.1]).unwrap(), false);
    }
}
