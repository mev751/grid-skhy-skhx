#[path = "../src/config.rs"]
mod config;
#[path = "../src/types.rs"]
mod types;

use config::AppConfig;

#[test]
fn test_default_config() {
    let cfg = AppConfig::from_env();
    assert_eq!(cfg.q_usdc, 1000.0);
    assert_eq!(cfg.split_n, 1);
    assert_eq!(cfg.max_grids, 10);
    assert_eq!(cfg.p1, 1.4);
    assert_eq!(cfg.step_x, 0.003);
    assert_eq!(cfg.ws_skhy, "xyz:SKHY");
    assert_eq!(cfg.ws_skhx, "xyz:SKHX");
}
