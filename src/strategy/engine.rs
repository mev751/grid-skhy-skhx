use super::order_executor::{OrderExecutor, TradeAction};
use crate::config::AppConfig;
use crate::hyperliquid::exchange::ExchangeClient;
use crate::hyperliquid::info::{AccountEquityState, AssetInfo, InfoClient};
use crate::hyperliquid::ws::MarketPrices;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

#[derive(Debug, Clone, Default)]
pub struct EngineStats {
    pub positions_held: usize,
    pub max_grids: usize,
    pub pos_skhy: f64,
    pub pos_skhx: f64,
    pub k_smoothed: Option<f64>,
    pub next_entry: Option<f64>,
    pub next_exit: Option<f64>,
    pub is_aligned: bool,
    pub is_trading: bool,
    pub entry_count: u32,
    pub tp_count: u32,
    pub true_profit: f64,
}

pub struct GridEngine {
    pub config: AppConfig,
    pub prices: Arc<RwLock<MarketPrices>>,
    pub executor: Option<OrderExecutor>,
    pub info_client: Arc<InfoClient>,
    pub exchange_client: Option<Arc<ExchangeClient>>,
    
    pub stats: Arc<RwLock<EngineStats>>,
    pub logs: Arc<RwLock<VecDeque<String>>>,
    pub is_trading_flag: Arc<AtomicBool>,
    
    // level -> (k1, fee1)
    layer_entry_data: Arc<RwLock<HashMap<usize, (f64, f64)>>>,
}

