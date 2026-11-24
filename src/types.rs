use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderTif {
    Alo, // Add Liquidity Only (Post-Only Maker)
    Ioc, // Immediate Or Cancel (Taker)
    Gtc, // Good Til Cancel
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderWire {
    #[serde(rename = "a")]
    pub asset: u32,
    #[serde(rename = "b")]
    pub is_buy: bool,
    #[serde(rename = "p")]
    pub limit_px: String,
    #[serde(rename = "s")]
    pub sz: String,
    #[serde(rename = "r")]
    pub reduce_only: bool,
    #[serde(rename = "t")]
    pub order_type: OrderTypeWire,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderTypeWire {
    pub limit: LimitTifWire,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitTifWire {
    pub tif: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelWire {
    #[serde(rename = "a")]
    pub asset: u32,
    #[serde(rename = "o")]
    pub oid: u64,
}

#[derive(Debug, Clone, Default)]
pub struct BookLevel {
    pub px: f64,
    pub sz: f64,
}

#[derive(Debug, Clone, Default)]
pub struct L2Prices {
    pub bid: Option<f64>,
    pub ask: Option<f64>,
}
