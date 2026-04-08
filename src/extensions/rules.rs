//! Rule abstractions used by Janus boolean extension functions.

use crate::extensions::math::{abs_diff, relative_change, zscore};

/// Error returned when a rule receives the wrong number of arguments.
#[derive(Debug, Clone, PartialEq)]
pub enum ExtensionRuleError {
    /// Caller supplied the wrong number of arguments.
    WrongArgCount {
        /// Number the rule expects.
        expected: usize,
        /// Number the caller supplied.
        got: usize,
    },
}

impl std::fmt::Display for ExtensionRuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtensionRuleError::WrongArgCount { expected, got } => {
                write!(f, "wrong argument count: expected {expected}, got {got}")
            }
        }
    }
}

impl std::error::Error for ExtensionRuleError {}

/// Trait implemented by Janus boolean extension functions.
pub trait ExtensionRule: Send + Sync {
    /// Evaluate the rule against a flat argument list.
    fn evaluate(&self, args: &[f64]) -> Result<bool, ExtensionRuleError>;
}

#[inline]
fn check_args(got: usize, expected: usize) -> Result<(), ExtensionRuleError> {
    if got != expected {
        Err(ExtensionRuleError::WrongArgCount { expected, got })
    } else {
        Ok(())
    }
}

/// `absolute_threshold_exceeded(live, hist, threshold)` -> `|live - hist| > threshold`
pub struct AbsoluteThreshold;

impl ExtensionRule for AbsoluteThreshold {
    fn evaluate(&self, args: &[f64]) -> Result<bool, ExtensionRuleError> {
        check_args(args.len(), 3)?;
        Ok(abs_diff(args[0], args[1]) > args[2])
    }
}

/// `relative_threshold_exceeded(live, hist, threshold)` -> `(live - hist) / hist > threshold`
pub struct RelativeThreshold;

impl ExtensionRule for RelativeThreshold {
    fn evaluate(&self, args: &[f64]) -> Result<bool, ExtensionRuleError> {
        check_args(args.len(), 3)?;
        Ok(relative_change(args[0], args[1]) > args[2])
    }
}

/// `catch_up(hist, live, threshold)` -> `(hist - live) > threshold`
pub struct CatchUp;

impl ExtensionRule for CatchUp {
    fn evaluate(&self, args: &[f64]) -> Result<bool, ExtensionRuleError> {
        check_args(args.len(), 3)?;
        Ok((args[0] - args[1]) > args[2])
    }
}

/// `volatility_increase(live_sigma, hist_sigma, buffer)` -> `live_sigma > hist_sigma + buffer`
pub struct VolatilityIncrease;

impl ExtensionRule for VolatilityIncrease {
    fn evaluate(&self, args: &[f64]) -> Result<bool, ExtensionRuleError> {
        check_args(args.len(), 3)?;
        Ok(args[0] > args[1] + args[2])
    }
}

/// `is_outlier(value, mean, sigma, z_threshold)` -> `|zscore(value, mean, sigma)| > z_threshold`
pub struct IsOutlier;

impl ExtensionRule for IsOutlier {
    fn evaluate(&self, args: &[f64]) -> Result<bool, ExtensionRuleError> {
        check_args(args.len(), 4)?;
        Ok(zscore(args[0], args[1], args[2]).abs() > args[3])
    }
}

/// `trend_divergent(live_slope, hist_slope, epsilon)` -> `|live_slope - hist_slope| > epsilon`
pub struct TrendDivergent;

impl ExtensionRule for TrendDivergent {
    fn evaluate(&self, args: &[f64]) -> Result<bool, ExtensionRuleError> {
        check_args(args.len(), 3)?;
        Ok((args[0] - args[1]).abs() > args[2])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_threshold_true() {
        let rule = AbsoluteThreshold;
        assert!(rule.evaluate(&[3.0, 1.0, 1.5]).unwrap());
    }

    #[test]
    fn relative_threshold_false() {
        let rule = RelativeThreshold;
        assert!(!rule.evaluate(&[1.05, 1.0, 0.5]).unwrap());
    }

    #[test]
    fn catch_up_true() {
        let rule = CatchUp;
        assert!(rule.evaluate(&[5.0, 1.0, 3.0]).unwrap());
    }

    #[test]
    fn volatility_increase_false() {
        let rule = VolatilityIncrease;
        assert!(!rule.evaluate(&[1.0, 0.5, 1.0]).unwrap());
    }

    #[test]
    fn is_outlier_true() {
        let rule = IsOutlier;
        assert!(rule.evaluate(&[10.0, 3.0, 2.0, 3.0]).unwrap());
    }

    #[test]
    fn trend_divergent_wrong_args() {
        let rule = TrendDivergent;
        assert!(rule.evaluate(&[0.1, 0.2]).is_err());
    }
}
