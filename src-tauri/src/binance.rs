//! Binance 实盘接口封装。
//!
//! 这个模块是唯一允许拼接 Binance 请求、签名参数和解析交易所响应的地方。
//! 上层策略与 Tauri command 只接收已经归一化的领域结构，避免前端或策略层接触
//! API Secret、签名 query 或交易所原始响应。

use std::collections::HashMap;
use std::future::Future;
use std::str::FromStr;
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};
use std::time::Duration;

use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use reqwest::{Client, StatusCode};
use rust_decimal::Decimal;
use serde_json::Value;
use sha2::Sha256;
use tokio::time::sleep;

use crate::domain::{
    AppError, AppResult, ExchangeOrderAck, ExerciseRecord, OptionAccountSnapshot,
    OptionAssetBalance, OptionContract, OptionMarkPrice, SimpleEarnAssetBalance,
    SimpleEarnFlexibleProduct, SimpleEarnSnapshot, SpotAccountSnapshot, SpotAssetBalance,
    StrategyDirection, StrategyRequest,
};

type HmacSha256 = Hmac<Sha256>;
const QUERY_RETRY_DELAYS: [Duration; 2] = [Duration::from_millis(500), Duration::from_millis(1500)];

/// Binance HTTP 客户端。
///
/// `api_secret` 只保存在 Rust 内存里用于签名，不能进入日志、审计事件或前端状态。
#[derive(Clone)]
pub struct BinanceClient {
    http: Client,
    api_key: String,
    api_secret: String,
    eapi_base: String,
    #[allow(dead_code)]
    spot_base: String,
    timestamp_offset_ms: Arc<AtomicI64>,
}

