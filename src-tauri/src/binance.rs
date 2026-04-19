use std::str::FromStr;
use std::time::Duration;

use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use reqwest::{Client, StatusCode};
use rust_decimal::Decimal;
use serde_json::Value;
use sha2::Sha256;

use crate::domain::{
    AppError, AppResult, ExchangeOrderAck, ExerciseRecord, OptionAccountSnapshot,
    OptionAssetBalance, OptionContract, OptionMarkPrice, SpotAccountSnapshot, SpotAssetBalance,
    StrategyDirection, StrategyRequest,
};

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

impl BinanceClient {
    pub fn new(api_key: String, api_secret: String) -> AppResult<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(12))
            .build()
            .map_err(|error| AppError::Config(error.to_string()))?;

        Ok(Self {
            http,
            api_key,
            api_secret,
            eapi_base: "https://eapi.binance.com".to_string(),
            spot_base: "https://api.binance.com".to_string(),
        })
    }

    pub async fn verify_account(&self) -> AppResult<OptionAccountSnapshot> {
        let account = self.option_account().await?;
        if !account.can_trade {
            return Err(AppError::Config(
                "Binance 期权账户当前不可交易，请检查 API 权限和账户状态".to_string(),
            ));
        }
        Ok(account)
    }

    pub async fn option_account(&self) -> AppResult<OptionAccountSnapshot> {
        let response = self
            .signed_get(&self.eapi_base, "/eapi/v1/marginAccount", Vec::new())
            .await?;
        parse_account_snapshot(&response)
    }

    pub async fn exchange_info(&self) -> AppResult<Vec<OptionContract>> {
        let response = self
            .public_get(&self.eapi_base, "/eapi/v1/exchangeInfo")
            .await?;
        parse_exchange_info(&response)
    }

    pub async fn option_mark_prices(&self) -> AppResult<Vec<OptionMarkPrice>> {
        let response = self.public_get(&self.eapi_base, "/eapi/v1/mark").await?;
        parse_mark_prices(&response)
    }

    pub async fn place_option_order(
        &self,
        request: &StrategyRequest,
        client_order_id: &str,
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
                request.limit_price.normalize().to_string(),
            ),
            ("clientOrderId".to_string(), client_order_id.to_string()),
            ("newOrderRespType".to_string(), "RESULT".to_string()),
        ];
        self.signed_post(&self.eapi_base, "/eapi/v1/order", params)
            .await
    }

    pub async fn query_option_order(
        &self,
        symbol: &str,
        order_id: Option<&str>,
        client_order_id: Option<&str>,
    ) -> AppResult<ExchangeOrderAck> {
        let mut params = vec![("symbol".to_string(), symbol.to_string())];
        if let Some(order_id) = order_id.filter(|value| !value.trim().is_empty()) {
            params.push(("orderId".to_string(), order_id.to_string()));
        }
        if let Some(client_order_id) = client_order_id.filter(|value| !value.trim().is_empty()) {
            params.push(("clientOrderId".to_string(), client_order_id.to_string()));
        }
        let response = self
            .signed_get(&self.eapi_base, "/eapi/v1/order", params)
            .await?;
        parse_order_ack(&response)
    }

    pub async fn spot_account(&self) -> AppResult<SpotAccountSnapshot> {
        let response = self
            .signed_get(&self.spot_base, "/api/v3/account", Vec::new())
            .await?;
        parse_spot_account(&response)
    }

    pub async fn spot_price(&self, symbol: &str) -> AppResult<Decimal> {
        let response = self
            .public_get(
                &self.spot_base,
                &format!("/api/v3/ticker/price?symbol={symbol}"),
            )
            .await?;
        decimal_field(&response, "price")
    }

    pub async fn exercise_records(
        &self,
        symbol: &str,
        start_time_ms: i64,
        end_time_ms: i64,
    ) -> AppResult<Vec<ExerciseRecord>> {
        let response = self
            .signed_get(
                &self.eapi_base,
                "/eapi/v1/exerciseRecord",
                vec![
                    ("symbol".to_string(), symbol.to_string()),
                    ("startTime".to_string(), start_time_ms.to_string()),
                    ("endTime".to_string(), end_time_ms.to_string()),
                ],
            )
            .await?;
        parse_exercise_records(&response)
    }

    #[allow(dead_code)]
    pub async fn place_spot_market_order(
        &self,
        symbol: &str,
        side: &str,
        quantity: Decimal,
        client_order_id: &str,
    ) -> AppResult<ExchangeOrderAck> {
        let params = vec![
            ("symbol".to_string(), symbol.to_string()),
            ("side".to_string(), side.to_string()),
            ("type".to_string(), "MARKET".to_string()),
            ("quantity".to_string(), quantity.normalize().to_string()),
            ("newClientOrderId".to_string(), client_order_id.to_string()),
        ];
        self.signed_post(&self.spot_base, "/api/v3/order", params)
            .await
    }

    pub async fn query_spot_order(
        &self,
        symbol: &str,
        order_id: Option<&str>,
        client_order_id: Option<&str>,
    ) -> AppResult<ExchangeOrderAck> {
        let mut params = vec![("symbol".to_string(), symbol.to_string())];
        if let Some(order_id) = order_id.filter(|value| !value.trim().is_empty()) {
            params.push(("orderId".to_string(), order_id.to_string()));
        }
        if let Some(client_order_id) = client_order_id.filter(|value| !value.trim().is_empty()) {
            params.push(("origClientOrderId".to_string(), client_order_id.to_string()));
        }
        let response = self
            .signed_get(&self.spot_base, "/api/v3/order", params)
            .await?;
        parse_order_ack(&response)
    }

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
        Self::decode_response(response.status(), response.text().await)
    }

    async fn public_get(&self, base: &str, path: &str) -> AppResult<Value> {
        let response = self
            .http
            .get(format!("{base}{path}"))
            .send()
            .await
            .map_err(|error| AppError::Binance(error.to_string()))?;
        Self::decode_response(response.status(), response.text().await)
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
        let response_json = Self::decode_response(response.status(), response.text().await)?;
        parse_order_ack(&response_json)
    }

    fn signed_params(&self, mut params: Vec<(String, String)>) -> AppResult<Vec<(String, String)>> {
        params.push((
            "timestamp".to_string(),
            Utc::now().timestamp_millis().to_string(),
        ));
        params.push(("recvWindow".to_string(), "5000".to_string()));
        let query = encode_query(&params);
        let mut mac = <HmacSha256 as KeyInit>::new_from_slice(self.api_secret.as_bytes())
            .map_err(|error| AppError::Config(error.to_string()))?;
        mac.update(query.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());
        params.push(("signature".to_string(), signature));
        Ok(params)
    }

    fn decode_response(
        status: StatusCode,
        body_result: Result<String, reqwest::Error>,
    ) -> AppResult<Value> {
        let body = body_result.map_err(|error| AppError::Binance(error.to_string()))?;
        if !status.is_success() {
            if let Ok(value) = serde_json::from_str::<Value>(&body) {
                let code = value
                    .get("code")
                    .map_or_else(|| status.as_u16().to_string(), value_to_string);
                let message = value
                    .get("msg")
                    .and_then(Value::as_str)
                    .unwrap_or("请求被 Binance 拒绝");
                return Err(AppError::Binance(format!(
                    "Binance 请求失败（HTTP {status}, code {code}）：{message}"
                )));
            }
            return Err(AppError::Binance(format!(
                "Binance 请求失败（HTTP {status}）"
            )));
        }
        serde_json::from_str::<Value>(&body).map_err(|error| AppError::Binance(error.to_string()))
    }
}

