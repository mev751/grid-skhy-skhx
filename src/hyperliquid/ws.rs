use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{sleep, Duration};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone, Default)]
pub struct MarketPrices {
    pub skhy_bid: Option<f64>,
    pub skhy_ask: Option<f64>,
    pub skhx_bid: Option<f64>,
    pub skhx_ask: Option<f64>,
    pub is_connected: bool,
    pub last_update_ms: u64,
}

impl MarketPrices {
    pub fn current_ratio_k(&self) -> Option<f64> {
        match (self.skhy_bid, self.skhx_ask) {
            (Some(bid_y), Some(ask_x)) if ask_x > 0.0 => Some(bid_y * 10.0 / ask_x),
            _ => None,
        }
    }
}

pub struct WsManager {
    pub prices: Arc<RwLock<MarketPrices>>,
    pub is_running: Arc<AtomicBool>,
    coin_hy: String,
    coin_hx: String,
}

impl WsManager {
    pub fn new(coin_hy: &str, coin_hx: &str) -> Self {
        Self {
            prices: Arc::new(RwLock::new(MarketPrices::default())),
            is_running: Arc::new(AtomicBool::new(false)),
            coin_hy: coin_hy.to_string(),
            coin_hx: coin_hx.to_string(),
        }
    }

    pub fn start(&self, tick_tx: Option<mpsc::UnboundedSender<()>>) {
        let is_running = self.is_running.clone();
        let prices = self.prices.clone();
        let coin_hy = self.coin_hy.clone();
        let coin_hx = self.coin_hx.clone();

        is_running.store(true, Ordering::SeqCst);

        tokio::spawn(async move {
            let ws_url = "wss://api.hyperliquid.xyz/ws";

            while is_running.load(Ordering::SeqCst) {
                {
                    let mut p = prices.write().await;
                    p.is_connected = false;
                }

                match connect_async(ws_url).await {
                    Ok((ws_stream, _)) => {
                        {
                            let mut p = prices.write().await;
                            p.is_connected = true;
                        }

                        let (mut write, mut read) = ws_stream.split();

                        // 1. Subscribe to both L2 books
                        for coin in [&coin_hy, &coin_hx] {
                            let sub_msg = json!({
                                "method": "subscribe",
                                "subscription": {
                                    "type": "l2Book",
                                    "coin": coin
                                }
                            });
                            let _ = write.send(Message::Text(sub_msg.to_string())).await;
                        }

                        // 2. Ping loop task
                        let ping_handle = {
                            let is_running_ping = is_running.clone();
                            tokio::spawn(async move {
                                while is_running_ping.load(Ordering::SeqCst) {
                                    sleep(Duration::from_secs(5)).await;
                                    // Tungstenite Ping message is handled internally or send json ping
                                }
                            })
                        };

                        // 3. Reader loop
                        while let Some(msg_res) = read.next().await {
                            if !is_running.load(Ordering::SeqCst) {
                                break;
                            }

                            match msg_res {
                                Ok(Message::Text(text)) => {
                                    if let Ok(data) = serde_json::from_str::<Value>(&text) {
                                        if data.get("channel").and_then(|v| v.as_str()) == Some("l2Book") {
                                            if let Some(book_data) = data.get("data") {
                                                let coin = book_data.get("coin").and_then(|v| v.as_str()).unwrap_or("");
                                                if let Some(levels) = book_data.get("levels").and_then(|v| v.as_array()) {
                                                    let mut bid_opt = None;
                                                    let mut ask_opt = None;

                                                    if !levels.is_empty() {
                                                        if let Some(bids) = levels[0].as_array() {
                                                            if let Some(b) = bids.first() {
                                                                if let Some(px_str) = b.get("px").and_then(|v| v.as_str()) {
                                                                    bid_opt = px_str.parse::<f64>().ok();
                                                                }
                                                            }
                                                        }
                                                    }

                                                    if levels.len() > 1 {
                                                        if let Some(asks) = levels[1].as_array() {
                                                            if let Some(a) = asks.first() {
                                                                if let Some(px_str) = a.get("px").and_then(|v| v.as_str()) {
                                                                    ask_opt = px_str.parse::<f64>().ok();
                                                                }
                                                            }
                                                        }
                                                    }

                                                    let mut p = prices.write().await;
                                                    let now_ms = std::time::SystemTime::now()
                                                        .duration_since(std::time::UNIX_EPOCH)
                                                        .unwrap_or_default()
                                                        .as_millis() as u64;
                                                    p.last_update_ms = now_ms;

                                                    if coin == coin_hy {
                                                        if bid_opt.is_some() { p.skhy_bid = bid_opt; }
                                                        if ask_opt.is_some() { p.skhy_ask = ask_opt; }
                                                    } else if coin == coin_hx {
                                                        if bid_opt.is_some() { p.skhx_bid = bid_opt; }
                                                        if ask_opt.is_some() { p.skhx_ask = ask_opt; }
                                                    }

                                                    // Notify engine tick
                                                    if let Some(ref tx) = tick_tx {
                                                        let _ = tx.send(());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Ok(Message::Ping(payload)) => {
                                    let _ = write.send(Message::Pong(payload)).await;
                                }
                                Ok(Message::Close(_)) => {
                                    break;
                                }
                                Err(_) => {
                                    break;
                                }
                                _ => {}
                            }
                        }

                        ping_handle.abort();
                    }
                    Err(_) => {
                        // Connection failed, wait and retry
                        sleep(Duration::from_secs(2)).await;
                    }
                }

                sleep(Duration::from_millis(500)).await;
            }
        });
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
    }
}
