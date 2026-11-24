use super::signer::{ExchangeRequest, Signer};
use crate::types::{CancelWire, LimitTifWire, OrderTypeWire, OrderWire};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn format_price(px: f64) -> String {
    // Hyperliquid expects up to 5 significant digits or standard decimal string
    let s = format!("{:.5}", px);
    if s.contains('.') {
        let trimmed = s.trim_end_matches('0').trim_end_matches('.');
        trimmed.to_string()
    } else {
        s
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderActionWire {
    #[serde(rename = "type")]
    pub action_type: String,
    pub orders: Vec<OrderWire>,
    pub grouping: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelActionWire {
    #[serde(rename = "type")]
    pub action_type: String,
    pub cancels: Vec<CancelWire>,
}

#[derive(Debug, Clone, Default)]
pub struct OrderExecutionReport {
    pub success: bool,
    pub order_id: Option<u64>,
    pub error_msg: Option<String>,
    pub filled: bool,
    pub resting: bool,
}

pub struct ExchangeClient {
    client: Client,
    signer: Signer,
    pub exchange_url: String,
    pub is_mainnet: bool,
    pub vault_address: Option<String>,
}

impl ExchangeClient {
    pub fn new(secret_key: &str) -> Result<Self, String> {
        let signer = Signer::new(secret_key)?;
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_else(|_| Client::new()),
            signer,
            exchange_url: "https://api.hyperliquid.xyz/exchange".to_string(),
            is_mainnet: true,
            vault_address: None,
        })
    }

    pub fn wallet_address(&self) -> &str {
        &self.signer.address
    }

    pub async fn place_orders(
        &self,
        orders: Vec<OrderWire>,
    ) -> Result<Vec<OrderExecutionReport>, String> {
        let action = OrderActionWire {
            action_type: "order".to_string(),
            orders,
            grouping: "na".to_string(),
        };

        let nonce = Signer::get_timestamp_ms();
        let sig = self.signer.sign_action(
            &action,
            self.vault_address.as_deref(),
            nonce,
            self.is_mainnet,
        )?;

        let request = ExchangeRequest {
            action,
            nonce,
            signature: sig,
            vault_address: self.vault_address.clone(),
        };

        let resp = self
            .client
            .post(&self.exchange_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Place orders HTTP error: {}", e))?;

        let body: Value = resp.json().await.map_err(|e| format!("Place orders JSON error: {}", e))?;
        
        let mut reports = Vec::new();
        if let Some(status) = body.get("status").and_then(|v| v.as_str()) {
            if status == "ok" {
                if let Some(statuses) = body
                    .get("response")
                    .and_then(|r| r.get("data"))
                    .and_then(|d| d.get("statuses"))
                    .and_then(|s| s.as_array())
                {
                    for s in statuses {
                        if let Some(resting) = s.get("resting") {
                            let oid = resting.get("oid").and_then(|v| v.as_u64());
                            reports.push(OrderExecutionReport {
                                success: true,
                                order_id: oid,
                                error_msg: None,
                                filled: false,
                                resting: true,
                            });
                        } else if let Some(filled) = s.get("filled") {
                            let oid = filled.get("oid").and_then(|v| v.as_u64());
                            reports.push(OrderExecutionReport {
                                success: true,
                                order_id: oid,
                                error_msg: None,
                                filled: true,
                                resting: false,
                            });
                        } else if let Some(err) = s.get("error").and_then(|v| v.as_str()) {
                            reports.push(OrderExecutionReport {
                                success: false,
                                order_id: None,
                                error_msg: Some(err.to_string()),
                                filled: false,
                                resting: false,
                            });
                        }
                    }
                }
            } else {
                let err_str = body.to_string();
                reports.push(OrderExecutionReport {
                    success: false,
                    order_id: None,
                    error_msg: Some(err_str),
                    filled: false,
                    resting: false,
                });
            }
        }

        Ok(reports)
    }

    pub async fn cancel_orders(&self, cancels: Vec<CancelWire>) -> Result<bool, String> {
        if cancels.is_empty() {
            return Ok(true);
        }

        let action = CancelActionWire {
            action_type: "cancel".to_string(),
            cancels,
        };

        let nonce = Signer::get_timestamp_ms();
        let sig = self.signer.sign_action(
            &action,
            self.vault_address.as_deref(),
            nonce,
            self.is_mainnet,
        )?;

        let request = ExchangeRequest {
            action,
            nonce,
            signature: sig,
            vault_address: self.vault_address.clone(),
        };

        let resp = self
            .client
            .post(&self.exchange_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Cancel orders HTTP error: {}", e))?;

        let body: Value = resp.json().await.map_err(|e| format!("Cancel orders JSON error: {}", e))?;
        if let Some(status) = body.get("status").and_then(|v| v.as_str()) {
            return Ok(status == "ok");
        }

        Ok(false)
    }

    pub fn build_order_wire(
        asset: u32,
        is_buy: bool,
        limit_px: f64,
        sz: f64,
        reduce_only: bool,
        tif: &str, // "Alo", "Ioc", "Gtc"
    ) -> OrderWire {
        OrderWire {
            asset,
            is_buy,
            limit_px: format_price(limit_px),
            sz: format!("{:.4}", sz),
            reduce_only,
            order_type: OrderTypeWire {
                limit: LimitTifWire {
                    tif: tif.to_string(),
                },
            },
        }
    }
}