impl BinanceClient {
    /// 创建带固定超时的 Binance 客户端。
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
            timestamp_offset_ms: Arc::new(AtomicI64::new(0)),
        })
    }

    /// 验证期权账户是否可交易。
    ///
    /// 连接阶段会先调用这个方法，避免 UI 显示已连接但后续真实下单被权限拒绝。
    pub async fn verify_account(&self) -> AppResult<OptionAccountSnapshot> {
        let account = self.option_account().await?;
        if !account.can_trade {
            return Err(AppError::Config(
                "Binance 期权账户当前不可交易，请检查 API 权限和账户状态".to_string(),
            ));
        }
        Ok(account)
    }

    /// 查询期权保证金账户。
    pub async fn option_account(&self) -> AppResult<OptionAccountSnapshot> {
        let response = self
            .signed_get(&self.eapi_base, "/eapi/v1/marginAccount", Vec::new())
            .await?;
        parse_account_snapshot(&response)
    }

    /// 查询期权合约元数据。
    pub async fn exchange_info(&self) -> AppResult<Vec<OptionContract>> {
        let response = self
            .public_get(&self.eapi_base, "/eapi/v1/exchangeInfo")
            .await?;
        parse_exchange_info(&response)
    }

    /// 查询期权 mark price，用于候选估算与价格上下限校验。
    pub async fn option_mark_prices(&self) -> AppResult<Vec<OptionMarkPrice>> {
        let response = self.public_get(&self.eapi_base, "/eapi/v1/mark").await?;
        parse_mark_prices(&response)
    }

    /// 发送真实 Binance 期权卖出限价 IOC 订单。
    ///
    /// 调用方必须已经完成显式确认、合约匹配和风控校验。
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

    /// 按交易所订单号或客户端订单号查询期权订单。
    ///
    /// 恢复流程依赖这个方法重新读取 Binance 事实，不能用本地恢复记录直接造订单。
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

    /// 查询 Binance 现货账户，用于到期 ITM 后补齐前检查可用资产。
    pub async fn spot_account(&self) -> AppResult<SpotAccountSnapshot> {
        let response = self
            .signed_get(&self.spot_base, "/api/v3/account", Vec::new())
            .await?;
        parse_spot_account(&response)
    }

    /// 查询 Binance Simple Earn 活期和定期持仓。
    ///
    /// 现货账户里的 `LD*` 资产只是理财镜像，账户概览应使用这里的真实理财持仓数量。
    pub async fn simple_earn_snapshot(&self) -> AppResult<SimpleEarnSnapshot> {
        let mut balances = self
            .simple_earn_positions(
                "/sapi/v1/simple-earn/flexible/position",
                "活期理财",
                &["totalAmount"],
                true,
            )
            .await?;
        balances.extend(
            self.simple_earn_positions(
                "/sapi/v1/simple-earn/locked/position",
                "定期理财",
                &["amount", "principal", "subscriptionAmount", "totalAmount"],
                false,
            )
            .await?,
        );
        Ok(SimpleEarnSnapshot { balances })
    }

    /// 查询指定资产的 Simple Earn 活期产品。
    pub async fn simple_earn_flexible_products(
        &self,
        asset: &str,
    ) -> AppResult<Vec<SimpleEarnFlexibleProduct>> {
        let response = self
            .signed_get(
                &self.spot_base,
                "/sapi/v1/simple-earn/flexible/list",
                vec![
                    ("asset".to_string(), asset.to_string()),
                    ("current".to_string(), "1".to_string()),
                    ("size".to_string(), "100".to_string()),
                ],
            )
            .await?;
        parse_simple_earn_flexible_product_page(&response)
    }

    /// 从 Simple Earn 活期赎回到现货账户。
    pub async fn redeem_simple_earn_flexible(
        &self,
        product_id: &str,
        amount: Decimal,
    ) -> AppResult<()> {
        let response = self
            .signed_post_value(
                &self.spot_base,
                "/sapi/v1/simple-earn/flexible/redeem",
                vec![
                    ("productId".to_string(), product_id.to_string()),
                    ("redeemAll".to_string(), "false".to_string()),
                    ("amount".to_string(), amount.normalize().to_string()),
                    ("destAccount".to_string(), "SPOT".to_string()),
                ],
            )
            .await?;
        ensure_success_response(&response, "Simple Earn 活期赎回")
    }

    /// 从现货账户申购 Simple Earn 活期。
    pub async fn subscribe_simple_earn_flexible(
        &self,
        product_id: &str,
        amount: Decimal,
    ) -> AppResult<()> {
        let response = self
            .signed_post_value(
                &self.spot_base,
                "/sapi/v1/simple-earn/flexible/subscribe",
                vec![
                    ("productId".to_string(), product_id.to_string()),
                    ("amount".to_string(), amount.normalize().to_string()),
                    ("autoSubscribe".to_string(), "true".to_string()),
                    ("sourceAccount".to_string(), "SPOT".to_string()),
                ],
            )
            .await?;
        ensure_success_response(&response, "Simple Earn 活期申购")
    }

    /// 在现货账户和期权账户之间划转资金。
    pub async fn universal_transfer(
        &self,
        transfer_type: &str,
        asset: &str,
        amount: Decimal,
    ) -> AppResult<()> {
        let response = self
            .signed_post_value(
                &self.spot_base,
                "/sapi/v1/asset/transfer",
                vec![
                    ("type".to_string(), transfer_type.to_string()),
                    ("asset".to_string(), asset.to_string()),
                    ("amount".to_string(), amount.normalize().to_string()),
                ],
            )
            .await?;
        if response.get("tranId").is_some() {
            Ok(())
        } else {
            Err(AppError::Binance("Binance 划转响应缺少 tranId".to_string()))
        }
    }

    /// 查询现货价格。当前 v1 固定用于 `BTCUSDT`。
    pub async fn spot_price(&self, symbol: &str) -> AppResult<Decimal> {
        let response = self
            .public_get(
                &self.spot_base,
                &format!("/api/v3/ticker/price?symbol={symbol}"),
            )
            .await?;
        decimal_field(&response, "price")
    }

    /// 查询全部现货交易对价格。
    ///
    /// 账户估值需要先知道有哪些真实资产，再按 `XXXUSDT` 或 `XXXBTC * BTCUSDT`
    /// 路由估值。一次拉取全部 ticker 可以避免对不存在交易对逐个请求并失败。
    pub async fn spot_price_tickers(&self) -> AppResult<HashMap<String, Decimal>> {
        let response = self
            .public_get(&self.spot_base, "/api/v3/ticker/price")
            .await?;
        parse_spot_price_tickers(&response)
    }

    /// 查询期权行权记录，优先用于确定到期结算参考价。
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

    /// 发送真实 Binance 现货市价补齐订单。
    ///
    /// 目前入口由结算流程控制；保留 `dead_code` 是因为补齐流程可能受功能开关或测试路径影响。
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

    /// 查询现货订单，用于同步和恢复补齐状态。
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

    /// 发送带签名的 GET 请求。
    ///
    /// 返回值仍是 JSON，只在本模块内继续解析，避免把原始响应扩散到其他层。
    pub async fn signed_get(
        &self,
        base: &str,
        path: &str,
        params: Vec<(String, String)>,
    ) -> AppResult<Value> {
        self.query_with_retry(|| self.signed_get_once(base, path, &params))
            .await
    }

    /// 分页查询 Simple Earn 持仓。
    async fn simple_earn_positions(
        &self,
        path: &str,
        source: &str,
        amount_keys: &[&str],
        flexible: bool,
    ) -> AppResult<Vec<SimpleEarnAssetBalance>> {
        let mut balances = Vec::new();
        let mut current = 1_u32;
        let size = 100_u32;
        loop {
            let response = self
                .signed_get(
                    &self.spot_base,
                    path,
                    vec![
                        ("current".to_string(), current.to_string()),
                        ("size".to_string(), size.to_string()),
                    ],
                )
                .await?;
            let page = parse_simple_earn_position_page(&response, source, amount_keys, flexible)?;
            let page_len = page.rows.len();
            let total = page.total;
            balances.extend(page.rows);
            if page_len == 0
                || page_len < size as usize
                || total.is_some_and(|total| balances.len() >= total)
            {
                break;
            }
            current += 1;
        }
        Ok(balances)
    }

    /// 发送不需要签名的行情/合约查询请求。
    async fn public_get(&self, base: &str, path: &str) -> AppResult<Value> {
        self.query_with_retry(|| self.public_get_once(base, path))
            .await
    }

    async fn signed_get_once(
        &self,
        base: &str,
        path: &str,
        params: &[(String, String)],
    ) -> AppResult<Value> {
        let signed = self.signed_params(params.to_vec())?;
        let response = self
            .http
            .get(format!("{base}{path}"))
            .header("X-MBX-APIKEY", &self.api_key)
            .query(&signed)
            .send()
            .await
            .map_err(|error| request_error("GET", path, &error))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| request_error("GET", path, &error))?;
        Self::decode_response(status, &body)
    }

    async fn public_get_once(&self, base: &str, path: &str) -> AppResult<Value> {
        let response = self
            .http
            .get(format!("{base}{path}"))
            .send()
            .await
            .map_err(|error| request_error("GET", path, &error))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| request_error("GET", path, &error))?;
        Self::decode_response(status, &body)
    }

    async fn query_with_retry<T, F, Fut>(&self, mut request: F) -> AppResult<T>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = AppResult<T>>,
    {
        let mut synced_time = false;
        let mut transient_failures = 0_usize;
        loop {
            match request().await {
                Ok(value) => return Ok(value),
                Err(error) if !synced_time && is_timestamp_error(&error) => {
                    self.sync_server_time().await?;
                    synced_time = true;
                }
                Err(error) if is_transient_query_error(&error) => {
                    let Some(delay) = QUERY_RETRY_DELAYS.get(transient_failures).copied() else {
                        return Err(error);
                    };
                    transient_failures += 1;
                    sleep(delay).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn sync_server_time(&self) -> AppResult<()> {
        let response = self
            .public_get_once(&self.spot_base, "/api/v3/time")
            .await?;
        let server_time = response
            .get("serverTime")
            .and_then(Value::as_i64)
            .ok_or_else(|| AppError::Binance("Binance serverTime 字段缺失".to_string()))?;
        let offset = server_time - Utc::now().timestamp_millis();
        self.timestamp_offset_ms.store(offset, Ordering::SeqCst);
        Ok(())
    }

    /// 发送带签名的 POST 请求并归一化订单回报。
    async fn signed_post(
        &self,
        base: &str,
        path: &str,
        params: Vec<(String, String)>,
    ) -> AppResult<ExchangeOrderAck> {
        match self.signed_post_once(base, path, &params).await {
            Ok(value) => Ok(value),
            Err(error) if is_timestamp_error(&error) => {
                self.sync_server_time().await?;
                self.signed_post_once(base, path, &params).await
            }
            Err(error) => Err(error),
        }
    }

    async fn signed_post_once(
        &self,
        base: &str,
        path: &str,
        params: &[(String, String)],
    ) -> AppResult<ExchangeOrderAck> {
        let signed = self.signed_params(params.to_vec())?;
        let response = self
            .http
            .post(format!("{base}{path}"))
            .header("X-MBX-APIKEY", &self.api_key)
            .query(&signed)
            .send()
            .await
            .map_err(|error| request_error("POST", path, &error))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| request_error("POST", path, &error))?;
        let response_json = Self::decode_response(status, &body)?;
        parse_order_ack(&response_json)
    }

    /// 发送带签名的 POST 请求并返回归一化 JSON。
    ///
    /// 非订单类接口只在本模块解析结果，不把原始响应扩散到上层。
    async fn signed_post_value(
        &self,
        base: &str,
        path: &str,
        params: Vec<(String, String)>,
    ) -> AppResult<Value> {
        match self.signed_post_value_once(base, path, &params).await {
            Ok(value) => Ok(value),
            Err(error) if is_timestamp_error(&error) => {
                self.sync_server_time().await?;
                self.signed_post_value_once(base, path, &params).await
            }
            Err(error) => Err(error),
        }
    }

    async fn signed_post_value_once(
        &self,
        base: &str,
        path: &str,
        params: &[(String, String)],
    ) -> AppResult<Value> {
        let signed = self.signed_params(params.to_vec())?;
        let response = self
            .http
            .post(format!("{base}{path}"))
            .header("X-MBX-APIKEY", &self.api_key)
            .query(&signed)
            .send()
            .await
            .map_err(|error| request_error("POST", path, &error))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| request_error("POST", path, &error))?;
        Self::decode_response(status, &body)
    }

    /// 生成 Binance 签名参数。
    ///
    /// 签名 query 只在内存中存在；错误消息不能包含 query 或签名值。
    fn signed_params(&self, mut params: Vec<(String, String)>) -> AppResult<Vec<(String, String)>> {
        params.push((
            "timestamp".to_string(),
            (Utc::now().timestamp_millis() + self.timestamp_offset_ms.load(Ordering::SeqCst))
                .to_string(),
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

    /// 解码 Binance HTTP 响应。
    ///
    /// 非 2xx 响应只返回 code/msg 摘要，不把完整响应体带到错误文本里。
    fn decode_response(status: StatusCode, body: &str) -> AppResult<Value> {
        if !status.is_success() {
            if let Ok(value) = serde_json::from_str::<Value>(body) {
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
        serde_json::from_str::<Value>(body).map_err(|error| AppError::Binance(error.to_string()))
    }
}

fn request_error(method: &str, path: &str, error: &reqwest::Error) -> AppError {
    let reason = if error.is_timeout() {
        "请求超时"
    } else if error.is_connect() {
        "连接失败"
    } else if error.is_decode() {
        "响应解码失败"
    } else if error.is_body() {
        "响应体读取失败"
    } else if error.is_request() {
        "请求构造失败"
    } else {
        "网络请求失败"
    };
    AppError::Binance(format!("Binance {method} {path} {reason}"))
}

fn is_timestamp_error(error: &AppError) -> bool {
    matches!(
        error,
        AppError::Binance(message)
            if message.contains("code -1021")
                || message.contains("timestamp")
                || message.contains("Timestamp")
    )
}

fn is_transient_query_error(error: &AppError) -> bool {
    matches!(
        error,
        AppError::Binance(message)
            if message.contains("请求超时")
                || message.contains("连接失败")
                || message.contains("响应体读取失败")
                || message.contains("网络请求失败")
    )
}

/// 解析期权账户快照，字段名保持贴近 Binance EAPI 响应。
fn parse_account_snapshot(value: &Value) -> AppResult<OptionAccountSnapshot> {
    let assets = value
        .get("asset")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Binance("Binance 账户响应缺少 asset 字段".to_string()))?
        .iter()
        .map(parse_asset_balance)
        .collect::<AppResult<Vec<_>>>()?;

    Ok(OptionAccountSnapshot {
        assets,
        can_trade: value
            .get("canTrade")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// 解析单个期权保证金资产。
fn parse_asset_balance(value: &Value) -> AppResult<OptionAssetBalance> {
    Ok(OptionAssetBalance {
        asset: string_field(value, "asset")?,
        margin_balance: decimal_field(value, "marginBalance")?,
        equity: decimal_field(value, "equity")?,
        available: decimal_field(value, "available")?,
        initial_margin: decimal_field(value, "initialMargin")?,
        maint_margin: decimal_field(value, "maintMargin")?,
        adjusted_equity: decimal_field(value, "adjustedEquity")?,
    })
}

/// 解析现货账户快照。
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

/// 解析单个现货资产余额。
fn parse_spot_asset_balance(value: &Value) -> AppResult<SpotAssetBalance> {
    Ok(SpotAssetBalance {
        asset: string_field(value, "asset")?,
        free: decimal_field(value, "free")?,
        locked: decimal_field(value, "locked")?,
    })
}

struct SimpleEarnPositionPage {
    rows: Vec<SimpleEarnAssetBalance>,
    total: Option<usize>,
}

/// 解析 Simple Earn 活期/定期持仓分页。
fn parse_simple_earn_position_page(
    value: &Value,
    source: &str,
    amount_keys: &[&str],
    flexible: bool,
) -> AppResult<SimpleEarnPositionPage> {
    let rows = value
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Binance("Binance Simple Earn 响应缺少 rows 字段".to_string()))?
        .iter()
        .map(|row| parse_simple_earn_position(row, source, amount_keys, flexible))
        .collect::<AppResult<Vec<_>>>()?;
    let total = value
        .get("total")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok());
    Ok(SimpleEarnPositionPage { rows, total })
}

/// 解析单条 Simple Earn 持仓。
fn parse_simple_earn_position(
    value: &Value,
    source: &str,
    amount_keys: &[&str],
    flexible: bool,
) -> AppResult<SimpleEarnAssetBalance> {
    let amount = decimal_from_any_field(value, amount_keys)?
        .ok_or_else(|| AppError::Binance("Binance Simple Earn 持仓响应缺少数量字段".to_string()))?;
    Ok(SimpleEarnAssetBalance {
        asset: string_field(value, "asset")?,
        source: source.to_string(),
        product_id: optional_string_field(value, "productId").unwrap_or_default(),
        amount,
        flexible,
        can_redeem: value
            .get("canRedeem")
            .and_then(Value::as_bool)
            .unwrap_or(flexible),
    })
}

/// 解析 Simple Earn 活期产品列表。
fn parse_simple_earn_flexible_product_page(
    value: &Value,
) -> AppResult<Vec<SimpleEarnFlexibleProduct>> {
    value
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AppError::Binance("Binance Simple Earn 产品列表响应缺少 rows 字段".to_string())
        })?
        .iter()
        .map(parse_simple_earn_flexible_product)
        .collect()
}

fn parse_simple_earn_flexible_product(value: &Value) -> AppResult<SimpleEarnFlexibleProduct> {
    Ok(SimpleEarnFlexibleProduct {
        asset: string_field(value, "asset")?,
        product_id: string_field(value, "productId")?,
        can_purchase: value
            .get("canPurchase")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_sold_out: value
            .get("isSoldOut")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        status: value.get("status").map(value_to_string).unwrap_or_default(),
        min_purchase_amount: optional_decimal_field(value, "minPurchaseAmount")?
            .unwrap_or(Decimal::ZERO),
    })
}

fn ensure_success_response(value: &Value, operation: &str) -> AppResult<()> {
    match value.get("success").and_then(Value::as_bool) {
        Some(true) => Ok(()),
        Some(false) => Err(AppError::Binance(format!("{operation}未成功"))),
        None => Err(AppError::Binance(format!(
            "{operation}响应缺少 success 字段"
        ))),
    }
}

fn decimal_from_any_field(value: &Value, keys: &[&str]) -> AppResult<Option<Decimal>> {
    for key in keys {
        if let Some(amount) = optional_decimal_field(value, key)? {
            return Ok(Some(amount));
        }
    }
    Ok(None)
}

/// 解析现货交易对价格列表。
fn parse_spot_price_tickers(value: &Value) -> AppResult<HashMap<String, Decimal>> {
    let rows = value
        .as_array()
        .ok_or_else(|| AppError::Binance("Binance ticker price 响应不是数组".to_string()))?;
    let mut prices = HashMap::with_capacity(rows.len());
    for row in rows {
        prices.insert(string_field(row, "symbol")?, decimal_field(row, "price")?);
    }
    Ok(prices)
}

/// 解析期权合约列表。
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

/// 解析单个期权合约，包含精度和保证金参数。
fn parse_option_contract(value: &Value) -> AppResult<OptionContract> {
    let expiry_ms = value
        .get("expiryDate")
        .and_then(Value::as_i64)
        .ok_or_else(|| AppError::Binance("期权合约缺少 expiryDate".to_string()))?;
    let expiry_at = DateTime::<Utc>::from_timestamp_millis(expiry_ms)
        .ok_or_else(|| AppError::Binance("期权合约 expiryDate 无效".to_string()))?;
    let expiry = expiry_at.date_naive();
    let price_filter = exchange_info_filter(value, "PRICE_FILTER")?;
    let lot_filter = exchange_info_filter(value, "LOT_SIZE")?;

    Ok(OptionContract {
        symbol: string_field(value, "symbol")?,
        side: string_field(value, "side")?,
        strike_price: decimal_field(value, "strikePrice")?,
        underlying: string_field(value, "underlying")?,
        unit: decimal_field(value, "unit")?,
        min_qty: decimal_field(value, "minQty")?,
        max_qty: decimal_field(value, "maxQty")?,
        quantity_step_size: decimal_field(lot_filter, "stepSize")?,
        initial_margin: decimal_field(value, "initialMargin")?,
        maintenance_margin: decimal_field(value, "maintenanceMargin")?,
        min_initial_margin: decimal_field(value, "minInitialMargin")?,
        min_maintenance_margin: decimal_field(value, "minMaintenanceMargin")?,
        min_price: decimal_field(price_filter, "minPrice")?,
        max_price: decimal_field(price_filter, "maxPrice")?,
        price_tick_size: decimal_field(price_filter, "tickSize")?,
        price_scale: u32_field(value, "priceScale")?,
        quantity_scale: u32_field(value, "quantityScale")?,
        quote_asset: string_field(value, "quoteAsset")?,
        status: string_field(value, "status")?,
        expiry,
        expiry_at,
    })
}

fn exchange_info_filter<'a>(value: &'a Value, filter_type: &str) -> AppResult<&'a Value> {
    value
        .get("filters")
        .and_then(Value::as_array)
        .and_then(|filters| {
            filters.iter().find(|filter| {
                filter
                    .get("filterType")
                    .map(value_to_string)
                    .is_some_and(|current| current == filter_type)
            })
        })
        .ok_or_else(|| AppError::Binance(format!("Binance 期权合约缺少 {filter_type} 过滤器")))
}

/// 解析 mark price 列表。
fn parse_mark_prices(value: &Value) -> AppResult<Vec<OptionMarkPrice>> {
    value
        .as_array()
        .ok_or_else(|| AppError::Binance("Binance mark 响应不是数组".to_string()))?
        .iter()
        .map(parse_mark_price)
        .collect()
}

/// 解析单条 mark price。
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

/// 归一化期权/现货订单响应。
///
/// Binance 不同接口的成交数量、均价和手续费字段不完全一致，这里集中归一化。
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

/// 从现货订单累计成交额反推均价。
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

/// 从现货 fills 汇总手续费。
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

/// 解析行权记录。
///
/// Binance 历史接口可能返回数组、`data` 或 `rows` 包装，这里只提取结算价相关字段。
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

/// 读取必填字符串字段，空字符串视为缺失。
fn string_field(value: &Value, key: &str) -> AppResult<String> {
    value
        .get(key)
        .map(value_to_string)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| AppError::Binance(format!("Binance 响应缺少 {key} 字段")))
}

fn optional_string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .map(value_to_string)
        .filter(|text| !text.trim().is_empty())
}

