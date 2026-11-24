use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub secret_key: String,
    pub wallet_addr: String,
    
    pub q_usdc: f64,
    pub split_n: usize,
    pub max_grids: usize,
    pub p1: f64,
    pub step_x: f64,
    pub slice_delay_ms: u64,
    pub leverage: u32,
    pub ema_alpha: f64,
    
    pub maker_timeout_ms: u64,
    pub maker_max_retries: usize,
    
    pub ws_skhy: String,
    pub ws_skhx: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let secret_key = env::var("HL_SECRET_KEY")
            .unwrap_or_default()
            .trim()
            .trim_start_matches("0x")
            .to_string();

        let wallet_addr = env::var("HL_WALLET_ADDR")
            .unwrap_or_default()
            .trim()
            .to_string();

        let q_usdc = env::var("Q_USDC")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(1000.0);

        let split_n = env::var("SPLIT_N")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(1);

        let max_grids = env::var("MAX_GRIDS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(10);

        let p1 = env::var("P1")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(1.4);

        let step_x = env::var("STEP_X")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.003);

        let slice_delay_ms = env::var("SLICE_DELAY_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(750);

        let leverage = env::var("LEVERAGE")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(10);

        let ema_alpha = env::var("EMA_ALPHA")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.25);

        let maker_timeout_ms = env::var("MAKER_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(2000);

        let maker_max_retries = env::var("MAKER_MAX_RETRIES")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(2);

        let ws_skhy = env::var("WS_SKHY")
            .unwrap_or_else(|_| "xyz:SKHY".to_string());

        let ws_skhx = env::var("WS_SKHX")
            .unwrap_or_else(|_| "xyz:SKHX".to_string());

        Self {
            secret_key,
            wallet_addr,
            q_usdc,
            split_n,
            max_grids,
            p1,
            step_x,
            slice_delay_ms,
            leverage,
            ema_alpha,
            maker_timeout_ms,
            maker_max_retries,
            ws_skhy,
            ws_skhx,
        }
    }

    pub fn is_live_trading(&self) -> bool {
        !self.secret_key.is_empty()
    }
}
