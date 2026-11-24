use crate::config::AppConfig;
use crate::hyperliquid::info::{AccountEquityState, TradeHistoryStats};
use crate::hyperliquid::ws::MarketPrices;
use crate::strategy::engine::EngineStats;

pub fn format_runtime_str(total_seconds: u64) -> String {
    let days = total_seconds / 86400;
    let remainder = total_seconds % 86400;
    let hours = remainder / 3600;
    let remainder = remainder % 3600;
    let minutes = remainder / 60;
    let secs = remainder % 60;
    format!("{}天 {:02}时 {:02}分 {:02}秒", days, hours, minutes, secs)
}

pub fn render_dashboard(
    cfg: &AppConfig,
    prices: &MarketPrices,
    equity: &AccountEquityState,
    history: &TradeHistoryStats,
    stats: &EngineStats,
    logs: &[String],
    running_seconds: u64,
) {
    let time_str = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let runtime_str = format_runtime_str(running_seconds);

    let mode_txt = if cfg.is_live_trading() {
        "[实盘交易开启 🟢]"
    } else {
        "[模拟运行模式 🟡]"
    };

    let align_txt = if stats.is_aligned {
        "对齐成功"
    } else {
        "对准中..."
    };

    let engine_txt = if stats.is_trading {
        "工作中 ⚙️"
    } else {
        "空闲中"
    };

    let ws_txt = if prices.is_connected {
        "[连通] ●"
    } else {
        "[重连中...]"
    };

    let p_hy_bid_str = prices
        .skhy_bid
        .map(|v| format!("{:.6}", v))
        .unwrap_or_else(|| "Wait...".to_string());
    let p_hy_ask_str = prices
        .skhy_ask
        .map(|v| format!("{:.6}", v))
        .unwrap_or_else(|| "Wait...".to_string());

    let p_hx_bid_str = prices
        .skhx_bid
        .map(|v| format!("{:.6}", v))
        .unwrap_or_else(|| "Wait...".to_string());
    let p_hx_ask_str = prices
        .skhx_ask
        .map(|v| format!("{:.6}", v))
        .unwrap_or_else(|| "Wait...".to_string());

    let raw_k = prices.current_ratio_k();
    let raw_k_str = raw_k
        .map(|v| format!("{:.6}", v))
        .unwrap_or_else(|| "Wait...".to_string());
    let ema_k_str = stats
        .k_smoothed
        .map(|v| format!("{:.6}", v))
        .unwrap_or_else(|| "Wait...".to_string());

    let next_entry_str = stats
        .next_entry
        .map(|v| format!("{:.5}", v))
        .unwrap_or_else(|| "锁定封顶".to_string());
    let next_exit_str = stats
        .next_exit
        .map(|v| format!("{:.5}", v))
        .unwrap_or_else(|| "空仓无".to_string());

    let val_prog_y = if let Some(p) = prices.skhy_bid {
        -stats.pos_skhy * p
    } else {
        0.0
    };
    let val_prog_x = if let Some(p) = prices.skhx_ask {
        stats.pos_skhx * p
    } else {
        0.0
    };

    let val_real_y = if let Some(p) = prices.skhy_bid {
        equity.real_pos_skhy * p
    } else {
        0.0
    };
    let val_real_x = if let Some(p) = prices.skhx_ask {
        equity.real_pos_skhx * p
    } else {
        0.0
    };

    let total_net_pnl =
        history.realized_pnl + equity.floating_pnl + (history.funding / 2.0) - history.total_fees;

    let line_equal = "=".repeat(72);
    let line_dash = "-".repeat(72);

    let mut screen = Vec::new();
    screen.push(format!("  HL 做空动态网格机器人 <全景对账+智能平滑对准版 v3.8 (Rust)>"));
    screen.push(format!(
        "  模式: {} | 对齐: [{}] | 引擎: [{}]",
        mode_txt, align_txt, engine_txt
    ));
    screen.push(format!(
        "  时间: {} | 运行时间: {} | 通道: {}",
        time_str, runtime_str, ws_txt
    ));
    screen.push(format!("  {}", line_equal));

    screen.push("  [ 账户资产与盈亏透视 (链上同步) ]".to_string());
    if cfg.wallet_addr.is_empty() && cfg.secret_key.is_empty() {
        screen.push("  (未配置私钥或钱包地址，以模拟模式运行)".to_string());
    } else {
        screen.push(format!(
            "  当前总净值: {:.2} | 可用保证金: {:.2} | 总净盈亏: {:+.2}",
            equity.account_value, equity.withdrawable_margin, total_net_pnl
        ));
        screen.push(format!(
            "  盈亏拆解: 网格实现 {:+.2} | 当前浮动 {:+.2} | 资金费 {:+.2} | 手续费 -{:.2}",
            history.realized_pnl, equity.floating_pnl, history.funding / 2.0, history.total_fees
        ));
        screen.push(format!(
            "  策略统计: 累计建仓 {} 次 | 累计止盈 {} 次 | 真实总收益 {:+.2}",
            stats.entry_count, stats.tp_count, stats.true_profit
        ));
        screen.push(format!(
            "  程序账本: SKHY={:.4} ({:+.1}k) | SKHYNIX={:.4} ({:+.1}k)",
            -stats.pos_skhy,
            val_prog_y / 1000.0,
            stats.pos_skhx,
            val_prog_x / 1000.0
        ));
        screen.push(format!(
            "  链上真实: SKHY={:.4} ({:+.1}k) | SKHYNIX={:.4} ({:+.1}k)",
            equity.real_pos_skhy,
            val_real_y / 1000.0,
            equity.real_pos_skhx,
            val_real_x / 1000.0
        ));
    }
    screen.push(format!("  {}", line_dash));

    screen.push("  [ 行情盘口与策略状态 ]".to_string());
    screen.push(format!(
        "  # SKHY-USDC    : 买1={}  | 卖1={}",
        p_hy_bid_str, p_hy_ask_str
    ));
    screen.push(format!(
        "  # SKHYNIX-USDC : 买1={} | 卖1={}",
        p_hx_bid_str, p_hx_ask_str
    ));
    screen.push(format!(
        "  # 基准P1={} | 步长X={} | 单次={}",
        cfg.p1, cfg.step_x, cfg.q_usdc
    ));
    screen.push(format!(
        "  # 拆分={}笔({}ms) | 持有=[{}/{}层]",
        cfg.split_n, cfg.slice_delay_ms, stats.positions_held, stats.max_grids
    ));

    screen.push(format!("  {}", line_equal));
    screen.push(format!("  # 下一建仓线(做空): {}", next_entry_str));
    screen.push(format!(
        "  # 当前平滑值(原始): {} ({})",
        ema_k_str, raw_k_str
    ));
    screen.push(format!("  # 下一平仓线(止盈): {}", next_exit_str));
    screen.push(format!("  {}", line_equal));

    screen.push("  [ 底层交易与对账日志 ]".to_string());
    if logs.is_empty() {
        screen.push("  <策略监控中...等待行情触发>".to_string());
    } else {
        for l in logs {
            screen.push(format!("  {}", l));
        }
    }
    screen.push(format!("  {}", line_equal));
    screen.push("  [Ctrl+C 退出程序]\n".to_string());

    // Clear terminal screen and print
    print!("\x1B[2J\x1B[1;1H");
    println!("{}", screen.join("\n"));
}