/// 读取必填 u32 字段。
fn u32_field(value: &Value, key: &str) -> AppResult<u32> {
    let text = value
        .get(key)
        .map(value_to_string)
        .ok_or_else(|| AppError::Binance(format!("Binance 响应缺少 {key} 字段")))?;
    text.parse::<u32>()
        .map_err(|error| AppError::Binance(format!("Binance 字段 {key} 无法解析：{error}")))
}

/// 读取必填 Decimal 字段。
fn decimal_field(value: &Value, key: &str) -> AppResult<Decimal> {
    optional_decimal_field(value, key)?
        .ok_or_else(|| AppError::Binance(format!("Binance 响应缺少 {key} 字段")))
}

/// 读取可选 Decimal 字段。
///
/// Binance 数字字段可能以字符串或数字形式返回，统一先转字符串再解析成 Decimal。
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

/// 把 JSON 标量转为字符串；数组和对象不参与金融字段解析。
fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null | Value::Array(_) | Value::Object(_) => String::new(),
    }
}

/// 构造 Binance 签名原文。
fn encode_query(params: &[(String, String)]) -> String {
    params
        .iter()
        .map(|(key, value)| format!("{key}={}", urlencoding::encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// 根据双币赢方向决定现货补齐方向。
#[allow(dead_code)]
pub const fn spot_rebalance_side(direction: StrategyDirection) -> &'static str {
    match direction {
        StrategyDirection::SellPut => "BUY",
        StrategyDirection::SellCall => "SELL",
    }
}
