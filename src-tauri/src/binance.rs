//! Binance 实盘接口封装。
//!
//! 这个模块是唯一允许拼接 Binance 请求、签名参数和解析交易所响应的地方。
//! 上层策略与 Tauri command 只接收已经归一化的领域结构，避免前端或策略层接触
//! API Secret、签名 query 或交易所原始响应。

mod http;
mod parsing;

use std::collections::HashMap;
use std::future::Future;
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};
use std::time::Duration;

use chrono::Utc;
use hmac::{Hmac, KeyInit, Mac};
use reqwest::{Client, StatusCode};
use rust_decimal::Decimal;
use serde_json::Value;
use sha2::Sha256;
use tokio::time::sleep;

use self::http::{encode_query, is_timestamp_error, is_transient_query_error, request_error};
use self::parsing::{
    decimal_field, ensure_success_response, parse_account_snapshot, parse_exchange_info,
    parse_exercise_records, parse_mark_prices, parse_option_positions, parse_order_ack,
    parse_simple_earn_flexible_product_page, parse_simple_earn_position_page,
    parse_simple_earn_redemption_records, parse_simple_earn_subscription_records,
    parse_spot_account, parse_spot_price_tickers, parse_universal_transfer_records,
    value_to_string,
};
use crate::domain::{
    AppError, AppResult, ExchangeOrderAck, ExerciseRecord, OptionAccountSnapshot, OptionContract,
    OptionMarkPrice, OptionPosition, SimpleEarnAssetBalance, SimpleEarnFlexibleProduct,
    SimpleEarnRedemptionRecord, SimpleEarnSnapshot, SimpleEarnSubscriptionRecord,
    SpotAccountSnapshot, StrategyDirection, StrategyRequest, UniversalTransferRecord,
};

