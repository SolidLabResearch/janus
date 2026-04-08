//! Pure numeric helpers used by Janus extension functions.

/// Absolute difference between live and historical values: `|live - hist|`.
pub fn abs_diff(live: f64, hist: f64) -> f64 {
    (live - hist).abs()
}

/// Relative change from historical to live: `(live - hist) / hist`.
///
/// Returns `f64::NAN` when `hist` is zero.
pub fn relative_change(live: f64, hist: f64) -> f64 {
    (live - hist) / hist
}

/// Z-score of `value` given a distribution with `mean` and `sigma`.
///
/// Returns `0.0` when `sigma` is zero.
pub fn zscore(value: f64, mean: f64, sigma: f64) -> f64 {
    if sigma.abs() < f64::EPSILON {
        0.0
    } else {
        (value - mean) / sigma
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abs_diff_positive_delta() {
        assert!((abs_diff(3.0, 1.0) - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn abs_diff_negative_delta() {
        assert!((abs_diff(1.0, 3.0) - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn relative_change_increase() {
        let rc = relative_change(1.1, 1.0);
        assert!((rc - 0.1).abs() < 1e-10);
    }

    #[test]
    fn relative_change_zero_hist_is_nan_or_infinite() {
        let rc = relative_change(1.0, 0.0);
        assert!(rc.is_nan() || rc.is_infinite());
    }

    #[test]
    fn zscore_zero_sigma_returns_zero() {
        assert!(zscore(99.0, 1.0, 0.0).abs() < f64::EPSILON);
    }
}
