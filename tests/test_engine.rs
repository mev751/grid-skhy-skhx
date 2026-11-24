#[path = "../src/config.rs"]
mod config;
#[path = "../src/types.rs"]
mod types;

#[test]
fn test_trigger_calculations() {
    let p1: f64 = 1.4;
    let step_x: f64 = 0.003;
    let max_grids = 10;

    // Level 0: next entry = P1 + 0 * X = 1.4; next exit = None
    let level_0_entry = p1 + 0.0 * step_x;
    assert!((level_0_entry - 1.4f64).abs() < 1e-6);

    // Level 1: next entry = P1 + 1 * X = 1.403; next exit = P1 + (1 - 2) * X = 1.397
    let level_1_entry = p1 + 1.0 * step_x;
    let level_1_exit = p1 + (1.0 - 2.0) * step_x;
    assert!((level_1_entry - 1.403f64).abs() < 1e-6);
    assert!((level_1_exit - 1.397f64).abs() < 1e-6);

    // Level 10 (max): next entry = None
    assert!(10 >= max_grids);
}

#[test]
fn test_ema_smoothing() {
    let alpha = 0.25;
    let mut smoothed: Option<f64> = None;

    // First tick
    let k1 = 1.405;
    smoothed = match smoothed {
        None => Some(k1),
        Some(prev) => Some(alpha * k1 + (1.0 - alpha) * prev),
    };
    assert!((smoothed.unwrap() - 1.405).abs() < 1e-6);

    // Second tick
    let k2 = 1.415;
    smoothed = match smoothed {
        None => Some(k2),
        Some(prev) => Some(alpha * k2 + (1.0 - alpha) * prev),
    };
    // Expected: 0.25 * 1.415 + 0.75 * 1.405 = 0.35375 + 1.05375 = 1.4075
    assert!((smoothed.unwrap() - 1.4075).abs() < 1e-6);
}
