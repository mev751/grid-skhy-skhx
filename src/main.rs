pub mod config;
pub mod hyperliquid;
pub mod strategy;
pub mod types;
pub mod ui;

use config::AppConfig;
use hyperliquid::exchange::ExchangeClient;
use hyperliquid::info::{AccountEquityState, InfoClient, TradeHistoryStats};
use hyperliquid::ws::WsManager;
use strategy::engine::GridEngine;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{sleep, Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::load();

    println!("正在唤醒单向做空网格对冲结算系统 (Rust v3.8)...");

    let info_client = Arc::new(InfoClient::new());

    let (exchange_client, wallet_addr) = if config.is_live_trading() {
        let ex = ExchangeClient::new(&config.secret_key)
            .map_err(|e| format!("初始化钱包与签名器失败: {}", e))?;
        let addr = if !config.wallet_addr.is_empty() {
            config.wallet_addr.clone()
        } else {
            ex.wallet_address().to_string()
        };
        (Some(Arc::new(ex)), addr)
    } else {
        (None, config.wallet_addr.clone())
    };

    // 1. Resolve assets for builder DEX "xyz"
    println!("正在解析 Hyperliquid HIP-3 perpDexs 资产映射与精度...");
    let asset_map = info_client
        .resolve_assets(&[&config.ws_skhy, &config.ws_skhx])
        .await
        .unwrap_or_default();

    // 2. Fetch initial prices
    let ws_manager = Arc::new(WsManager::new(&config.ws_skhy, &config.ws_skhx));
    if let Ok((bid_y, ask_y)) = info_client.fetch_l2_book(&config.ws_skhy).await {
        let mut p = ws_manager.prices.write().await;
        p.skhy_bid = bid_y;
        p.skhy_ask = ask_y;
    }
    if let Ok((bid_x, ask_x)) = info_client.fetch_l2_book(&config.ws_skhx).await {
        let mut p = ws_manager.prices.write().await;
        p.skhx_bid = bid_x;
        p.skhx_ask = ask_x;
    }

    // 3. Setup WebSocket tick channel
    let (tick_tx, mut tick_rx) = mpsc::unbounded_channel::<()>();
    ws_manager.start(Some(tick_tx));

    // 4. Initialize Grid Engine
    let engine = Arc::new(GridEngine::new(
        config.clone(),
        ws_manager.prices.clone(),
        info_client.clone(),
        exchange_client.clone(),
        asset_map,
    ));

    // Mark as aligned in simulation or live trading startup
    {
        let mut st = engine.stats.write().await;
        st.is_aligned = true;
    }

    // 5. Shared account state & stats
    let equity_state = Arc::new(RwLock::new(AccountEquityState::default()));
    let trade_stats = Arc::new(RwLock::new(TradeHistoryStats::default()));
    let bot_start_time_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    // 5.1 Synchronously fetch initial account equity before starting tickers!
    if !wallet_addr.is_empty() {
        println!("正在同步主钱包账户资产与可用保证金: {}...", wallet_addr);
        if let Ok(st) = info_client.fetch_account_state(&wallet_addr).await {
            println!(
                "账户资产同步成功: 总净值={:.2}, 可用保证金={:.2}, Spot USDC={:.2}",
                st.account_value, st.withdrawable_margin, st.spot_usdc
            );
            let mut eq = equity_state.write().await;
            *eq = st;
        }
    } else {
        println!("[⚠️ 警告] 未配置主钱包地址 wallet_addr！若使用 API 钱包，请在 config.toml 或环境变量 HL_WALLET_ADDR 中配置主钱包地址！");
    }

    // 6. Background account equity tracking
    if !wallet_addr.is_empty() {
        let info_clone = info_client.clone();
        let eq_clone = equity_state.clone();
        let w_addr = wallet_addr.clone();
        tokio::spawn(async move {
            loop {
                if let Ok(st) = info_clone.fetch_account_state(&w_addr).await {
                    let mut eq = eq_clone.write().await;
                    *eq = st;
                }
                sleep(Duration::from_secs(3)).await;
            }
        });

        let info_clone2 = info_client.clone();
        let tr_clone = trade_stats.clone();
        let w_addr2 = wallet_addr.clone();
        tokio::spawn(async move {
            let mut processed = HashSet::new();
            loop {
                if let Ok(st) = info_clone2
                    .fetch_trade_history(&w_addr2, bot_start_time_ms, &mut processed)
                    .await
                {
                    let mut tr = tr_clone.write().await;
                    *tr = st;
                }
                sleep(Duration::from_secs(5)).await;
            }
        });
    }

    // 7. Tick Processing Task
    let engine_tick = engine.clone();
    let prices_tick = ws_manager.prices.clone();
    let equity_tick = equity_state.clone();
    tokio::spawn(async move {
        while let Some(_) = tick_rx.recv().await {
            let (p_hy_bid, p_hx_ask) = {
                let p = prices_tick.read().await;
                (p.skhy_bid, p.skhx_ask)
            };

            if let (Some(hy_bid), Some(hx_ask)) = (p_hy_bid, p_hx_ask) {
                if hx_ask > 0.0 {
                    let k = hy_bid * 10.0 / hx_ask;
                    let eq = equity_tick.read().await.clone();
                    engine_tick.on_tick(k, hy_bid, hx_ask, &eq).await;
                }
            }
        }
    });

    // 8. Main UI loop
    ui::init_terminal();
    let start_instant = Instant::now();
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                ui::restore_terminal();
                println!("已安全退出自动交易守护进程。");
                break;
            }
            _ = sleep(Duration::from_millis(300)) => {
                let p = ws_manager.prices.read().await.clone();
                let eq = equity_state.read().await.clone();
                let hist = trade_stats.read().await.clone();
                let st = engine.stats.read().await.clone();
                let logs = {
                    let l = engine.logs.read().await;
                    l.iter().cloned().collect::<Vec<_>>()
                };

                let running_sec = start_instant.elapsed().as_secs();
                ui::render_dashboard(&config, &p, &eq, &hist, &st, &logs, running_sec);
            }
        }
    }

    ui::restore_terminal();
    ws_manager.stop();
    Ok(())
}
