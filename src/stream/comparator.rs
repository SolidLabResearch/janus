use std::collections::VecDeque;
use std::fmt;

/// Represents a single data point in the stream.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DataPoint {
    pub timestamp: f64,
    pub value: f64,
}

/// Statistical metrics calculated from a window of data points.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowStats {
    pub mean: f64,
    pub std_dev: f64,
    pub slope: f64,
    pub count: usize,
}

impl WindowStats {
    /// Calculates statistics from a slice of DataPoints.
    /// Returns None if the window is empty.
    pub fn from_window(window: &VecDeque<DataPoint>) -> Option<Self> {
        if window.is_empty() {
            return None;
        }

        let n = window.len() as f64;
        let count = window.len();

        // 1. Mean
        let sum_val: f64 = window.iter().map(|dp| dp.value).sum();
        let mean = sum_val / n;

        // 2. Standard Deviation (Population)
        // \sigma = \sqrt{ \frac{\sum (x - \mu)^2}{N} }
        let variance_sum: f64 = window.iter().map(|dp| (dp.value - mean).powi(2)).sum();
        let std_dev = (variance_sum / n).sqrt();

        // 3. Slope (Linear Regression - Least Squares)
        // m = \frac{N \sum(xy) - \sum x \sum y}{N \sum(x^2) - (\sum x)^2}
        let sum_x: f64 = window.iter().map(|dp| dp.timestamp).sum();
        let sum_xy: f64 = window.iter().map(|dp| dp.timestamp * dp.value).sum();
        let sum_x2: f64 = window.iter().map(|dp| dp.timestamp.powi(2)).sum();

        let denominator = n * sum_x2 - sum_x.powi(2);

        let slope = if denominator.abs() < f64::EPSILON {
            0.0 // Avoid division by zero if all timestamps are identical or N=1
        } else {
            (n * sum_xy - sum_x * sum_val) / denominator
        };

        Some(WindowStats { mean, std_dev, slope, count })
    }
}

/// Configuration thresholds for the comparator.
#[derive(Debug, Clone)]
pub struct ComparatorConfig {
    /// Threshold for absolute difference: |live - hist| > threshold
    pub abs_threshold: f64,
    /// Threshold for relative drop: (live - hist) / hist > threshold
    pub rel_threshold: f64,
    /// Threshold for catch-up: hist - live > threshold
    pub catchup_trigger: f64,
    /// Minimum magnitude of slope to consider for trend divergence
    pub slope_epsilon: f64,
    /// Buffer for volatility check: live_sigma > hist_sigma + buffer
    pub volatility_buffer: f64,
    /// Size of the sliding window to maintain for statistics
    pub window_size: usize,
    /// Z-score threshold for outlier detection: |z| > threshold
    pub outlier_z_threshold: f64,
}

impl Default for ComparatorConfig {
    fn default() -> Self {
        Self {
            abs_threshold: 1.0,
            rel_threshold: 0.1,
            catchup_trigger: 2.0,
            slope_epsilon: 0.01,
            volatility_buffer: 0.5,
            window_size: 10,
            outlier_z_threshold: 3.0,
        }
    }
}

/// Result of a comparison between live and historical windows.
#[derive(Debug, Clone, PartialEq)]
pub enum ComparisonResult {
    /// Triggered when |live.mean - hist.mean| > abs_threshold
    AbsoluteThresholdExceeded { diff: f64 },
    /// Triggered when (live.mean - hist.mean) / hist.mean > rel_threshold
    RelativeDropDetected { rel_change: f64 },
    /// Triggered when hist.mean - live.mean > catchup_trigger
    CatchUpTriggered { lag: f64 },
    /// Triggered when slopes have opposite signs and magnitudes > slope_epsilon
    TrendDivergence { live_slope: f64, hist_slope: f64 },
    /// Triggered when live.std_dev > hist.std_dev + volatility_buffer
    VolatilityIncrease { live_sigma: f64, hist_sigma: f64 },
    /// Triggered when the latest live value is an outlier compared to historical distribution
    LiveOutlierDetected { value: f64, z_score: f64 },
}