fn parse_account_snapshot(value: &Value) -> AppResult<OptionAccountSnapshot> {
    let assets = value
        .get("asset")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Binance("Binance 账户响应缺少 asset 字段".to_string()))?
        .iter()
        .map(parse_asset_balance)
        .collect::<AppResult<Vec<_>>>()?;

    let time = value
        .get("time")
        .and_then(Value::as_i64)
        .and_then(DateTime::<Utc>::from_timestamp_millis);

    Ok(OptionAccountSnapshot {
        assets,
        can_trade: value
            .get("canTrade")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        time,
    })
}

fn parse_asset_balance(value: &Value) -> AppResult<OptionAssetBalance> {
    Ok(OptionAssetBalance {
        asset: string_field(value, "asset")?,
        margin_balance: decimal_field(value, "marginBalance")?,
        equity: decimal_field(value, "equity")?,
        available: decimal_field(value, "available")?,
        initial_margin: decimal_field(value, "initialMargin")?,
        maint_margin: decimal_field(value, "maintMargin")?,
        unrealized_pnl: decimal_field(value, "unrealizedPNL")?,
        adjusted_equity: decimal_field(value, "adjustedEquity")?,
    })
}

fn parse_spot_account(value: &Value) -> AppResult<SpotAccountSnapshot> {
    let balances = value
        .get("balances")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Binance("Binance 现货账户响应缺少 balances 字段".to_string()))?
        .iter()
        .map(parse_spot_asset_balance)
        .collect::<AppResult<Vec<_>>>()?;

    Ok(SpotAccountSnapshot {
        balances,
        can_trade: value
            .get("canTrade")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_spot_asset_balance(value: &Value) -> AppResult<SpotAssetBalance> {
    Ok(SpotAssetBalance {
        asset: string_field(value, "asset")?,
        free: decimal_field(value, "free")?,
        locked: decimal_field(value, "locked")?,
    })
}

fn parse_exchange_info(value: &Value) -> AppResult<Vec<OptionContract>> {
    value
        .get("optionSymbols")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AppError::Binance("Binance exchangeInfo 响应缺少 optionSymbols 字段".to_string())
        })?
        .iter()
        .map(parse_option_contract)
        .collect()
}

