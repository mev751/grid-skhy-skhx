use std::env;
use std::fs;
use std::path::Path;

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

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            secret_key: String::new(),
            wallet_addr: String::new(),
            q_usdc: 1000.0,
            split_n: 1,
            max_grids: 10,
            p1: 1.4,
            step_x: 0.003,
            slice_delay_ms: 750,
            leverage: 10,
            ema_alpha: 0.25,
            maker_timeout_ms: 2000,
            maker_max_retries: 2,
            ws_skhy: "xyz:SKHY".to_string(),
            ws_skhx: "xyz:SKHX".to_string(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let mut cfg = Self::default();
        let path = Path::new("config.toml");

        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                cfg.parse_toml_content(&content);
            }
        } else {
            // Auto-create default template if missing
            let _ = Self::save_default_template(path);
        }

        // Apply environment variables (highest priority for secrets & CLI overrides)
        cfg.apply_env();

        cfg
    }

    pub fn save_default_template(path: &Path) -> std::io::Result<()> {
        let template = r#"# ==========================================
# ⚙️ Hyperliquid 做空网格策略配置文件 (config.toml)
# ==========================================

# 1. 钱包与账户配置
# 若使用 API 钱包 (Agent Key)，实际资产在主钱包中。
# 必须在此处填写拥有资金的主钱包地址（0x开头），以便正确读取保证金余额与链上仓位对账！
wallet_addr = ""

# 2. 核心策略网格配置
max_grids = 10         # 多少个格子 (最大允许建仓的网格层数)
q_usdc = 1000.0        # 单次网格目标仓位额度 (单位: USDC)
p1 = 1.4               # 基准起线 (做空建仓触发基准线)
step_x = 0.003         # 网格步长 (每层网格间距)
split_n = 1            # 单次拆分笔数 (1 表示不拆分)
slice_delay_ms = 750   # 切片拆分间隔 (毫秒)
leverage = 10          # 策略杠杆倍数

# 3. 算法与滤波配置
ema_alpha = 0.25       # K值平滑指数 (0.1 ~ 1.0, 越小越平滑)

# 4. Maker 挂单与重试配置
maker_timeout_ms = 2000 # 每次 Maker 挂单等待超时 (毫秒)
maker_max_retries = 2  # Maker 最大重试次数 (达到后自动转 Taker 兜底扫单)

# 5. 交易代币标识 (HIP-3 DEX 命名空间)
ws_skhy = "xyz:SKHY"
ws_skhx = "xyz:SKHX"

# 注意：钱包私钥 HL_SECRET_KEY 请通过系统环境变量传入，不在本文件中明文保存！
"#;
        fs::write(path, template)
    }

    pub fn parse_toml_content(&mut self, content: &str) {
        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Strip inline comment
            let code_part = if let Some((code, _)) = line.split_once('#') {
                code.trim()
            } else {
                line
            };

            if let Some((key, val)) = code_part.split_once('=') {
                let k = key.trim().to_lowercase();
                let v = val.trim().trim_matches('"').trim_matches('\'');

                match k.as_str() {
                    "max_grids" => {
                        if let Ok(n) = v.parse::<usize>() {
                            self.max_grids = n;
                        }
                    }
                    "q_usdc" => {
                        if let Ok(n) = v.parse::<f64>() {
                            self.q_usdc = n;
                        }
                    }
                    "p1" => {
                        if let Ok(n) = v.parse::<f64>() {
                            self.p1 = n;
                        }
                    }
                    "step_x" => {
                        if let Ok(n) = v.parse::<f64>() {
                            self.step_x = n;
                        }
                    }
                    "split_n" => {
                        if let Ok(n) = v.parse::<usize>() {
                            self.split_n = n;
                        }
                    }
                    "slice_delay_ms" => {
                        if let Ok(n) = v.parse::<u64>() {
                            self.slice_delay_ms = n;
                        }
                    }
                    "leverage" => {
                        if let Ok(n) = v.parse::<u32>() {
                            self.leverage = n;
                        }
                    }
                    "ema_alpha" => {
                        if let Ok(n) = v.parse::<f64>() {
                            self.ema_alpha = n;
                        }
                    }
                    "maker_timeout_ms" => {
                        if let Ok(n) = v.parse::<u64>() {
                            self.maker_timeout_ms = n;
                        }
                    }
                    "maker_max_retries" => {
                        if let Ok(n) = v.parse::<usize>() {
                            self.maker_max_retries = n;
                        }
                    }
                    "ws_skhy" => self.ws_skhy = v.to_string(),
                    "ws_skhx" => self.ws_skhx = v.to_string(),
                    "wallet_addr" => self.wallet_addr = v.to_string(),
                    _ => {}
                }
            }
        }
    }

    pub fn apply_env(&mut self) {
        if let Ok(key) = env::var("HL_SECRET_KEY") {
            let clean = key.trim().trim_start_matches("0x").to_string();
            if !clean.is_empty() {
                self.secret_key = clean;
            }
        }
        if let Ok(addr) = env::var("HL_WALLET_ADDR") {
            let clean = addr.trim().to_string();
            if !clean.is_empty() {
                self.wallet_addr = clean;
            }
        }
        if let Some(v) = env::var("Q_USDC").ok().and_then(|s| s.parse::<f64>().ok()) {
            self.q_usdc = v;
        }
        if let Some(v) = env::var("MAX_GRIDS").ok().and_then(|s| s.parse::<usize>().ok()) {
            self.max_grids = v;
        }
        if let Some(v) = env::var("P1").ok().and_then(|s| s.parse::<f64>().ok()) {
            self.p1 = v;
        }
        if let Some(v) = env::var("STEP_X").ok().and_then(|s| s.parse::<f64>().ok()) {
            self.step_x = v;
        }
        if let Some(v) = env::var("SPLIT_N").ok().and_then(|s| s.parse::<usize>().ok()) {
            self.split_n = v;
        }
    }

    pub fn is_live_trading(&self) -> bool {
        !self.secret_key.is_empty()
    }
}