impl fmt::Display for ComparisonResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ComparisonResult::AbsoluteThresholdExceeded { diff } => {
                write!(f, "Absolute Threshold Exceeded (diff: {:.4})", diff)
            }
            ComparisonResult::RelativeDropDetected { rel_change } => {
                write!(f, "Relative Drop Detected (change: {:.2}%)", rel_change * 100.0)
            }
            ComparisonResult::CatchUpTriggered { lag } => {
                write!(f, "Catch-Up Triggered (lag: {:.4})", lag)
            }
            ComparisonResult::TrendDivergence { live_slope, hist_slope } => {
                write!(f, "Trend Divergence (live: {:.4}, hist: {:.4})", live_slope, hist_slope)
            }
            ComparisonResult::VolatilityIncrease { live_sigma, hist_sigma } => {
                write!(f, "Volatility Increase (live: {:.4}, hist: {:.4})", live_sigma, hist_sigma)
            }
            ComparisonResult::LiveOutlierDetected { value, z_score } => {
                write!(f, "Live Outlier Detected (value: {:.4}, z-score: {:.2})", value, z_score)
            }
        }
    }
}

/// A stateful comparator that maintains a history of aggregated values to compute trends.
pub struct StatefulComparator {
    config: ComparatorConfig,
    live_history: VecDeque<DataPoint>,
    hist_history: VecDeque<DataPoint>,
}

impl StatefulComparator {
    /// Creates a new StatefulComparator with the given configuration.
    pub fn new(config: ComparatorConfig) -> Self {
        Self { config, live_history: VecDeque::new(), hist_history: VecDeque::new() }
    }

    /// Updates the comparator with new aggregated values for live and historical streams,
    /// and returns any triggered anomalies based on the updated statistics.
    pub fn update_and_compare(
        &mut self,
        timestamp: f64,
        live_val: f64,
        hist_val: f64,
    ) -> Vec<ComparisonResult> {
        // 1. Update History
        Self::add_point(&mut self.live_history, timestamp, live_val, self.config.window_size);
        Self::add_point(&mut self.hist_history, timestamp, hist_val, self.config.window_size);

        // 2. Calculate Statistics from History
        let live_stats = WindowStats::from_window(&self.live_history);
        let hist_stats = WindowStats::from_window(&self.hist_history);

        if let (Some(live), Some(hist)) = (live_stats, hist_stats) {
            self.compare_stats(&live, &hist)
        } else {
            Vec::new() // Not enough data yet
        }
    }

    // Static helper to avoid double borrow issues
    fn add_point(history: &mut VecDeque<DataPoint>, timestamp: f64, value: f64, max_size: usize) {
        if history.len() >= max_size {
            history.pop_front();
        }
        history.push_back(DataPoint { timestamp, value });
    }

    fn compare_stats(&self, live: &WindowStats, hist: &WindowStats) -> Vec<ComparisonResult> {
        let mut results = Vec::new();

        // 1. Absolute Threshold
        let abs_diff = (live.mean - hist.mean).abs();
        if abs_diff > self.config.abs_threshold {
            results.push(ComparisonResult::AbsoluteThresholdExceeded { diff: abs_diff });
        }

        // 2. Relative Drop
        if hist.mean.abs() > f64::EPSILON {
            let rel_change = (live.mean - hist.mean) / hist.mean;
            if rel_change > self.config.rel_threshold {
                results.push(ComparisonResult::RelativeDropDetected { rel_change });
            }
        }

        // 3. Catch-Up Trigger
        let lag = hist.mean - live.mean;
        if lag > self.config.catchup_trigger {
            results.push(ComparisonResult::CatchUpTriggered { lag });
        }

        // 4. Trend Divergence
        if (live.slope * hist.slope < 0.0)
            && (live.slope.abs() > self.config.slope_epsilon)
            && (hist.slope.abs() > self.config.slope_epsilon)
        {
            results.push(ComparisonResult::TrendDivergence {
                live_slope: live.slope,
                hist_slope: hist.slope,
            });
        }

        // 5. Volatility
        if live.std_dev > hist.std_dev + self.config.volatility_buffer {
            results.push(ComparisonResult::VolatilityIncrease {
                live_sigma: live.std_dev,
                hist_sigma: hist.std_dev,
            });
        }

        // 6. Outlier Detection
        // Check if the latest live value is an outlier compared to historical distribution
        if let Some(latest_live) = self.live_history.back() {
            if hist.std_dev > f64::EPSILON {
                let z_score = (latest_live.value - hist.mean) / hist.std_dev;
                if z_score.abs() > self.config.outlier_z_threshold {
                    results.push(ComparisonResult::LiveOutlierDetected {
                        value: latest_live.value,
                        z_score,
                    });
                }
            }
        }

        results
    }
}