type HmacSha256 = Hmac<Sha256>;
const QUERY_RETRY_DELAYS: [Duration; 2] = [Duration::from_millis(500), Duration::from_millis(1500)];
const BINANCE_FUNDING_AMOUNT_SCALE: u32 = 8;

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

    /// 查询当前期权持仓，用于接管外部订单和同步已接管记录。
    pub async fn option_positions(&self) -> AppResult<Vec<OptionPosition>> {
        let response = self
            .signed_get(&self.eapi_base, "/eapi/v1/position", Vec::new())
            .await?;
        parse_option_positions(&response)
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
    ) -> AppResult<String> {
        let amount = Self::funding_amount_param("Simple Earn 活期赎回", amount)?;
        let response = self
            .signed_post_value(
                &self.spot_base,
                "/sapi/v1/simple-earn/flexible/redeem",
                vec![
                    ("productId".to_string(), product_id.to_string()),
                    ("redeemAll".to_string(), "false".to_string()),
                    ("amount".to_string(), amount),
                    ("destAccount".to_string(), "SPOT".to_string()),
                ],
            )
            .await?;
        ensure_success_response(&response, "Simple Earn 活期赎回")?;
        response_id(&response, "redeemId", "Simple Earn 活期赎回")
    }

    /// 从现货账户申购 Simple Earn 活期。
    pub async fn subscribe_simple_earn_flexible(
        &self,
        product_id: &str,
        amount: Decimal,
    ) -> AppResult<String> {
        let amount = Self::funding_amount_param("Simple Earn 活期申购", amount)?;
        let response = self
            .signed_post_value(
                &self.spot_base,
                "/sapi/v1/simple-earn/flexible/subscribe",
                vec![
                    ("productId".to_string(), product_id.to_string()),
                    ("amount".to_string(), amount),
                    ("autoSubscribe".to_string(), "true".to_string()),
                    ("sourceAccount".to_string(), "SPOT".to_string()),
                ],
            )
            .await?;
        ensure_success_response(&response, "Simple Earn 活期申购")?;
        response_id(&response, "purchaseId", "Simple Earn 活期申购")
    }

    /// 在现货账户和期权账户之间划转资金。
    pub async fn universal_transfer(
        &self,
        transfer_type: &str,
        asset: &str,
        amount: Decimal,
    ) -> AppResult<String> {
        let amount = Self::funding_amount_param("Binance 账户划转", amount)?;
        let response = self
            .signed_post_value(
                &self.spot_base,
                "/sapi/v1/asset/transfer",
                vec![
                    ("type".to_string(), transfer_type.to_string()),
                    ("asset".to_string(), asset.to_string()),
                    ("amount".to_string(), amount),
                ],
            )
            .await?;
        response_id(&response, "tranId", "Binance 账户划转")
    }

    /// 查询 Binance 账户划转历史，用于资金动作恢复。
    pub async fn universal_transfer_history(
        &self,
        transfer_type: &str,
        start_time_ms: i64,
        end_time_ms: i64,
    ) -> AppResult<Vec<UniversalTransferRecord>> {
        let response = self
            .signed_get(
                &self.spot_base,
                "/sapi/v1/asset/transfer",
                vec![
                    ("type".to_string(), transfer_type.to_string()),
                    ("startTime".to_string(), start_time_ms.to_string()),
                    ("endTime".to_string(), end_time_ms.to_string()),
                    ("current".to_string(), "1".to_string()),
                    ("size".to_string(), "100".to_string()),
                ],
            )
            .await?;
        parse_universal_transfer_records(&response)
    }

    /// 查询 Simple Earn 活期申购历史，用于资金动作恢复。
    pub async fn simple_earn_flexible_subscription_history(
        &self,
        product_id: Option<&str>,
        purchase_id: Option<&str>,
        asset: &str,
        start_time_ms: i64,
        end_time_ms: i64,
    ) -> AppResult<Vec<SimpleEarnSubscriptionRecord>> {
        let mut params = vec![
            ("asset".to_string(), asset.to_string()),
            ("startTime".to_string(), start_time_ms.to_string()),
            ("endTime".to_string(), end_time_ms.to_string()),
            ("current".to_string(), "1".to_string()),
            ("size".to_string(), "100".to_string()),
        ];
        if let Some(product_id) = product_id.filter(|value| !value.trim().is_empty()) {
            params.push(("productId".to_string(), product_id.to_string()));
        }
        if let Some(purchase_id) = purchase_id.filter(|value| !value.trim().is_empty()) {
            params.push(("purchaseId".to_string(), purchase_id.to_string()));
        }
        let response = self
            .signed_get(
                &self.spot_base,
                "/sapi/v1/simple-earn/flexible/history/subscriptionRecord",
                params,
            )
            .await?;
        parse_simple_earn_subscription_records(&response)
    }

    /// 查询 Simple Earn 活期赎回历史，用于资金动作恢复。
    pub async fn simple_earn_flexible_redemption_history(
        &self,
        product_id: Option<&str>,
        redeem_id: Option<&str>,
        asset: &str,
        start_time_ms: i64,
        end_time_ms: i64,
    ) -> AppResult<Vec<SimpleEarnRedemptionRecord>> {
        let mut params = vec![
            ("asset".to_string(), asset.to_string()),
            ("startTime".to_string(), start_time_ms.to_string()),
            ("endTime".to_string(), end_time_ms.to_string()),
            ("current".to_string(), "1".to_string()),
            ("size".to_string(), "100".to_string()),
        ];
        if let Some(product_id) = product_id.filter(|value| !value.trim().is_empty()) {
            params.push(("productId".to_string(), product_id.to_string()));
        }
        if let Some(redeem_id) = redeem_id.filter(|value| !value.trim().is_empty()) {
            params.push(("redeemId".to_string(), redeem_id.to_string()));
        }
        let response = self
            .signed_get(
                &self.spot_base,
                "/sapi/v1/simple-earn/flexible/history/redemptionRecord",
                params,
            )
            .await?;
        parse_simple_earn_redemption_records(&response)
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
        // Simple Earn 活期和定期接口分页结构相近，但金额字段不同；调用方传入候选字段名。
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
        // 签名 query 只交给 reqwest 发送；响应 body 立即解码，不向日志或上层返回原文。
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
        // 公共行情查询没有凭据，但仍统一走 decode_response，避免错误文本夹带完整响应体。
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
        // 只给查询类请求短退避重试；下单和资金 POST 只允许时间戳校正后重试一次。
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
        // Binance 时间戳错误通常来自本机时钟偏移。只缓存偏移量，不修改系统时间。
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
        // POST 可能产生真实订单，因此这里只发送一次；重试策略由 signed_post 外层严格控制。
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
        // 资金 POST 同样不能盲目重放；返回 JSON 后只提取交易所引用或状态字段。
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

    fn funding_amount_param(action: &str, amount: Decimal) -> AppResult<String> {
        // Binance 资金接口对小数位更敏感，调用方必须先按动作方向完成取整。
        let normalized = amount.normalize();
        if normalized <= Decimal::ZERO {
            return Err(AppError::Config(format!(
                "{action} amount 必须大于 0，已阻止发送 Binance 资金请求"
            )));
        }
        if normalized.scale() > BINANCE_FUNDING_AMOUNT_SCALE {
            return Err(AppError::Config(format!(
                "{action} amount 小数位超过 {BINANCE_FUNDING_AMOUNT_SCALE} 位，已阻止发送 Binance 资金请求"
            )));
        }
        Ok(normalized.to_string())
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

fn response_id(value: &Value, key: &str, operation: &str) -> AppResult<String> {
    // 资金接口成功后必须拿到交易所引用，后续恢复只能靠它或脱敏匹配条件查询历史。
    value
        .get(key)
        .map(value_to_string)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| AppError::Binance(format!("{operation}响应缺少 {key} 字段")))
}

/// 根据双币赢方向决定现货补齐方向。
#[allow(dead_code)]
pub const fn spot_rebalance_side(direction: StrategyDirection) -> &'static str {
    match direction {
        StrategyDirection::SellPut => "BUY",
        StrategyDirection::SellCall => "SELL",
    }
}
