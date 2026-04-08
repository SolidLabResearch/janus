//! Registry of Janus extension-function names.

use std::collections::HashMap;
use std::sync::Arc;

use crate::extensions::rules::{
    AbsoluteThreshold, CatchUp, ExtensionRule, IsOutlier, RelativeThreshold, TrendDivergent,
    VolatilityIncrease,
};

/// Namespace shared by all Janus extension functions.
pub const JANUS_NS: &str = "https://janus.rs/fn#";

/// Fully qualified URIs for registered scalar and boolean functions.
pub const FN_ABS_DIFF: &str = "https://janus.rs/fn#abs_diff";
pub const FN_RELATIVE_CHANGE: &str = "https://janus.rs/fn#relative_change";
pub const FN_ZSCORE: &str = "https://janus.rs/fn#zscore";
pub const FN_ABSOLUTE_THRESHOLD: &str = "https://janus.rs/fn#absolute_threshold_exceeded";
pub const FN_RELATIVE_THRESHOLD: &str = "https://janus.rs/fn#relative_threshold_exceeded";
pub const FN_CATCH_UP: &str = "https://janus.rs/fn#catch_up";
pub const FN_VOLATILITY_INCREASE: &str = "https://janus.rs/fn#volatility_increase";
pub const FN_IS_OUTLIER: &str = "https://janus.rs/fn#is_outlier";
pub const FN_TREND_DIVERGENT: &str = "https://janus.rs/fn#trend_divergent";

/// Registry that maps Janus function URIs to boolean rule implementations.
pub struct FunctionRegistry {
    rules: HashMap<&'static str, Arc<dyn ExtensionRule>>,
}

impl FunctionRegistry {
    /// Build the default registry with all boolean rules pre-populated.
    pub fn new() -> Self {
        let mut rules: HashMap<&'static str, Arc<dyn ExtensionRule>> = HashMap::new();
        rules.insert(FN_ABSOLUTE_THRESHOLD, Arc::new(AbsoluteThreshold));
        rules.insert(FN_RELATIVE_THRESHOLD, Arc::new(RelativeThreshold));
        rules.insert(FN_CATCH_UP, Arc::new(CatchUp));
        rules.insert(FN_VOLATILITY_INCREASE, Arc::new(VolatilityIncrease));
        rules.insert(FN_IS_OUTLIER, Arc::new(IsOutlier));
        rules.insert(FN_TREND_DIVERGENT, Arc::new(TrendDivergent));
        Self { rules }
    }

    /// Look up a boolean rule by its fully-qualified URI.
    pub fn lookup(&self, name: &str) -> Option<&dyn ExtensionRule> {
        self.rules.get(name).map(Arc::as_ref)
    }

    /// Iterate over all registry entries.
    pub fn all_rules(&self) -> impl Iterator<Item = (&'static str, Arc<dyn ExtensionRule>)> + '_ {
        self.rules.iter().map(|(key, value)| (*key, Arc::clone(value)))
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_known_rule_returns_some() {
        let registry = FunctionRegistry::new();
        assert!(registry.lookup(FN_ABSOLUTE_THRESHOLD).is_some());
    }

    #[test]
    fn lookup_unknown_name_returns_none() {
        let registry = FunctionRegistry::new();
        assert!(registry.lookup("https://janus.rs/fn#nonexistent").is_none());
    }
}
