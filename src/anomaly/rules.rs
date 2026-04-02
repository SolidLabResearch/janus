//! Anomaly detection rule trait and implementations.
//!
//! Each rule receives its numeric parameters as a `&[f64]` slice and returns
//! a boolean decision.  Argument ordering follows the SPARQL call convention
//! documented in [`crate::anomaly::registry`].

use crate::anomaly::math::{abs_diff, relative_change, zscore};

/// Error returned when a rule receives the wrong number of arguments.
#[derive(Debug, Clone, PartialEq)]
pub enum AnomalyRuleError {
    /// Caller supplied the wrong number of arguments.
    WrongArgCount {
        /// Number the rule expects.
        expected: usize,
        /// Number the caller supplied.
        got: usize,
    },
}

impl std::fmt::Display for AnomalyRuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnomalyRuleError::WrongArgCount { expected, got } => {
                write!(f, "wrong argument count: expected {expected}, got {got}")
            }
        }
    }
}

impl std::error::Error for AnomalyRuleError {}

/// A named, stateless anomaly detection rule.
///
/// Implementations must be `Send + Sync` so they can be stored in the shared
/// registry and captured in Oxigraph custom-function closures.
pub trait AnomalyRule: Send + Sync {
    /// Short human-readable name (without namespace prefix).
    fn name(&self) -> &'static str;

