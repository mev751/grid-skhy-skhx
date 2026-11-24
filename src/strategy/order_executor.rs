use crate::hyperliquid::exchange::ExchangeClient;
use crate::hyperliquid::info::AssetInfo;
use crate::hyperliquid::ws::MarketPrices;
use crate::types::CancelWire;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeAction {
    Entry,
    Exit,
}

pub struct OrderExecutor {
    exchange: Arc<ExchangeClient>,
    prices: Arc<RwLock<MarketPrices>>,
    asset_map: HashMap<String, AssetInfo>,
    coin_hy: String,
    coin_hx: String,
    maker_timeout_ms: u64,
    maker_max_retries: usize,
}

impl OrderExecutor {
    pub fn new(
        exchange: Arc<ExchangeClient>,
        prices: Arc<RwLock<MarketPrices>>,
        asset_map: HashMap<String, AssetInfo>,
        coin_hy: &str,
        coin_hx: &str,
        maker_timeout_ms: u64,
        maker_max_retries: usize,
    ) -> Self {
        Self {
            exchange,
            prices,
            asset_map,
            coin_hy: coin_hy.to_string(),
            coin_hx: coin_hx.to_string(),
            maker_timeout_ms,
            maker_max_retries,
        }
    }

    /// Execute dual-leg order: 2-stage Maker order attempts, followed by Taker fallback
    pub async fn execute_slice(
        &self,
        action: TradeAction,
        sz_hy: f64,
        sz_hx: f64,
        _is_last_close: bool,
    ) -> Result<String, String> {
        let asset_hy = self.asset_map.get(&self.coin_hy).map(|a| a.asset_id).unwrap_or(110000);
        let asset_hx = self.asset_map.get(&self.coin_hx).map(|a| a.asset_id).unwrap_or(110001);

        // Action determines sides:
        // Entry: Short HY (Sell HY), Long HX (Buy HX), reduce_only = false
        // Exit: Long HY (Buy HY), Short HX (Sell HX), reduce_only = true
        let (is_buy_hy, is_buy_hx, reduce_only) = match action {
            TradeAction::Entry => (false, true, false),
            TradeAction::Exit => (true, false, true),
        };

        let mut remaining_hy = sz_hy;
        let mut remaining_hx = sz_hx;

        let mut attempt = 0;
        let mut filled_hy = 0.0;
        let mut filled_hx = 0.0;

        // Stage 1 & 2: Maker attempts (Alo = Add Liquidity Only)
        while attempt < self.maker_max_retries && (remaining_hy > 0.0001 || remaining_hx > 0.0001) {
            attempt += 1;

            let (p_hy_maker, p_hx_maker) = self.get_maker_prices(is_buy_hy, is_buy_hx).await?;

            let mut orders = Vec::new();
            if remaining_hy > 0.0001 {
                orders.push(ExchangeClient::build_order_wire(
                    asset_hy,
                    is_buy_hy,
                    p_hy_maker,
                    remaining_hy,
                    reduce_only,
                    "Alo",
                ));
            }
            if remaining_hx > 0.0001 {
                orders.push(ExchangeClient::build_order_wire(
                    asset_hx,
                    is_buy_hx,
                    p_hx_maker,
                    remaining_hx,
                    reduce_only,
                    "Alo",
                ));
            }

            let mut active_cancels = Vec::new();
            if !orders.is_empty() {
                if let Ok(reports) = self.exchange.place_orders(orders).await {
                    for (idx, rep) in reports.iter().enumerate() {
                        if rep.filled {
                            if idx == 0 && remaining_hy > 0.0001 {
                                filled_hy += remaining_hy;
                                remaining_hy = 0.0;
                            } else {
                                filled_hx += remaining_hx;
                                remaining_hx = 0.0;
                            }
                        } else if rep.resting {
                            if let Some(oid) = rep.order_id {
                                let asset = if idx == 0 && remaining_hy > 0.0001 {
                                    asset_hy
                                } else {
                                    asset_hx
                                };
                                active_cancels.push(CancelWire { asset, oid });
                            }
                        }
                    }
                }
            }

            // If both sides filled immediately, return success
            if remaining_hy <= 0.0001 && remaining_hx <= 0.0001 {
                return Ok(format!("Maker 第{}次挂单双侧全部即时成交!", attempt));
            }

            // Wait for maker fill
            sleep(Duration::from_millis(self.maker_timeout_ms)).await;

            // Cancel any remaining resting orders before next attempt or taker fallback
            if !active_cancels.is_empty() {
                let _ = self.exchange.cancel_orders(active_cancels).await;
            }
        }

        // Stage 3: Taker fallback (IOC) for any remaining quantities
        if remaining_hy > 0.0001 || remaining_hx > 0.0001 {
            let (p_hy_taker, p_hx_taker) = self.get_taker_prices(is_buy_hy, is_buy_hx).await?;

            let mut taker_orders = Vec::new();
            if remaining_hy > 0.0001 {
                taker_orders.push(ExchangeClient::build_order_wire(
                    asset_hy,
                    is_buy_hy,
                    p_hy_taker,
                    remaining_hy,
                    reduce_only,
                    "Ioc",
                ));
            }
            if remaining_hx > 0.0001 {
                taker_orders.push(ExchangeClient::build_order_wire(
                    asset_hx,
                    is_buy_hx,
                    p_hx_taker,
                    remaining_hx,
                    reduce_only,
                    "Ioc",
                ));
            }

            let mut ok_hy = false;
            let mut ok_hx = false;

            if let Ok(reports) = self.exchange.place_orders(taker_orders).await {
                for (idx, rep) in reports.iter().enumerate() {
                    if rep.filled || rep.success {
                        if idx == 0 && remaining_hy > 0.0001 {
                            ok_hy = true;
                            filled_hy += remaining_hy;
                            remaining_hy = 0.0;
                        } else {
                            ok_hx = true;
                            filled_hx += remaining_hx;
                            remaining_hx = 0.0;
                        }
                    }
                }
            }

            // Dual-leg imbalance protection:
            // If one leg succeeded but the other failed, hedge the imbalanced leg immediately!
            if action == TradeAction::Entry {
                if ok_hx && !ok_hy {
                    // SKHX succeeded, SKHY failed -> Close excess SKHX immediately
                    let sell_px_hx = p_hx_taker * 0.95;
                    let hedge_order = ExchangeClient::build_order_wire(
                        asset_hx,
                        false,
                        sell_px_hx,
                        sz_hx,
                        true,
                        "Ioc",
                    );
                    let _ = self.exchange.place_orders(vec![hedge_order]).await;
                    return Err("SKHY建仓失败，已平多余SKHX保持平衡".to_string());
                } else if !ok_hx && ok_hy {
                    // SKHY succeeded, SKHX failed -> Close excess SKHY immediately
                    let buy_px_hy = p_hy_taker * 1.05;
                    let hedge_order = ExchangeClient::build_order_wire(
                        asset_hy,
                        true,
                        buy_px_hy,
                        sz_hy,
                        true,
                        "Ioc",
                    );
                    let _ = self.exchange.place_orders(vec![hedge_order]).await;
                    return Err("SKHX建仓失败，已平多余SKHY保持平衡".to_string());
                }
            }

            return Ok(format!(
                "2次Maker未完全成交，Taker兜底扫单完成 (HY={:.4}, HX={:.4})",
                filled_hy, filled_hx
            ));
        }

        Ok("Maker双侧下单完全成交".to_string())
    }

