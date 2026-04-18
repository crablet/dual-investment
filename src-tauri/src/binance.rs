use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::{Client, StatusCode};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;

use crate::domain::{AppError, AppResult, StrategyDirection, StrategyRequest};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct BinanceClient {
    http: Client,
    api_key: String,
    api_secret: String,
    eapi_base: String,
    #[allow(dead_code)]
    spot_base: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeOrderAck {
    pub order_id: String,
    pub client_order_id: Option<String>,
    pub raw: Value,
}

impl BinanceClient {
    pub fn new(api_key: String, api_secret: String) -> Self {
        Self {
            http: Client::new(),
            api_key,
            api_secret,
            eapi_base: "https://eapi.binance.com".to_string(),
            spot_base: "https://api.binance.com".to_string(),
        }
    }

    pub async fn place_option_order(
        &self,
        request: &StrategyRequest,
        protected_limit_price: Decimal,
    ) -> AppResult<ExchangeOrderAck> {
        let option_symbol = request.option_symbol.clone().ok_or_else(|| {
            AppError::Config("真实期权下单必须提供 Binance option symbol".to_string())
        })?;
        let params = vec![
            ("symbol".to_string(), option_symbol),
            ("side".to_string(), "SELL".to_string()),
            ("type".to_string(), "LIMIT".to_string()),
            ("timeInForce".to_string(), "IOC".to_string()),
            (
                "quantity".to_string(),
                request.quantity.normalize().to_string(),
            ),
            (
                "price".to_string(),
                protected_limit_price.round_dp(2).normalize().to_string(),
            ),
            (
                "newClientOrderId".to_string(),
                format!(
                    "di-{}-{}",
                    request.direction.option_type().to_lowercase(),
                    Utc::now().timestamp_millis()
                ),
            ),
        ];
        self.signed_post(&self.eapi_base, "/eapi/v1/order", params)
            .await
    }

    #[allow(dead_code)]
    pub async fn place_spot_market_order(
        &self,
        symbol: &str,
        side: &str,
        quantity: Decimal,
    ) -> AppResult<ExchangeOrderAck> {
        let params = vec![
            ("symbol".to_string(), symbol.to_string()),
            ("side".to_string(), side.to_string()),
            ("type".to_string(), "MARKET".to_string()),
            ("quantity".to_string(), quantity.normalize().to_string()),
            (
                "newClientOrderId".to_string(),
                format!("di-spot-{}", Utc::now().timestamp_millis()),
            ),
        ];
        self.signed_post(&self.spot_base, "/api/v3/order", params)
            .await
    }

    #[allow(dead_code)]
    pub async fn signed_get(
        &self,
        base: &str,
        path: &str,
        params: Vec<(String, String)>,
    ) -> AppResult<Value> {
        let signed = self.signed_params(params)?;
        let response = self
            .http
            .get(format!("{base}{path}"))
            .header("X-MBX-APIKEY", &self.api_key)
            .query(&signed)
            .send()
            .await
            .map_err(|error| AppError::Binance(error.to_string()))?;
        Self::decode_response(response.status(), response.text().await).await
    }

    async fn signed_post(
        &self,
        base: &str,
        path: &str,
        params: Vec<(String, String)>,
    ) -> AppResult<ExchangeOrderAck> {
        let signed = self.signed_params(params)?;
        let response = self
            .http
            .post(format!("{base}{path}"))
            .header("X-MBX-APIKEY", &self.api_key)
            .query(&signed)
            .send()
            .await
            .map_err(|error| AppError::Binance(error.to_string()))?;
        let raw = Self::decode_response(response.status(), response.text().await).await?;
        let order_id = raw
            .get("orderId")
            .map(Value::to_string)
            .unwrap_or_else(|| format!("binance-{}", Utc::now().timestamp_millis()));
        let client_order_id = raw
            .get("clientOrderId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        Ok(ExchangeOrderAck {
            order_id,
            client_order_id,
            raw,
        })
    }

    fn signed_params(&self, mut params: Vec<(String, String)>) -> AppResult<Vec<(String, String)>> {
        params.push((
            "timestamp".to_string(),
            Utc::now().timestamp_millis().to_string(),
        ));
        params.push(("recvWindow".to_string(), "5000".to_string()));
        let query = encode_query(&params);
        let mut mac = HmacSha256::new_from_slice(self.api_secret.as_bytes())
            .map_err(|error| AppError::Config(error.to_string()))?;
        mac.update(query.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());
        params.push(("signature".to_string(), signature));
        Ok(params)
    }

    async fn decode_response(
        status: StatusCode,
        body_result: Result<String, reqwest::Error>,
    ) -> AppResult<Value> {
        let body = body_result.map_err(|error| AppError::Binance(error.to_string()))?;
        if !status.is_success() {
            return Err(AppError::Binance(format!("HTTP {status}: {body}")));
        }
        serde_json::from_str::<Value>(&body).map_err(|error| AppError::Binance(error.to_string()))
    }
}

fn encode_query(params: &[(String, String)]) -> String {
    params
        .iter()
        .map(|(key, value)| format!("{key}={}", urlencoding::encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

#[allow(dead_code)]
pub const fn spot_rebalance_side(direction: StrategyDirection) -> &'static str {
    match direction {
        StrategyDirection::SellPut => "BUY",
        StrategyDirection::SellCall => "SELL",
    }
}