    /// Evaluate the rule against a flat argument list.
    ///
    /// Returns `Ok(true)` when the anomaly condition is met, `Ok(false)`
    /// otherwise, or an error if `args` has the wrong length.
    fn evaluate(&self, args: &[f64]) -> Result<bool, AnomalyRuleError>;
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

#[inline]
fn check_args(got: usize, expected: usize) -> Result<(), AnomalyRuleError> {
    if got != expected {
        Err(AnomalyRuleError::WrongArgCount { expected, got })
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Rule implementations
// ---------------------------------------------------------------------------

/// `absolute_threshold_exceeded(live, hist, threshold)` →
/// `|live − hist| > threshold`
pub struct AbsoluteThreshold;

impl AnomalyRule for AbsoluteThreshold {
    fn name(&self) -> &'static str { "absolute_threshold_exceeded" }

    fn evaluate(&self, args: &[f64]) -> Result<bool, AnomalyRuleError> {
        check_args(args.len(), 3)?;
        Ok(abs_diff(args[0], args[1]) > args[2])
    }
}

/// `relative_threshold_exceeded(live, hist, threshold)` →
/// `(live − hist) / hist > threshold`
pub struct RelativeThreshold;

impl AnomalyRule for RelativeThreshold {
    fn name(&self) -> &'static str { "relative_threshold_exceeded" }

    fn evaluate(&self, args: &[f64]) -> Result<bool, AnomalyRuleError> {
        check_args(args.len(), 3)?;
        Ok(relative_change(args[0], args[1]) > args[2])
    }
}

/// `catch_up(hist, live, threshold)` → `(hist − live) > threshold`
pub struct CatchUp;

impl AnomalyRule for CatchUp {
    fn name(&self) -> &'static str { "catch_up" }

    fn evaluate(&self, args: &[f64]) -> Result<bool, AnomalyRuleError> {
        check_args(args.len(), 3)?;
        Ok((args[0] - args[1]) > args[2])
    }
}

/// `volatility_increase(live_sigma, hist_sigma, buffer)` →
/// `live_sigma > hist_sigma + buffer`
pub struct VolatilityIncrease;

impl AnomalyRule for VolatilityIncrease {
    fn name(&self) -> &'static str { "volatility_increase" }

    fn evaluate(&self, args: &[f64]) -> Result<bool, AnomalyRuleError> {
        check_args(args.len(), 3)?;
        Ok(args[0] > args[1] + args[2])
    }
}

/// `is_outlier(value, mean, sigma, z_threshold)` →
/// `|zscore(value, mean, sigma)| > z_threshold`
pub struct IsOutlier;

impl AnomalyRule for IsOutlier {
    fn name(&self) -> &'static str { "is_outlier" }

    fn evaluate(&self, args: &[f64]) -> Result<bool, AnomalyRuleError> {
        check_args(args.len(), 4)?;
        Ok(zscore(args[0], args[1], args[2]).abs() > args[3])
    }
}

/// `trend_divergent(live_slope, hist_slope, epsilon)` →
/// `|live_slope − hist_slope| > epsilon`
pub struct TrendDivergent;

impl AnomalyRule for TrendDivergent {
    fn name(&self) -> &'static str { "trend_divergent" }

    fn evaluate(&self, args: &[f64]) -> Result<bool, AnomalyRuleError> {
        check_args(args.len(), 3)?;
        Ok((args[0] - args[1]).abs() > args[2])
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- AbsoluteThreshold ---

    #[test]
    fn absolute_threshold_true() {
        let r = AbsoluteThreshold;
        assert_eq!(r.evaluate(&[3.0, 1.0, 1.5]).unwrap(), true);
    }

    #[test]
    fn absolute_threshold_false() {
        let r = AbsoluteThreshold;
        assert_eq!(r.evaluate(&[1.1, 1.0, 0.5]).unwrap(), false);
    }

    #[test]
    fn absolute_threshold_wrong_args() {
        let r = AbsoluteThreshold;
        assert!(r.evaluate(&[1.0, 2.0]).is_err());
    }

    // --- RelativeThreshold ---

    #[test]
    fn relative_threshold_true() {
        // (2.0 - 1.0) / 1.0 = 1.0 > 0.5
        let r = RelativeThreshold;
        assert_eq!(r.evaluate(&[2.0, 1.0, 0.5]).unwrap(), true);
    }

    #[test]
    fn relative_threshold_false() {
        // (1.05 - 1.0) / 1.0 = 0.05 < 0.5
        let r = RelativeThreshold;
        assert_eq!(r.evaluate(&[1.05, 1.0, 0.5]).unwrap(), false);
    }

    #[test]
    fn relative_threshold_wrong_args() {
        let r = RelativeThreshold;
        assert!(r.evaluate(&[1.0]).is_err());
    }

    // --- CatchUp ---

    #[test]
    fn catch_up_true() {
        // hist=5.0, live=1.0, threshold=3.0 → 4.0 > 3.0
        let r = CatchUp;
        assert_eq!(r.evaluate(&[5.0, 1.0, 3.0]).unwrap(), true);
    }

    #[test]
    fn catch_up_false() {
        // hist=1.5, live=1.0, threshold=3.0 → 0.5 < 3.0
        let r = CatchUp;
        assert_eq!(r.evaluate(&[1.5, 1.0, 3.0]).unwrap(), false);
    }

    #[test]
    fn catch_up_wrong_args() {
        let r = CatchUp;
        assert!(r.evaluate(&[]).is_err());
    }

    // --- VolatilityIncrease ---

    #[test]
    fn volatility_increase_true() {
        // live_sigma=2.0 > hist_sigma=0.5 + buffer=1.0
        let r = VolatilityIncrease;
        assert_eq!(r.evaluate(&[2.0, 0.5, 1.0]).unwrap(), true);
    }

    #[test]
    fn volatility_increase_false() {
        let r = VolatilityIncrease;
        assert_eq!(r.evaluate(&[1.0, 0.5, 1.0]).unwrap(), false);
    }

    #[test]
    fn volatility_increase_wrong_args() {
        let r = VolatilityIncrease;
        assert!(r.evaluate(&[1.0, 2.0, 3.0, 4.0]).is_err());
    }

    // --- IsOutlier ---

    #[test]
    fn is_outlier_true() {
        // zscore(10.0, 3.0, 2.0) = 3.5 > 3.0
        let r = IsOutlier;
        assert_eq!(r.evaluate(&[10.0, 3.0, 2.0, 3.0]).unwrap(), true);
    }

    #[test]
    fn is_outlier_false() {
        // zscore(4.0, 3.0, 2.0) = 0.5 < 3.0
        let r = IsOutlier;
        assert_eq!(r.evaluate(&[4.0, 3.0, 2.0, 3.0]).unwrap(), false);
    }

    #[test]
    fn is_outlier_zero_sigma() {
        // sigma=0 → zscore=0.0 → not an outlier for any positive threshold
        let r = IsOutlier;
        assert_eq!(r.evaluate(&[99.0, 1.0, 0.0, 3.0]).unwrap(), false);
    }

    // --- TrendDivergent ---

    #[test]
    fn trend_divergent_true() {
        // |0.1 - (-0.1)| = 0.2 > 0.05
        let r = TrendDivergent;
        assert_eq!(r.evaluate(&[0.1, -0.1, 0.05]).unwrap(), true);
    }

    #[test]
    fn trend_divergent_false() {
        // |0.1 - 0.09| = 0.01 < 0.05
        let r = TrendDivergent;
        assert_eq!(r.evaluate(&[0.1, 0.09, 0.05]).unwrap(), false);
    }

    #[test]
    fn trend_divergent_wrong_args() {
        let r = TrendDivergent;
        assert!(r.evaluate(&[0.1, 0.2]).is_err());
    }
}
