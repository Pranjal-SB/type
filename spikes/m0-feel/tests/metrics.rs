use std::time::Duration;

use m0_feel::metrics::FrameTimer;

#[test]
fn p99_reflects_the_slow_tail() {
    let mut t = FrameTimer::new();
    for _ in 0..99 {
        t.record(Duration::from_micros(100));
    }
    t.record(Duration::from_micros(50_000));
    let report = t.report();
    assert!(report.contains("n=100"), "report was: {report}");
    assert!(report.contains("max=50000us"), "report was: {report}");
}

#[test]
fn empty_timer_reports_without_panicking() {
    assert!(FrameTimer::new().report().contains("n=0"));
}
