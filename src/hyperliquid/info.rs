use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct AssetInfo {
    pub asset_id: u32,
    pub sz_decimals: u32,
}

#[derive(Debug, Clone, Default)]
pub struct AccountEquityState {
    pub account_value: f64,
    pub withdrawable_margin: f64,
    pub spot_usdc: f64,
    pub real_pos_skhy: f64,
    pub real_pos_skhx: f64,
    pub floating_pnl: f64,
}

#[derive(Debug, Clone, Default)]
pub struct TradeHistoryStats {
    pub realized_pnl: f64,
    pub total_fees: f64,
    pub funding: f64,
}

pub struct InfoClient {
    client: Client,
    pub base_url: String,
}

impl InfoClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| Client::new()),
            base_url: "https://api.hyperliquid.xyz/info".to_string(),
        }
    }

    pub async fn fetch_perp_dexs(&self) -> Result<Vec<String>, String> {
        let resp = self
            .client
            .post(&self.base_url)
            .json(&json!({"type": "perpDexs"}))
            .send()
            .await
            .map_err(|e| format!("perpDexs req failed: {}", e))?;

        let val: Value = resp.json().await.map_err(|e| format!("perpDexs json: {}", e))?;
        let mut dexs = Vec::new();
        if let Value::Array(arr) = val {
            for item in arr {
                if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                    dexs.push(name.to_string());
                }
            }
        }
        Ok(dexs)
    }

    /// Resolve asset ID and size decimals for a coin, e.g. "xyz:SKHY" or "xyz:SKHX"
    pub async fn resolve_assets(
        &self,
        coins: &[&str],
    ) -> Result<HashMap<String, AssetInfo>, String> {
        let mut map = HashMap::new();
        let perp_dexs = self.fetch_perp_dexs().await.unwrap_or_default();

        // 1. Fetch native perps meta
        let native_resp = self
            .client
            .post(&self.base_url)
            .json(&json!({"type": "meta"}))
            .send()
            .await
            .map_err(|e| format!("meta req failed: {}", e))?;

        if let Ok(Value::Object(native_meta)) = native_resp.json::<Value>().await {
            if let Some(Value::Array(universe)) = native_meta.get("universe") {
                for (idx, item) in universe.iter().enumerate() {
                    if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                        let sz_dec = item.get("szDecimals").and_then(|v| v.as_u64()).unwrap_or(4) as u32;
                        map.insert(
                            name.to_string(),
                            AssetInfo {
                                asset_id: idx as u32,
                                sz_decimals: sz_dec,
                            },
                        );
                    }
                }
            }
        }

        // 2. Fetch builder DEXs (e.g. "xyz")
        for (i, dex_name) in perp_dexs.iter().enumerate() {
            let offset: u32 = 110_000 + (i as u32) * 10_000;
            let dex_resp = self
                .client
                .post(&self.base_url)
                .json(&json!({"type": "meta", "dex": dex_name}))
                .send()
                .await;

            if let Ok(resp) = dex_resp {
                if let Ok(Value::Object(dex_meta)) = resp.json::<Value>().await {
                    if let Some(Value::Array(universe)) = dex_meta.get("universe") {
                        for (idx, item) in universe.iter().enumerate() {
                            if let Some(coin_name) = item.get("name").and_then(|v| v.as_str()) {
                                let sz_dec = item.get("szDecimals").and_then(|v| v.as_u64()).unwrap_or(4) as u32;
                                let asset_id = offset + (idx as u32);
                                // Store as "xyz:SKHY", "SKHY", and coin_name
                                let full_sym = format!("{}:{}", dex_name, coin_name);
                                map.insert(
                                    full_sym.clone(),
                                    AssetInfo {
                                        asset_id,
                                        sz_decimals: sz_dec,
                                    },
                                );
                                map.insert(
                                    coin_name.to_string(),
                                    AssetInfo {
                                        asset_id,
                                        sz_decimals: sz_dec,
                                    },
                                );
                            }
                        }
                    }
                }
            }
        }

        // Fallback defaults for SKHY and SKHX if network/meta is missing
        for coin in coins {
            if !map.contains_key(*coin) {
                // If not found, assign safe defaults
                let fallback_id = if coin.contains("SKHX") || coin.contains("NIX") {
                    110001
                } else {
                    110000
                };
                map.insert(
                    coin.to_string(),
                    AssetInfo {
                        asset_id: fallback_id,
                        sz_decimals: 4,
                    },
                );
            }
        }

        Ok(map)
    }

    pub async fn fetch_l2_book(&self, coin: &str) -> Result<(Option<f64>, Option<f64>), String> {
        let resp = self
            .client
            .post(&self.base_url)
            .json(&json!({"type": "l2Book", "coin": coin}))
            .send()
            .await
            .map_err(|e| format!("l2Book req error: {}", e))?;

        let data: Value = resp.json().await.map_err(|e| format!("l2Book json error: {}", e))?;
        let mut bid = None;
        let mut ask = None;

        if let Some(levels) = data.get("levels").and_then(|v| v.as_array()) {
            if !levels.is_empty() {
                if let Some(bids) = levels[0].as_array() {
                    if let Some(first_bid) = bids.first() {
                        if let Some(px_str) = first_bid.get("px").and_then(|v| v.as_str()) {
                            bid = px_str.parse::<f64>().ok();
                        }
                    }
                }
            }
            if levels.len() > 1 {
                if let Some(asks) = levels[1].as_array() {
                    if let Some(first_ask) = asks.first() {
                        if let Some(px_str) = first_ask.get("px").and_then(|v| v.as_str()) {
                            ask = px_str.parse::<f64>().ok();
                        }
                    }
                }
            }
        }

        Ok((bid, ask))
    }

    pub async fn fetch_account_state(&self, wallet: &str) -> Result<AccountEquityState, String> {
        let mut state = AccountEquityState::default();

        // 1. Spot USDC balance
        if let Ok(resp) = self
            .client
            .post(&self.base_url)
            .json(&json!({"type": "spotClearinghouseState", "user": wallet}))
            .send()
            .await
        {
            if let Ok(data) = resp.json::<Value>().await {
                if let Some(balances) = data.get("balances").and_then(|v| v.as_array()) {
                    for b in balances {
                        if b.get("coin").and_then(|v| v.as_str()) == Some("USDC") {
                            state.spot_usdc = b
                                .get("total")
                                .and_then(|v| v.as_str())
                                .and_then(|s| s.parse::<f64>().ok())
                                .unwrap_or(0.0);
                        }
                    }
                }
            }
        }

        // 2. Perp XYZ clearinghouse state
        let mut margin_used_xyz = 0.0;
        if let Ok(resp) = self
            .client
            .post(&self.base_url)
            .json(&json!({"type": "clearinghouseState", "user": wallet, "dex": "xyz"}))
            .send()
            .await
        {
            if let Ok(data) = resp.json::<Value>().await {
                if let Some(summary) = data.get("marginSummary") {
                    margin_used_xyz = summary
                        .get("totalMarginUsed")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0);
                }
                let withdrawable = data
                    .get("withdrawable")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);

                if state.spot_usdc > 0.0 {
                    state.withdrawable_margin = (state.spot_usdc - margin_used_xyz).max(0.0);
                } else {
                    state.withdrawable_margin = withdrawable;
                }

                if let Some(asset_positions) = data.get("assetPositions").and_then(|v| v.as_array()) {
                    for item in asset_positions {
                        if let Some(pos) = item.get("position") {
                            let coin = pos.get("coin").and_then(|v| v.as_str()).unwrap_or("").to_uppercase();
                            let szi = pos
                                .get("szi")
                                .and_then(|v| v.as_str())
                                .and_then(|s| s.parse::<f64>().ok())
                                .unwrap_or(0.0);
                            let upnl = pos
                                .get("unrealizedPnl")
                                .and_then(|v| v.as_str())
                                .and_then(|s| s.parse::<f64>().ok())
                                .unwrap_or(0.0);

                            if coin.contains("SKHX") || coin.contains("NIX") {
                                state.real_pos_skhx = szi;
                                state.floating_pnl += upnl;
                            } else if coin.contains("SKHY") {
                                state.real_pos_skhy = szi;
                                state.floating_pnl += upnl;
                            }
                        }
                    }
                }
            }
        }

        // 3. Native perp clearinghouse state
        if let Ok(resp) = self
            .client
            .post(&self.base_url)
            .json(&json!({"type": "clearinghouseState", "user": wallet}))
            .send()
            .await
        {
            if let Ok(data) = resp.json::<Value>().await {
                if let Some(summary) = data.get("marginSummary") {
                    let acct_val = summary
                        .get("accountValue")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0);
                    state.account_value = if acct_val > 0.0 { acct_val } else { state.spot_usdc };
                }
            }
        }

        Ok(state)
    }

    pub async fn fetch_trade_history(
        &self,
        wallet: &str,
        bot_start_time_ms: u64,
        processed_fill_ids: &mut std::collections::HashSet<String>,
    ) -> Result<TradeHistoryStats, String> {
        let mut stats = TradeHistoryStats::default();

        // 1. User fills
        if let Ok(resp) = self
            .client
            .post(&self.base_url)
            .json(&json!({"type": "userFills", "user": wallet}))
            .send()
            .await
        {
            if let Ok(Value::Array(fills)) = resp.json::<Value>().await {
                for f in fills {
                    let time = f.get("time").and_then(|v| v.as_u64()).unwrap_or(0);
                    if time >= bot_start_time_ms {
                        let coin = f.get("coin").and_then(|v| v.as_str()).unwrap_or("");
                        if coin.contains("SKHY") || coin.contains("SKHX") || coin.contains("NIX") {
                            let pnl = f
                                .get("closedPnl")
                                .and_then(|v| v.as_str())
                                .and_then(|s| s.parse::<f64>().ok())
                                .unwrap_or(0.0);
                            stats.realized_pnl += pnl;

                            let fill_id = f
                                .get("tid")
                                .and_then(|v| v.as_u64())
                                .map(|t| t.to_string())
                                .unwrap_or_else(|| {
                                    format!(
                                        "{}_{}_{}",
                                        time,
                                        coin,
                                        f.get("sz").and_then(|v| v.as_str()).unwrap_or("")
                                    )
                                });

                            if !processed_fill_ids.contains(&fill_id) {
                                processed_fill_ids.insert(fill_id);
                                let fee = f
                                    .get("fee")
                                    .and_then(|v| v.as_str())
                                    .and_then(|s| s.parse::<f64>().ok())
                                    .unwrap_or(0.0)
                                    .abs();
                                stats.total_fees += fee;
                            }
                        }
                    }
                }
            }
        }

        // 2. User funding (checking both default and dex: xyz)
        for dex_param in [None, Some("xyz")] {
            let mut payload = json!({
                "type": "userFunding",
                "user": wallet,
                "startTime": bot_start_time_ms
            });
            if let Some(dex) = dex_param {
                payload["dex"] = json!(dex);
            }

            if let Ok(resp) = self.client.post(&self.base_url).json(&payload).send().await {
                if let Ok(Value::Array(funds)) = resp.json::<Value>().await {
                    for f in funds {
                        let mut coin = f.get("coin").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        if coin.is_empty() {
                            if let Some(delta) = f.get("delta") {
                                coin = delta.get("coin").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            }
                        }

                        if coin.contains("SKHY") || coin.contains("SKHX") || coin.contains("NIX") {
                            if let Some(usdc_val) = f.get("usdc").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok()) {
                                stats.funding += usdc_val;
                            } else if let Some(delta_usdc) = f
                                .get("delta")
                                .and_then(|d| d.get("usdc"))
                                .and_then(|v| v.as_str())
                                .and_then(|s| s.parse::<f64>().ok())
                            {
                                stats.funding += delta_usdc;
                            }
                        }
                    }
                }
            }
        }

        Ok(stats)
    }
}
