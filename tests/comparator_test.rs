use janus::stream::comparator::{ComparatorConfig, DataPoint, StatefulComparator, WindowStats};
use std::collections::VecDeque;

#[test]
fn test_window_stats_calculation() {
    // y = 2x + 1
    let mut data = VecDeque::new();
    data.push_back(DataPoint { timestamp: 0.0, value: 1.0 });
    data.push_back(DataPoint { timestamp: 1.0, value: 3.0 });
    data.push_back(DataPoint { timestamp: 2.0, value: 5.0 });

    let stats = WindowStats::from_window(&data).unwrap();

    assert_eq!(stats.mean, 3.0);
    assert_eq!(stats.count, 3);
    assert!((stats.slope - 2.0).abs() < 1e-9);
    // Population std dev of 1, 3, 5:
    // Mean = 3. Variance = ((1-3)^2 + (3-3)^2 + (5-3)^2) / 3 = (4 + 0 + 4) / 3 = 8/3 = 2.666...
    // Std Dev = sqrt(2.666...) = 1.63299...
    assert!((stats.std_dev - (8.0f64 / 3.0).sqrt()).abs() < 1e-9);
}

#[test]
fn test_stateful_comparator_triggers() {
    let config = ComparatorConfig {
        abs_threshold: 10.0,
        rel_threshold: 0.5,
        catchup_trigger: 5.0,
        slope_epsilon: 0.1,
        volatility_buffer: 1.0,
        window_size: 5,
        outlier_z_threshold: 2.0, // Lower threshold for testing
    };

    let mut comparator = StatefulComparator::new(config);

    // Feed data to simulate divergence
    // Hist: Stable increasing (100, 101, 102)
    // Live: Dropping (90, 89, 88)

    // T=0
    let res0 = comparator.update_and_compare(0.0, 90.0, 100.0);
    // Not enough data for slope, but absolute threshold might trigger if we allowed 1-point stats.
    // But WindowStats works with 1 point (slope=0, std_dev=0).
    // Abs diff = 10. Threshold = 10. 10 > 10 is False.
    // Catchup = 10. Trigger = 5. 10 > 5 is True.
    assert!(res0.iter().any(|r| matches!(
        r,
        janus::stream::comparator::ComparisonResult::CatchUpTriggered { .. }
    )));

    // T=1
    let res1 = comparator.update_and_compare(1.0, 89.0, 101.0);
    // Hist Slope: (101-100)/1 = 1.0
    // Live Slope: (89-90)/1 = -1.0
    // Divergence should trigger
    assert!(res1
        .iter()
        .any(|r| matches!(r, janus::stream::comparator::ComparisonResult::TrendDivergence { .. })));
}

#[test]
fn test_outlier_detection() {
    let config = ComparatorConfig {
        abs_threshold: 1000.0,     // High threshold to avoid triggering
        rel_threshold: 10.0,       // High threshold to avoid triggering
        catchup_trigger: 1000.0,   // High threshold to avoid triggering
        slope_epsilon: 10.0,       // High threshold to avoid triggering
        volatility_buffer: 1000.0, // High threshold to avoid triggering
        window_size: 10,
        outlier_z_threshold: 2.0,
    };

    let mut comparator = StatefulComparator::new(config);

    // Build stable historical baseline: values around 100.0
    for i in 0..10 {
        let hist_val = 100.0 + (i as f64 * 0.1); // 100.0, 100.1, 100.2, ...
        comparator.update_and_compare(i as f64, hist_val, hist_val);
    }

    // Now feed a normal live value (should not trigger outlier)
    let normal_res = comparator.update_and_compare(10.0, 100.5, 100.5);
    assert!(!normal_res.iter().any(|r| matches!(
        r,
        janus::stream::comparator::ComparisonResult::LiveOutlierDetected { .. }
    )));

    // Feed an outlier live value (way above historical mean)
    let outlier_res = comparator.update_and_compare(11.0, 150.0, 100.6); // 150.0 is ~5σ above mean
    assert!(outlier_res.iter().any(|r| matches!(
        r,
        janus::stream::comparator::ComparisonResult::LiveOutlierDetected { .. }
    )));

    // Check the z-score is reasonable
    if let Some(janus::stream::comparator::ComparisonResult::LiveOutlierDetected {
        value,
        z_score,
    }) = outlier_res.iter().find(|r| {
        matches!(r, janus::stream::comparator::ComparisonResult::LiveOutlierDetected { .. })
    }) {
        assert_eq!(*value, 150.0);
        assert!(z_score > &2.0); // Should be significantly above threshold
    }
}