fn parse_option_contract(value: &Value) -> AppResult<OptionContract> {
    let expiry_ms = value
        .get("expiryDate")
        .and_then(Value::as_i64)
        .ok_or_else(|| AppError::Binance("期权合约缺少 expiryDate".to_string()))?;
    let expiry = DateTime::<Utc>::from_timestamp_millis(expiry_ms)
        .ok_or_else(|| AppError::Binance("期权合约 expiryDate 无效".to_string()))?
        .date_naive();

    Ok(OptionContract {
        symbol: string_field(value, "symbol")?,
        side: string_field(value, "side")?,
        strike_price: decimal_field(value, "strikePrice")?,
        underlying: string_field(value, "underlying")?,
        unit: decimal_field(value, "unit")?,
        min_qty: decimal_field(value, "minQty")?,
        max_qty: decimal_field(value, "maxQty")?,
        initial_margin: decimal_field(value, "initialMargin")?,
        maintenance_margin: decimal_field(value, "maintenanceMargin")?,
        min_initial_margin: decimal_field(value, "minInitialMargin")?,
        min_maintenance_margin: decimal_field(value, "minMaintenanceMargin")?,
        price_scale: u32_field(value, "priceScale")?,
        quantity_scale: u32_field(value, "quantityScale")?,
        quote_asset: string_field(value, "quoteAsset")?,
        status: string_field(value, "status")?,
        expiry,
    })
}

fn parse_mark_prices(value: &Value) -> AppResult<Vec<OptionMarkPrice>> {
    value
        .as_array()
        .ok_or_else(|| AppError::Binance("Binance mark 响应不是数组".to_string()))?
        .iter()
        .map(parse_mark_price)
        .collect()
}

fn parse_mark_price(value: &Value) -> AppResult<OptionMarkPrice> {
    Ok(OptionMarkPrice {
        symbol: string_field(value, "symbol")?,
        mark_price: decimal_field(value, "markPrice")?,
        mark_iv: decimal_field(value, "markIV")?,
        delta: decimal_field(value, "delta")?,
        high_price_limit: optional_decimal_field(value, "highPriceLimit")?.unwrap_or(Decimal::ZERO),
        low_price_limit: optional_decimal_field(value, "lowPriceLimit")?.unwrap_or(Decimal::ZERO),
    })
}

