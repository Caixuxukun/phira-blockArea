use prpr::judge::{timing_error, LIMIT_GOOD, LIMIT_PERFECT};

#[test]
fn late_taps_no_longer_receive_seventy_milliseconds_of_perfect_grace() {
    for error in [0.081, 0.10, 0.149, 0.15] {
        assert!(timing_error(0., error, 1.) > LIMIT_PERFECT);
        assert!(timing_error(0., -error, 1.) > LIMIT_PERFECT);
    }
    assert!(timing_error(0., 0.079, 1.) < LIMIT_PERFECT);
    assert!(timing_error(0., -0.079, 1.) < LIMIT_PERFECT);
}

#[test]
fn ordinary_good_window_reaches_180ms_and_scales_with_playback() {
    assert!(timing_error(0., 0.179, 1.) < LIMIT_GOOD);
    assert!(timing_error(0., 0.181, 1.) > LIMIT_GOOD);
    assert!(timing_error(0., -0.179, 1.) < LIMIT_GOOD);
    // The same 100ms real-time delay at 2x is a 200ms chart-time delay.
    assert_eq!(timing_error(0., 0.2, 2.), timing_error(0., 0.1, 1.));
    assert!(timing_error(0., 0.2, 2.) > LIMIT_PERFECT);
}