    async fn get_maker_prices(&self, is_buy_hy: bool, is_buy_hx: bool) -> Result<(f64, f64), String> {
        let p = self.prices.read().await;
        let p_hy = if is_buy_hy {
            p.skhy_bid.ok_or("SKHY Bid盘口未就绪")?
        } else {
            p.skhy_ask.ok_or("SKHY Ask盘口未就绪")?
        };

        let p_hx = if is_buy_hx {
            p.skhx_bid.ok_or("SKHX Bid盘口未就绪")?
        } else {
            p.skhx_ask.ok_or("SKHX Ask盘口未就绪")?
        };

        Ok((p_hy, p_hx))
    }

    async fn get_taker_prices(&self, is_buy_hy: bool, is_buy_hx: bool) -> Result<(f64, f64), String> {
        let p = self.prices.read().await;
        let p_hy = if is_buy_hy {
            p.skhy_ask.ok_or("SKHY Ask盘口未就绪")? * 1.05
        } else {
            p.skhy_bid.ok_or("SKHY Bid盘口未就绪")? * 0.95
        };

        let p_hx = if is_buy_hx {
            p.skhx_ask.ok_or("SKHX Ask盘口未就绪")? * 1.05
        } else {
            p.skhx_bid.ok_or("SKHX Bid盘口未就绪")? * 0.95
        };

        Ok((p_hy, p_hx))
    }
}