fn parse_order_ack(value: &Value) -> AppResult<ExchangeOrderAck> {
    let order_id = value
        .get("orderId")
        .map(value_to_string)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| AppError::Binance("Binance 下单响应缺少 orderId".to_string()))?;

    let executed_qty = optional_decimal_field(value, "executedQty")?.unwrap_or(Decimal::ZERO);
    let quantity = optional_decimal_field(value, "quantity")?
        .or(optional_decimal_field(value, "origQty")?)
        .unwrap_or(Decimal::ZERO);
    let price = optional_decimal_field(value, "price")?.unwrap_or(Decimal::ZERO);
    let avg_price = optional_decimal_field(value, "avgPrice")?
        .or_else(|| average_price_from_spot_ack(value, executed_qty))
        .unwrap_or(price);
    let fee = optional_decimal_field(value, "fee")?
        .or_else(|| fee_from_fills(value))
        .unwrap_or(Decimal::ZERO);
    let transact_time = value
        .get("transactTime")
        .and_then(Value::as_i64)
        .and_then(DateTime::<Utc>::from_timestamp_millis);

    Ok(ExchangeOrderAck {
        order_id,
        client_order_id: value
            .get("clientOrderId")
            .map(value_to_string)
            .unwrap_or_default(),
        symbol: string_field(value, "symbol")?,
        status: value
            .get("status")
            .map_or_else(|| "UNKNOWN".to_string(), value_to_string),
        executed_qty,
        avg_price,
        price,
        quantity,
        fee,
        create_time: value
            .get("createTime")
            .and_then(Value::as_i64)
            .and_then(DateTime::<Utc>::from_timestamp_millis)
            .or(transact_time),
        update_time: value
            .get("updateTime")
            .and_then(Value::as_i64)
            .and_then(DateTime::<Utc>::from_timestamp_millis)
            .or(transact_time),
    })
}

fn average_price_from_spot_ack(value: &Value, executed_qty: Decimal) -> Option<Decimal> {
    if executed_qty <= Decimal::ZERO {
        return None;
    }
    let quote_qty = optional_decimal_field(value, "cummulativeQuoteQty")
        .ok()
        .flatten()
        .or_else(|| {
            optional_decimal_field(value, "cumulativeQuoteQty")
                .ok()
                .flatten()
        })?;
    if quote_qty <= Decimal::ZERO {
        None
    } else {
        Some((quote_qty / executed_qty).round_dp(8))
    }
}

fn fee_from_fills(value: &Value) -> Option<Decimal> {
    value
        .get("fills")
        .and_then(Value::as_array)
        .map(|fills| {
            fills
                .iter()
                .filter_map(|fill| optional_decimal_field(fill, "commission").ok().flatten())
                .sum::<Decimal>()
        })
        .filter(|fee| *fee > Decimal::ZERO)
}

fn parse_exercise_records(value: &Value) -> AppResult<Vec<ExerciseRecord>> {
    let rows = value
        .as_array()
        .or_else(|| value.get("data").and_then(Value::as_array))
        .or_else(|| value.get("rows").and_then(Value::as_array))
        .ok_or_else(|| AppError::Binance("Binance 行权记录响应不是数组".to_string()))?;

    rows.iter()
        .map(|row| {
            let settlement_price = optional_decimal_field(row, "settlePrice")?
                .or(optional_decimal_field(row, "settlementPrice")?)
                .or(optional_decimal_field(row, "exercisePrice")?)
                .or(optional_decimal_field(row, "price")?)
                .unwrap_or(Decimal::ZERO);
            Ok(ExerciseRecord {
                symbol: string_field(row, "symbol")?,
                settlement_price,
            })
        })
        .collect()
}

fn string_field(value: &Value, key: &str) -> AppResult<String> {
    value
        .get(key)
        .map(value_to_string)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| AppError::Binance(format!("Binance 响应缺少 {key} 字段")))
}

fn u32_field(value: &Value, key: &str) -> AppResult<u32> {
    let text = value
        .get(key)
        .map(value_to_string)
        .ok_or_else(|| AppError::Binance(format!("Binance 响应缺少 {key} 字段")))?;
    text.parse::<u32>()
        .map_err(|error| AppError::Binance(format!("Binance 字段 {key} 无法解析：{error}")))
}

fn decimal_field(value: &Value, key: &str) -> AppResult<Decimal> {
    optional_decimal_field(value, key)?
        .ok_or_else(|| AppError::Binance(format!("Binance 响应缺少 {key} 字段")))
}

fn optional_decimal_field(value: &Value, key: &str) -> AppResult<Option<Decimal>> {
    value
        .get(key)
        .map(value_to_string)
        .filter(|text| !text.trim().is_empty())
        .map(|text| {
            Decimal::from_str(&text)
                .map_err(|error| AppError::Binance(format!("Binance 字段 {key} 无法解析：{error}")))
        })
        .transpose()
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null | Value::Array(_) | Value::Object(_) => String::new(),
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