impl GridEngine {
    pub fn new(
        config: AppConfig,
        prices: Arc<RwLock<MarketPrices>>,
        info_client: Arc<InfoClient>,
        exchange_client: Option<Arc<ExchangeClient>>,
        asset_map: HashMap<String, AssetInfo>,
    ) -> Self {
        let max_grids = config.max_grids;
        let executor = exchange_client.as_ref().map(|ex| {
            OrderExecutor::new(
                ex.clone(),
                prices.clone(),
                asset_map,
                &config.ws_skhy,
                &config.ws_skhx,
                config.maker_timeout_ms,
                config.maker_max_retries,
            )
        });

        let mut initial_stats = EngineStats::default();
        initial_stats.max_grids = max_grids;

        Self {
            config,
            prices,
            executor,
            info_client,
            exchange_client,
            stats: Arc::new(RwLock::new(initial_stats)),
            logs: Arc::new(RwLock::new(VecDeque::with_capacity(25))),
            is_trading_flag: Arc::new(AtomicBool::new(false)),
            layer_entry_data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn add_log(&self, msg: String) {
        let mut logs = self.logs.write().await;
        logs.push_back(msg);
        if logs.len() > 20 {
            logs.pop_front();
        }
    }

    pub async fn calculate_triggers(&self) -> (Option<f64>, Option<f64>) {
        let st = self.stats.read().await;
        let next_entry = if st.positions_held < st.max_grids {
            Some(self.config.p1 + (st.positions_held as f64) * self.config.step_x)
        } else {
            None
        };

        let next_exit = if st.positions_held > 0 {
            Some(self.config.p1 + ((st.positions_held as f64) - 2.0) * self.config.step_x)
        } else {
            None
        };

        (next_entry, next_exit)
    }

    pub async fn on_tick(&self, k: f64, p_skhy: f64, p_skhx: f64, equity_state: &AccountEquityState) {
        // 1. Update smoothed k
        {
            let mut st = self.stats.write().await;
            st.k_smoothed = match st.k_smoothed {
                None => Some(k),
                Some(prev) => Some(self.config.ema_alpha * k + (1.0 - self.config.ema_alpha) * prev),
            };
            let (entry_t, _exit_t) = if st.positions_held < st.max_grids {
                (Some(self.config.p1 + (st.positions_held as f64) * self.config.step_x), None::<f64>)
            } else {
                (None::<f64>, None::<f64>)
            };
            st.next_entry = entry_t;
            st.next_exit = if st.positions_held > 0 {
                Some(self.config.p1 + ((st.positions_held as f64) - 2.0) * self.config.step_x)
            } else {
                None
            };
        }

        let st_snapshot = self.stats.read().await.clone();
        if !st_snapshot.is_aligned {
            return;
        }

        if self.is_trading_flag.load(Ordering::SeqCst) {
            return;
        }

        let k_use = match st_snapshot.k_smoothed {
            Some(val) => val,
            None => return,
        };

        // 🟢 Check ENTRY trigger
        if st_snapshot.positions_held < st_snapshot.max_grids {
            let next_entry_k = self.config.p1 + (st_snapshot.positions_held as f64) * self.config.step_x;
            if k_use >= next_entry_k {
                // Check margin: only evaluate if on-chain equity data has been synchronized
                let required_margin = (self.config.q_usdc * 2.0) / (self.config.leverage as f64);
                let has_synced_equity = equity_state.spot_usdc > 0.0
                    || equity_state.account_value > 0.0
                    || equity_state.withdrawable_margin > 0.0;

                if self.config.is_live_trading() && has_synced_equity {
                    if equity_state.withdrawable_margin < required_margin {
                        let mut st = self.stats.write().await;
                        st.max_grids = st.positions_held;
                        self.add_log(format!(
                            "[⚠️ 保证金不足] 可用 {:.2} < 所需 {:.2}，锁定 [第 {} 层] 为上限！",
                            equity_state.withdrawable_margin, required_margin, st.positions_held
                        ))
                        .await;
                        return;
                    }
                } else if self.config.is_live_trading() && !has_synced_equity {
                    // Waiting for initial equity data to be retrieved, do not lock max_grids to 0!
                    return;
                }

                // Calculate sizes
                let sz_hy = (self.config.q_usdc / p_skhy).max(0.001);
                let sz_hx = (self.config.q_usdc / p_skhx).max(0.001);

                {
                    let mut st = self.stats.write().await;
                    st.pos_skhy += sz_hy;
                    st.pos_skhx += sz_hx;
                    st.positions_held += 1;
                }

                self.execute_trade_flow(TradeAction::Entry, st_snapshot.positions_held + 1, sz_hy, sz_hx, k_use, false).await;
                return;
            }
        }

        // 🔴 Check EXIT trigger
        if st_snapshot.positions_held > 0 {
            let next_exit_k = self.config.p1 + ((st_snapshot.positions_held as f64) - 2.0) * self.config.step_x;
            if k_use <= next_exit_k {
                let (sz_hy_close, sz_hx_close) = if st_snapshot.positions_held == 1 {
                    (st_snapshot.pos_skhy, st_snapshot.pos_skhx)
                } else {
                    (
                        st_snapshot.pos_skhy / (st_snapshot.positions_held as f64),
                        st_snapshot.pos_skhx / (st_snapshot.positions_held as f64),
                    )
                };

                let is_last_close = st_snapshot.positions_held == 1;

                {
                    let mut st = self.stats.write().await;
                    st.pos_skhy = (st.pos_skhy - sz_hy_close).max(0.0);
                    st.pos_skhx = (st.pos_skhx - sz_hx_close).max(0.0);
                    st.positions_held -= 1;
                }

                self.execute_trade_flow(
                    TradeAction::Exit,
                    st_snapshot.positions_held,
                    sz_hy_close,
                    sz_hx_close,
                    k_use,
                    is_last_close,
                )
                .await;
            }
        }
    }

    async fn execute_trade_flow(
        &self,
        action: TradeAction,
        level: usize,
        total_sz_hy: f64,
        total_sz_hx: f64,
        k_val: f64,
        is_last_close: bool,
    ) {
        self.is_trading_flag.store(true, Ordering::SeqCst);
        {
            let mut st = self.stats.write().await;
            st.is_trading = true;
        }

        let now_str = chrono::Local::now().format("%H:%M:%S").to_string();
        let split_n = self.config.split_n.max(1);
        let slice_hy = total_sz_hy / (split_n as f64);
        let slice_hx = total_sz_hx / (split_n as f64);

        if let Some(ref exec) = self.executor {
            for i in 0..split_n {
                let is_final = i == split_n - 1;
                let actual_hy = if is_final { total_sz_hy - slice_hy * (i as f64) } else { slice_hy };
                let actual_hx = if is_final { total_sz_hx - slice_hx * (i as f64) } else { slice_hx };

                match exec.execute_slice(action, actual_hy, actual_hx, is_last_close && is_final).await {
                    Ok(msg) => {
                        self.add_log(format!("[{}] 切片 {}/{} 成功: {}", now_str, i + 1, split_n, msg)).await;
                    }
                    Err(e) => {
                        self.add_log(format!("[{}] 切片 {}/{} 异常: {}", now_str, i + 1, split_n, e)).await;
                    }
                }

                if i < split_n - 1 {
                    sleep(Duration::from_millis(self.config.slice_delay_ms)).await;
                }
            }
        }

        // Post-trade accounting
        {
            let mut st = self.stats.write().await;
            match action {
                TradeAction::Entry => {
                    st.entry_count += 1;
                    let mut layers = self.layer_entry_data.write().await;
                    layers.insert(level, (k_val, 0.0));
                    self.add_log(format!(
                        "[{}] 第{}格建仓成功! 理论建仓 k={:.4} (已建仓{}次)",
                        now_str, level, k_val, st.entry_count
                    ))
                    .await;
                }
                TradeAction::Exit => {
                    st.tp_count += 1;
                    let mut layers = self.layer_entry_data.write().await;
                    let default_theoretical_k1 = self.config.p1 + ((level as f64) - 1.0) * self.config.step_x;
                    let (k1, fee1) = layers.remove(&level).unwrap_or((default_theoretical_k1, 0.0));
                    let k2 = k_val;
                    let single_profit = if k2 > 0.0 {
                        (self.config.q_usdc * (k1 - k2) / k2) - fee1
                    } else {
                        0.0
                    };
                    st.true_profit += single_profit;

                    let last_tag = if is_last_close { " <全平>" } else { "" };
                    self.add_log(format!(
                        "[{}] 第{}格止盈成功![{:.4}->{:.4}] 真实收益: {:+.2} (已止盈{}次){}",
                        now_str, level, k1, k2, single_profit, st.tp_count, last_tag
                    ))
                    .await;
                }
            }
            st.is_trading = false;
        }

        self.is_trading_flag.store(false, Ordering::SeqCst);
    }
}
