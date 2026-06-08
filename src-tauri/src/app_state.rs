//! 应用状态编排层。
//!
//! 这个模块连接 Tauri command、Binance 客户端、策略计算和 `SQLite` 持久化。
//! 它负责实盘连接、快照聚合、下单门禁、订单恢复、到期结算、现货补齐和后台轮询。
//! 业务计算放在 `strategy.rs`，交易所请求放在 `binance.rs`，凭据读写放在 `secrets.rs`。

mod adoption;
mod funding;
mod orders;
mod progress;
mod runtime;
mod settlement;
mod settlement_funding;
mod settlement_recovery;
mod validation;

use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration as StdDuration,
};

use chrono::{Duration, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tauri::AppHandle;
use tokio::sync::{Mutex as AsyncMutex, RwLock};
use uuid::Uuid;

use self::adoption::{
    adoptable_position_view, ensure_position_can_be_adopted, is_short_position,
    normalized_unique_symbols,
};
use self::funding::{
    option_funding_shortfall, option_idle_usdt_sweep_amount, round_down_funding_amount,
    round_up_funding_amount, spot_asset_free,
};
use self::orders::{
    apply_option_ack, apply_option_position, apply_settlement_progress, apply_spot_ack,
    combined_balances, is_spot_attempt, is_unfilled_terminal_option_ack,
    request_from_option_attempt,
};
use self::progress::OrderProgressEmitter;
use self::runtime::{
    client_from_runtime, join_binance_request, make_client_order_id, make_spot_client_order_id,
    spawn_binance_request, verify_binance_access,
};
use self::validation::{
    enforce_risk_rules, has_exchange_order_ref, is_in_the_money, is_trackable_order,
    is_uncertain_submit_error, resolve_contract, should_sync_option_order, validate_contract_order,
    validate_contract_terms, validate_request, validate_spot_rebalance,
};
use crate::binance::BinanceClient;
use crate::domain::{
    AdoptOptionPositionsRequest, AdoptableOptionPositionView, AppError, AppResult, AppSettings,
    AppSnapshot, CandidateScanResult, ConnectBinanceRequest, FundingAttempt, FundingAttemptKind,
    FundingAttemptManualResolution, FundingAttemptStatus, FundingAttemptView, LogLevel,
    LogSnapshot, OptionAccountSnapshot, OptionContract, OptionMarkPrice, OrderAttempt,
    OrderAttemptStatus, OrderAttemptView, OrderProgressStage, OrderProgressStatus,
    ResolveFundingAttemptRequest, SettlementBatch, SettlementStage, SimpleEarnRedemptionRecord,
    SimpleEarnSnapshot, SimpleEarnSubscriptionRecord, SpotAccountSnapshot, StrategyCandidate,
    StrategyOrder, StrategyRequest, StrategyStatus, UniversalTransferRecord,
};
use crate::logs::LogStore;
use crate::persistence::Persistence;
use crate::secrets;
use crate::strategy;

/// 一次实盘同步需要的账户侧快照。
///
/// 三类账户和现货价格一起读取，是为了让快照、收益估值和资金准备使用同一批事实。
struct LiveAccounts {
    option: OptionAccountSnapshot,
    spot: SpotAccountSnapshot,
    earn: SimpleEarnSnapshot,
    spot_prices: HashMap<String, Decimal>,
}

/// 发送期权订单前必须重新读取的实盘输入。
///
/// 下单不能复用候选扫描缓存，因为合约过滤器、mark greeks、盘口和账户保证金都可能已经变化。
struct OrderLiveInputs {
    account: OptionAccountSnapshot,
    contracts: Vec<OptionContract>,
    marks: Vec<OptionMarkPrice>,
}

/// 待发送的资金动作请求。
///
/// 真实请求发送前会先转成 `funding_attempts` 记录；字段只保存恢复所需的脱敏事实。
#[derive(Clone, Debug)]
struct FundingRequest {
    chain_id: String,
    kind: FundingAttemptKind,
    purpose: String,
    asset: String,
    amount: Decimal,
    transfer_type: Option<String>,
    product_id: Option<String>,
}

/// 查询 Binance 历史后得到的资金动作事实。
///
/// `NotFound` 和 `Ambiguous` 都不能直接重发真实资金动作，必须继续查询或进入阻断。
enum FundingHistoryOutcome {
    Confirmed(String, String),
    Pending(Option<String>, String),
    Failed(Option<String>, String),
    NotFound,
    Ambiguous,
}

/// 期权账户闲置 USDT 归集任务的执行结果。
enum SweepExecutionOutcome {
    Completed(String),
    Skipped(String),
    Waiting(String),
}

const FUNDING_HISTORY_LOOKBACK_MINUTES: i64 = 5;
const FUNDING_HISTORY_LOOKAHEAD_DAYS: i64 = 30;
const FUNDING_HISTORY_EXPANDED_WINDOW_MINUTES: i64 = 30;
const FUNDING_HISTORY_WIDE_WINDOW_HOURS: i64 = 2;
const FUNDING_QUERY_BACKOFF_MINUTES: [i64; 5] = [1, 3, 10, 30, 120];
const SETTLEMENT_SWEEP_DELAY_MINUTES: i64 = 5;
const OPTION_IDLE_USDT_SWEEP_MIN_AMOUNT: Decimal = dec!(10);
const OPTION_ACCOUNT_FUNDING_REFRESH_ATTEMPTS: usize = 6;
const OPTION_ACCOUNT_FUNDING_REFRESH_DELAY_MS: u64 = 700;
const SPOT_BALANCE_CONFIRM_ATTEMPTS: usize = 6;
const SPOT_BALANCE_CONFIRM_DELAY_MS: u64 = 1_000;

fn funding_backoff_after_count(count: u32) -> Duration {
    let index = usize::try_from(count.saturating_sub(1))
        .unwrap_or(usize::MAX)
        .min(FUNDING_QUERY_BACKOFF_MINUTES.len().saturating_sub(1));
    Duration::minutes(FUNDING_QUERY_BACKOFF_MINUTES[index])
}

fn funding_history_effective_query_limit() -> u32 {
    u32::try_from(FUNDING_QUERY_BACKOFF_MINUTES.len()).unwrap_or(u32::MAX)
}

fn funding_history_window(
    attempt: &FundingAttempt,
    now: chrono::DateTime<Utc>,
) -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>) {
    // 历史查询窗口从本地写入恢复记录的时间向两侧扩展。有效查询次数越多，说明
    // Binance 历史落库可能越慢，窗口随之变宽，但仍限制在交易所可接受的时间范围内。
    let (lookback, lookahead) = match attempt.effective_query_count {
        0 | 1 => (
            Duration::minutes(FUNDING_HISTORY_LOOKBACK_MINUTES),
            Duration::minutes(FUNDING_HISTORY_LOOKBACK_MINUTES),
        ),
        2 | 3 => (
            Duration::minutes(FUNDING_HISTORY_EXPANDED_WINDOW_MINUTES),
            Duration::minutes(FUNDING_HISTORY_EXPANDED_WINDOW_MINUTES),
        ),
        _ => (
            Duration::hours(FUNDING_HISTORY_WIDE_WINDOW_HOURS),
            Duration::hours(FUNDING_HISTORY_WIDE_WINDOW_HOURS),
        ),
    };
    let start = attempt.created_at - lookback;
    let max_end = attempt.created_at + Duration::days(FUNDING_HISTORY_LOOKAHEAD_DAYS);
    let near_now = now + lookahead;
    let mut end = if near_now < max_end {
        near_now
    } else {
        max_end
    };
    if end <= start {
        end = start + Duration::minutes(1);
    }
    (start, end)
}

fn classify_funding_error(error: &AppError) -> &'static str {
    match error {
        AppError::Config(_) => "config",
        AppError::RiskRejected(_) => "risk_rejected",
        AppError::Storage(_) => "storage",
        AppError::Binance(message) if message.contains("请求超时") => "timeout",
        AppError::Binance(message) if message.contains("连接失败") => "connect_failed",
        AppError::Binance(message) if message.contains("响应体读取失败") => {
            "body_read_failed"
        }
        AppError::Binance(message) if message.contains("网络请求失败") => "network_failed",
        AppError::Binance(message) if message.contains("响应解码失败") => "decode_failed",
        AppError::Binance(message) if message.contains("响应缺少 rows 字段") => {
            "missing_rows"
        }
        AppError::Binance(message) if message.contains("HTTP 429") => "rate_limited",
        AppError::Binance(message)
            if message.contains("HTTP 500")
                || message.contains("HTTP 502")
                || message.contains("HTTP 503")
                || message.contains("HTTP 504") =>
        {
            "binance_server_error"
        }
        AppError::Binance(message) if message.contains("响应缺少") => "missing_response_field",
        AppError::Binance(_) => "binance",
    }
}

fn is_transient_binance_error(error: &AppError) -> bool {
    matches!(
        error,
        AppError::Binance(message)
            if message.contains("请求超时")
                || message.contains("连接失败")
                || message.contains("响应体读取失败")
                || message.contains("网络请求失败")
                || message.contains("响应解码失败")
                || message.contains("响应缺少 rows 字段")
                || message.contains("HTTP 429")
                || message.contains("HTTP 500")
                || message.contains("HTTP 502")
                || message.contains("HTTP 503")
                || message.contains("HTTP 504")
    )
}

/// 用交易所引用或脱敏匹配条件确认 Universal Transfer 事实。
fn match_universal_transfer_record(
    attempt: &FundingAttempt,
    records: &[UniversalTransferRecord],
) -> FundingHistoryOutcome {
    if let Some(exchange_ref) = attempt.exchange_ref.as_deref() {
        return records
            .iter()
            .find(|record| record.tran_id == exchange_ref)
            .map_or(FundingHistoryOutcome::NotFound, universal_transfer_outcome);
    }

    let matches = records
        .iter()
        .filter(|record| {
            attempt
                .transfer_type
                .as_ref()
                .is_some_and(|transfer_type| record.transfer_type == *transfer_type)
                && record.asset == attempt.asset
                && record.amount == attempt.amount
        })
        .collect::<Vec<_>>();
    unique_history_match(&matches, universal_transfer_outcome)
}

/// 把 Binance Universal Transfer 历史状态归一化为本地恢复结果。
fn universal_transfer_outcome(record: &UniversalTransferRecord) -> FundingHistoryOutcome {
    match record.status.as_str() {
        "CONFIRMED" => {
            FundingHistoryOutcome::Confirmed(record.tran_id.clone(), record.status.clone())
        }
        "FAILED" => {
            FundingHistoryOutcome::Failed(Some(record.tran_id.clone()), record.status.clone())
        }
        _ => FundingHistoryOutcome::Pending(Some(record.tran_id.clone()), record.status.clone()),
    }
}

/// 用交易所引用或脱敏匹配条件确认 Simple Earn 申购事实。
fn match_simple_earn_subscription_record(
    attempt: &FundingAttempt,
    records: &[SimpleEarnSubscriptionRecord],
) -> FundingHistoryOutcome {
    if let Some(exchange_ref) = attempt.exchange_ref.as_deref() {
        return records
            .iter()
            .find(|record| record.purchase_id == exchange_ref)
            .map_or(
                FundingHistoryOutcome::NotFound,
                simple_earn_subscription_outcome,
            );
    }

    let matches = records
        .iter()
        .filter(|record| {
            attempt
                .product_id
                .as_ref()
                .is_none_or(|product_id| record.product_id == *product_id)
                && record.asset == attempt.asset
                && record.amount == attempt.amount
        })
        .collect::<Vec<_>>();
    unique_history_match(&matches, simple_earn_subscription_outcome)
}

/// 把 Simple Earn 申购历史状态归一化为本地恢复结果。
fn simple_earn_subscription_outcome(
    record: &SimpleEarnSubscriptionRecord,
) -> FundingHistoryOutcome {
    match record.status.as_str() {
        "SUCCESS" => {
            FundingHistoryOutcome::Confirmed(record.purchase_id.clone(), record.status.clone())
        }
        "FAILED" => {
            FundingHistoryOutcome::Failed(Some(record.purchase_id.clone()), record.status.clone())
        }
        _ => {
            FundingHistoryOutcome::Pending(Some(record.purchase_id.clone()), record.status.clone())
        }
    }
}

/// 用交易所引用或脱敏匹配条件确认 Simple Earn 赎回事实。
fn match_simple_earn_redemption_record(
    attempt: &FundingAttempt,
    records: &[SimpleEarnRedemptionRecord],
) -> FundingHistoryOutcome {
    if let Some(exchange_ref) = attempt.exchange_ref.as_deref() {
        return records
            .iter()
            .find(|record| record.redeem_id == exchange_ref)
            .map_or(
                FundingHistoryOutcome::NotFound,
                simple_earn_redemption_outcome,
            );
    }

    let matches = records
        .iter()
        .filter(|record| {
            attempt
                .product_id
                .as_ref()
                .is_none_or(|product_id| record.product_id == *product_id)
                && record.asset == attempt.asset
                && record.amount == attempt.amount
        })
        .collect::<Vec<_>>();
    unique_history_match(&matches, simple_earn_redemption_outcome)
}

/// 把 Simple Earn 赎回历史状态归一化为本地恢复结果。
fn simple_earn_redemption_outcome(record: &SimpleEarnRedemptionRecord) -> FundingHistoryOutcome {
    match record.status.as_str() {
        "PAID" => FundingHistoryOutcome::Confirmed(record.redeem_id.clone(), record.status.clone()),
        "FAILED" => {
            FundingHistoryOutcome::Failed(Some(record.redeem_id.clone()), record.status.clone())
        }
        _ => FundingHistoryOutcome::Pending(Some(record.redeem_id.clone()), record.status.clone()),
    }
}

/// 只有唯一匹配才允许继续推进资金恢复；多条候选必须阻断等待人工核对。
fn unique_history_match<T>(
    matches: &[&T],
    outcome: impl Fn(&T) -> FundingHistoryOutcome,
) -> FundingHistoryOutcome {
    match matches {
        [] => FundingHistoryOutcome::NotFound,
        [record] => outcome(*record),
        _ => FundingHistoryOutcome::Ambiguous,
    }
}

/// 判断资金请求是否可能已经到达 Binance，需要走历史查询确认。
fn is_uncertain_funding_error(error: &AppError) -> bool {
    is_uncertain_submit_error(error)
        || matches!(
            error,
            AppError::Binance(message) if message.contains("响应缺少")
        )
}

/// 运行期连接配置。
///
/// API Key 和 Secret 只允许留在内存中用于重建 Binance 客户端，不能写入日志、
/// `SQLite` 或前端快照；前端只能通过 `settings()` 获取脱敏状态。
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    /// 当前进程是否已经验证过 Binance 连接和交易权限。
    pub connected: bool,
    /// 内存中的 API Key，用于本进程内重建客户端；保存凭据仍走系统 keyring。
    pub api_key: Option<String>,
    /// 内存中的 API Secret，只用于签名请求，不得进入任何持久化或展示层。
    pub api_secret: Option<String>,
    /// 系统 keyring 是否存在完整凭据。
    pub saved_credentials: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            connected: false,
            api_key: None,
            api_secret: None,
            saved_credentials: secrets::has_binance_credentials(),
        }
    }
}

impl RuntimeConfig {
    /// 转成前端可见设置，过滤掉明文 API Key 和 Secret。
    pub fn settings(&self) -> AppSettings {
        AppSettings {
            connected: self.connected,
            saved_credentials: self.saved_credentials,
        }
    }
}

/// 运行期共享状态。
///
/// 多数数据放在 `RwLock` 中便于 Tauri command 并发读取；SQLite 连接本身不可并发共享，
/// 因此用 `Mutex<Persistence>` 串行化本地持久化操作。
#[derive(Clone)]
pub struct AppState {
    runtime: Arc<RwLock<RuntimeConfig>>,
    client: Arc<RwLock<Option<BinanceClient>>>,
    candidates: Arc<RwLock<Vec<StrategyCandidate>>>,
    orders: Arc<RwLock<Vec<StrategyOrder>>>,
    persistence: Arc<Mutex<Persistence>>,
    logs: Arc<LogStore>,
    worker_started: Arc<AtomicBool>,
    maintenance_lock: Arc<AsyncMutex<()>>,
    trade_lock: Arc<AsyncMutex<()>>,
}

impl AppState {
    /// 从 Tauri 应用数据目录创建状态。
    pub fn new_in(data_dir: impl AsRef<Path>) -> AppResult<Self> {
        let data_dir = data_dir.as_ref();
        Self::from_persistence(
            Persistence::open_app_data(data_dir)?,
            LogStore::open_app_data(data_dir)?,
        )
    }

    /// 从持久化层恢复状态。
    ///
    /// 启动时只加载仍可跟踪的订单，包括没有原始订单号的接管订单。
    fn from_persistence(persistence: Persistence, logs: LogStore) -> AppResult<Self> {
        let orders = persistence
            .list_orders()?
            .into_iter()
            .filter(is_trackable_order)
            .collect();
        let state = Self {
            runtime: Arc::new(RwLock::new(RuntimeConfig::default())),
            client: Arc::new(RwLock::new(None)),
            candidates: Arc::new(RwLock::new(Vec::new())),
            orders: Arc::new(RwLock::new(orders)),
            persistence: Arc::new(Mutex::new(persistence)),
            logs: Arc::new(logs),
            worker_started: Arc::new(AtomicBool::new(false)),
            maintenance_lock: Arc::new(AsyncMutex::new(())),
            trade_lock: Arc::new(AsyncMutex::new(())),
        };
        state.record_log("app.started", "应用已启动，等待 Binance API 连接")?;
        Ok(state)
    }

    /// 聚合前端所需完整快照。
    ///
    /// 已连接时读取 Binance 账户和现货价格；未连接时仅根据本地记录生成快照。
    pub async fn snapshot(&self) -> AppResult<AppSnapshot> {
        let settings = self.runtime.read().await.settings();
        let candidates = self.candidates.read().await.clone();
        let mut orders = self.orders.read().await.clone();
        apply_settlement_progress(&mut orders, &self.settlement_batches()?);
        let accounts = self.live_accounts_if_connected().await?;
        let (order_attempts, funding_attempts) = {
            let persistence = self
                .persistence
                .lock()
                .map_err(|error| AppError::Storage(error.to_string()))?;
            let attempts = persistence
                .list_order_attempts(20)?
                .into_iter()
                .map(OrderAttemptView::from)
                .collect::<Vec<_>>();
            let funding_attempts = persistence
                .list_funding_attempts(20)?
                .into_iter()
                .map(FundingAttemptView::from)
                .collect::<Vec<_>>();
            (attempts, funding_attempts)
        };
        let balances = accounts.as_ref().map_or_else(Vec::new, |accounts| {
            combined_balances(
                &accounts.option,
                &accounts.spot,
                &accounts.earn,
                &accounts.spot_prices,
            )
        });
        let spot_price = accounts
            .as_ref()
            .and_then(|accounts| accounts.spot_prices.get("BTCUSDT").copied())
            .filter(|price| *price > Decimal::ZERO);

        Ok(AppSnapshot {
            balances,
            candidates,
            pnl: strategy::pnl_report_from_orders_with_spot(&orders, spot_price),
            settings,
            orders,
            order_attempts,
            funding_attempts,
            settlement_recoveries: self.settlement_recoveries()?,
        })
    }

    /// 使用用户输入凭据连接 Binance，并保存到系统 keyring。
    ///
    /// 连接阶段同时验证期权和现货账户权限，因为到期 ITM 后需要真实现货补齐能力。
    /// 连接只验证账户并缓存客户端；独立的后台维护按既有订单事实推进到期处理。
    pub async fn connect_binance(&self, request: ConnectBinanceRequest) -> AppResult<AppSettings> {
        let result = self.connect_binance_inner(request).await;
        if let Err(error) = &result {
            let _ = self.record_warning("account.connect_failed", &error.to_string());
        }
        result
    }

    async fn connect_binance_inner(
        &self,
        request: ConnectBinanceRequest,
    ) -> AppResult<AppSettings> {
        let _maintenance_guard = self.maintenance_lock.lock().await;
        let _trade_guard = self.trade_lock.lock().await;
        if request.api_key.trim().is_empty() || request.api_secret.trim().is_empty() {
            return Err(AppError::Config("API Key 和 Secret 不能为空".to_string()));
        }

        let api_key = request.api_key.trim().to_string();
        let api_secret = request.api_secret;
        let client = BinanceClient::new(api_key.clone(), api_secret.clone())?;
        verify_binance_access(&client).await?;
        secrets::save_binance_credentials(&api_key, &api_secret)?;

        let mut runtime = self.runtime.write().await;
        runtime.connected = true;
        runtime.api_key = Some(api_key);
        runtime.api_secret = Some(api_secret);
        runtime.saved_credentials = true;
        let settings = runtime.settings();
        drop(runtime);
        {
            let mut cached_client = self.client.write().await;
            *cached_client = Some(client);
        }

        self.record_log("account.connected", "已连接 Binance 账户")?;
        Ok(settings)
    }

    /// 使用系统 keyring 中的凭据连接 Binance。
    ///
    /// 凭据只恢复连接；此前已确认订单的结算由应用后台定时推进，无需再次授权。
    pub async fn connect_saved_binance(&self) -> AppResult<AppSettings> {
        let result = self.connect_saved_binance_inner().await;
        if let Err(error) = &result {
            let _ = self.record_warning("account.connect_saved_failed", &error.to_string());
        }
        result
    }

    async fn connect_saved_binance_inner(&self) -> AppResult<AppSettings> {
        let _maintenance_guard = self.maintenance_lock.lock().await;
        let _trade_guard = self.trade_lock.lock().await;
        let Some((api_key, api_secret)) = secrets::load_binance_credentials()? else {
            return Err(AppError::Config(
                "系统 keyring 中没有已保存的 Binance API 凭据".to_string(),
            ));
        };
        let client = BinanceClient::new(api_key.clone(), api_secret.clone())?;
        verify_binance_access(&client).await?;

        let mut runtime = self.runtime.write().await;
        runtime.connected = true;
        runtime.api_key = Some(api_key);
        runtime.api_secret = Some(api_secret);
        runtime.saved_credentials = true;
        let settings = runtime.settings();
        drop(runtime);
        {
            let mut cached_client = self.client.write().await;
            *cached_client = Some(client);
        }

        self.record_log(
            "account.connected.saved",
            "已使用系统 keyring 凭据连接 Binance 账户",
        )?;
        Ok(settings)
    }

    /// 删除系统 keyring 凭据并清空运行期连接状态。
    pub async fn forget_binance_credentials(&self) -> AppResult<AppSettings> {
        let _maintenance_guard = self.maintenance_lock.lock().await;
        let _trade_guard = self.trade_lock.lock().await;
        secrets::delete_binance_credentials()?;
        let mut runtime = self.runtime.write().await;
        runtime.connected = false;
        runtime.api_key = None;
        runtime.api_secret = None;
        runtime.saved_credentials = false;
        let settings = runtime.settings();
        drop(runtime);
        {
            let mut cached_client = self.client.write().await;
            *cached_client = None;
        }
        {
            let mut candidates = self.candidates.write().await;
            candidates.clear();
        }
        self.record_log(
            "account.credentials.deleted",
            "已从系统 keyring 删除 Binance API 凭据",
        )?;
        Ok(settings)
    }

    /// 从 Binance 实盘数据扫描候选，并缓存给前端快照使用。
    ///
    /// 合约、mark greeks、期权买一价和现货价格并发读取，减少一次刷新候选的等待时间。
    pub async fn scan_candidates(&self) -> AppResult<CandidateScanResult> {
        let client = self.live_client().await?;
        let contracts_task = spawn_binance_request({
            let client = client.clone();
            async move { client.exchange_info().await }
        });
        let marks_task = spawn_binance_request({
            let client = client.clone();
            async move { client.option_mark_prices().await }
        });
        let tickers_task = spawn_binance_request({
            let client = client.clone();
            async move { client.option_tickers().await }
        });
        let spot_price_task = spawn_binance_request({
            let client = client.clone();
            async move { client.spot_price("BTCUSDT").await }
        });
        let contracts = join_binance_request(contracts_task).await;
        let marks = join_binance_request(marks_task).await;
        let tickers = join_binance_request(tickers_task).await;
        let spot_price = join_binance_request(spot_price_task).await;
        let contracts = contracts?;
        let marks = marks?;
        let tickers = tickers?;
        let spot_price = spot_price?;
        let result = strategy::build_candidates(strategy::CandidateBuildInputs {
            contracts: &contracts,
            marks: &marks,
            tickers: &tickers,
            spot_price,
        });
        {
            let mut cached_candidates = self.candidates.write().await;
            cached_candidates.clone_from(&result.candidates);
        }
        Ok(result)
    }

    /// 手动发送真实期权策略订单。
    ///
    /// `confirm_live` 必须为 true；前端的确认弹窗和 Rust 的门禁缺一不可。
    pub async fn place_manual_strategy(
        &self,
        request: StrategyRequest,
        app: AppHandle,
    ) -> AppResult<StrategyOrder> {
        let _trade_guard = self.trade_lock.lock().await;
        let progress = OrderProgressEmitter::manual(app, &request);
        if !request.confirm_live {
            progress.emit(
                OrderProgressStage::Validate,
                OrderProgressStatus::Error,
                "手动下单需要显式确认",
                Some(request.option_symbol.as_str()),
                None,
            );
            return Err(AppError::RiskRejected("手动下单需要显式确认".to_string()));
        }
        if self.pending_recovery_count()? > 0 {
            progress.emit(
                OrderProgressStage::Validate,
                OrderProgressStatus::Error,
                "存在待确认恢复记录，已暂停新的真实下单",
                Some(request.option_symbol.as_str()),
                None,
            );
            return Err(AppError::RiskRejected(
                "存在待确认恢复记录，已暂停新的真实下单".to_string(),
            ));
        }
        if self.settlement_reserves_assets().await? {
            return Err(AppError::RiskRejected(
                "存在未完成的到期结算，资金已预留，暂不能创建新策略".to_string(),
            ));
        }
        self.place_live_strategy(request, &progress).await
    }

    /// 根据恢复记录重新查询 Binance 并恢复本地订单状态。
    ///
    /// 这里不会信任恢复记录里的本地状态来创建订单，必须重新查询交易所事实。
    pub async fn recover_order_attempt(&self, attempt_id: String) -> AppResult<StrategyOrder> {
        let _trade_guard = self.trade_lock.lock().await;
        let attempt = self
            .persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .list_order_attempts(usize::MAX)?
            .into_iter()
            .find(|attempt| attempt.id == attempt_id)
            .ok_or_else(|| AppError::Storage("未找到指定恢复记录".to_string()))?;

        let linked_spot = self
            .orders
            .read()
            .await
            .iter()
            .any(|order| order.spot_client_order_id == attempt.client_order_id);
        if is_spot_attempt(attempt.status) || linked_spot {
            self.recover_spot_attempt(attempt).await
        } else {
            self.recover_option_attempt(attempt).await
        }
    }

    /// 根据资金恢复记录重新查询 Binance 历史并推进状态。
    pub async fn recover_funding_attempt(
        &self,
        attempt_id: String,
    ) -> AppResult<FundingAttemptView> {
        let _trade_guard = self.trade_lock.lock().await;
        let client = self.live_client().await?;
        let attempt = self
            .persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .list_funding_attempts(usize::MAX)?
            .into_iter()
            .find(|attempt| attempt.id == attempt_id)
            .ok_or_else(|| AppError::Storage("未找到指定资金恢复记录".to_string()))?;

        let recovered = self.recover_funding_attempt_inner(&client, attempt).await?;
        Ok(FundingAttemptView::from(recovered))
    }

    /// 人工核对 Binance 事实后解除已阻断的资金恢复记录。
    pub async fn resolve_funding_attempt(
        &self,
        request: &ResolveFundingAttemptRequest,
    ) -> AppResult<FundingAttemptView> {
        let _trade_guard = self.trade_lock.lock().await;
        if !request.confirm_manual {
            return Err(AppError::RiskRejected(
                "人工解除资金阻断需要显式确认".to_string(),
            ));
        }
        if request.attempt_id.trim().is_empty() {
            return Err(AppError::RiskRejected(
                "资金恢复记录 ID 不能为空".to_string(),
            ));
        }

        let mut attempt = self
            .persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .list_funding_attempts(usize::MAX)?
            .into_iter()
            .find(|attempt| attempt.id == request.attempt_id)
            .ok_or_else(|| AppError::Storage("未找到指定资金恢复记录".to_string()))?;

        if attempt.status != FundingAttemptStatus::Blocked {
            return Err(AppError::RiskRejected(
                "只有已阻断的资金恢复记录可以人工解除".to_string(),
            ));
        }

        attempt.status = match request.resolution {
            FundingAttemptManualResolution::Confirmed => FundingAttemptStatus::Confirmed,
            FundingAttemptManualResolution::Failed => FundingAttemptStatus::Failed,
        };
        attempt.query_after_at = None;
        attempt.updated_at = Utc::now();
        attempt.message = format!(
            "人工核对 Binance 事实后标记为{}，未重新发送资金请求",
            request.resolution.display()
        );
        self.save_funding_attempt(&attempt)?;
        self.record_warning(
            "funding.recovery.manual_resolved",
            &format!(
                "{} {} {} 已人工标记为{}，用于{}",
                attempt.kind.display(),
                attempt.amount,
                attempt.asset,
                request.resolution.display(),
                attempt.purpose
            ),
        )?;
        Ok(FundingAttemptView::from(attempt))
    }

    /// 列出当前 Binance 期权持仓中可由本机接管监控的记录。
    pub async fn list_adoptable_option_positions(
        &self,
    ) -> AppResult<Vec<AdoptableOptionPositionView>> {
        let client = self.live_client().await?;
        let contracts = client.exchange_info().await?;
        let positions = client.option_positions().await?;
        let orders = self.orders.read().await.clone();
        let mut rows = positions
            .iter()
            .filter_map(|position| {
                contracts
                    .iter()
                    .find(|contract| contract.symbol == position.symbol)
                    .map(|contract| adoptable_position_view(position, contract, &orders))
            })
            .collect::<AppResult<Vec<_>>>()?;
        rows.sort_by(|left, right| {
            left.expiry_at
                .cmp(&right.expiry_at)
                .then_with(|| left.symbol.cmp(&right.symbol))
        });
        Ok(rows)
    }

    /// 把选中的 Binance 当前期权持仓接管为本地策略订单。
    ///
    /// 接管不发送交易所订单，但之后会进入本机同步、结算和现货补齐流程。
    pub async fn adopt_option_positions(
        &self,
        request: AdoptOptionPositionsRequest,
    ) -> AppResult<Vec<StrategyOrder>> {
        if !request.confirm_live {
            return Err(AppError::RiskRejected(
                "接管期权持仓需要显式确认后续到期补齐风险".to_string(),
            ));
        }
        let symbols = normalized_unique_symbols(&request.symbols)?;
        let _maintenance_guard = self.maintenance_lock.lock().await;
        let client = self.live_client().await?;
        let contracts = client.exchange_info().await?;
        let positions = client.option_positions().await?;
        let mut adopted = Vec::new();

        for symbol in symbols {
            let position = positions
                .iter()
                .find(|position| position.symbol == symbol)
                .ok_or_else(|| {
                    AppError::RiskRejected(format!(
                        "Binance 当前期权持仓中未找到 {symbol}，请刷新后重试"
                    ))
                })?;
            let contract = contracts
                .iter()
                .find(|contract| contract.symbol == symbol)
                .ok_or_else(|| {
                    AppError::RiskRejected(format!(
                        "{symbol} 已不在 Binance exchangeInfo 中，无法接管"
                    ))
                })?;
            {
                let orders = self.orders.read().await;
                ensure_position_can_be_adopted(position, contract, &orders)?;
            }
            let order = strategy::order_from_adopted_position(position, contract);
            self.save_order(&order).await?;
            self.record_log(
                "strategy.order.adopted",
                &format!(
                    "{} 已从 Binance 当前期权持仓接管，数量 {} 张",
                    order.option_symbol, order.filled_quantity
                ),
            )?;
            adopted.push(order);
        }
        Ok(adopted)
    }

    /// 刷新订单和结算计划。页面刷新与重连不会发送资金动作。
    pub async fn sync_live_state(&self) -> AppResult<AppSnapshot> {
        let _maintenance_guard = self.maintenance_lock.lock().await;
        let _trade_guard = self.trade_lock.lock().await;
        self.sync_live_orders().await?;
        self.discover_settlement_batches().await?;
        self.snapshot().await
    }

    /// 手动扫描与后台维护共用同一套既有订单结算流程。
    pub async fn run_settlement_scan(&self) -> AppResult<Vec<StrategyOrder>> {
        let _maintenance_guard = self.maintenance_lock.lock().await;
        self.live_client().await?;
        self.background_tick().await?;
        Ok(self.orders.read().await.clone())
    }

    /// 返回设置页日志快照。
    pub fn log_snapshot(&self) -> AppResult<LogSnapshot> {
        self.logs.snapshot()
    }

    /// 确保今日日志文件存在，并返回路径。
    pub fn ensure_current_log_file(&self) -> AppResult<std::path::PathBuf> {
        self.logs.ensure_current_file()
    }

    /// 确保日志目录存在，并返回路径。
    pub fn ensure_log_directory(&self) -> AppResult<std::path::PathBuf> {
        self.logs.ensure_directory()
    }

    /// 删除全部本地日志文件。
    pub fn clear_log_files(&self) -> AppResult<LogSnapshot> {
        self.logs.clear()?;
        self.logs.snapshot()
    }

    async fn load_order_live_inputs(client: &BinanceClient) -> AppResult<OrderLiveInputs> {
        let account_task = spawn_binance_request({
            let client = client.clone();
            async move { client.option_account().await }
        });
        let contracts_task = spawn_binance_request({
            let client = client.clone();
            async move { client.exchange_info().await }
        });
        let marks_task = spawn_binance_request({
            let client = client.clone();
            async move { client.option_mark_prices().await }
        });
        let account = join_binance_request(account_task).await;
        let contracts = join_binance_request(contracts_task).await;
        let marks = join_binance_request(marks_task).await;

        Ok(OrderLiveInputs {
            account: account?,
            contracts: contracts?,
            marks: marks?,
        })
    }

    /// 执行真实期权下单的核心流程。
    ///
    /// 顺序是：请求校验、读取实盘数据、合约匹配、风控校验、写入恢复记录、发送 Binance 订单、
    /// 保存本地订单。交易所接单后如果本地保存失败，会保留恢复记录并返回明确错误。
    #[allow(clippy::too_many_lines)]
    async fn place_live_strategy(
        &self,
        mut request: StrategyRequest,
        progress: &OrderProgressEmitter,
    ) -> AppResult<StrategyOrder> {
        progress.emit(
            OrderProgressStage::Validate,
            OrderProgressStatus::Running,
            "正在校验下单参数",
            Some(request.option_symbol.as_str()),
            None,
        );
        if let Err(error) = validate_request(&request) {
            progress.emit(
                OrderProgressStage::Validate,
                OrderProgressStatus::Error,
                error.to_string(),
                Some(request.option_symbol.as_str()),
                None,
            );
            return Err(error);
        }
        let client = match self.live_client().await {
            Ok(client) => client,
            Err(error) => {
                progress.emit(
                    OrderProgressStage::Validate,
                    OrderProgressStatus::Error,
                    error.to_string(),
                    Some(request.option_symbol.as_str()),
                    None,
                );
                return Err(error);
            }
        };
        progress.emit(
            OrderProgressStage::Validate,
            OrderProgressStatus::Running,
            "正在读取 Binance 账户、合约和行情",
            Some(request.option_symbol.as_str()),
            None,
        );
        let OrderLiveInputs {
            mut account,
            contracts,
            marks,
        } = match Self::load_order_live_inputs(&client).await {
            Ok(inputs) => inputs,
            Err(error) => {
                progress.emit(
                    OrderProgressStage::Validate,
                    OrderProgressStatus::Error,
                    error.to_string(),
                    Some(request.option_symbol.as_str()),
                    None,
                );
                return Err(error);
            }
        };
        let contract = match resolve_contract(&request, &contracts) {
            Ok(contract) => contract,
            Err(error) => {
                progress.emit(
                    OrderProgressStage::Validate,
                    OrderProgressStatus::Error,
                    error.to_string(),
                    Some(request.option_symbol.as_str()),
                    None,
                );
                return Err(error);
            }
        };
        progress.emit(
            OrderProgressStage::Validate,
            OrderProgressStatus::Running,
            "正在读取 Binance 当前买一价和买一量并计算主动成交限价",
            Some(&contract.symbol),
            None,
        );
        let order_book = match client.option_order_book(&contract.symbol).await {
            Ok(order_book) => order_book,
            Err(error) => {
                progress.emit(
                    OrderProgressStage::Validate,
                    OrderProgressStatus::Error,
                    error.to_string(),
                    Some(&contract.symbol),
                    None,
                );
                return Err(error);
            }
        };
        let Some(limit_price) = strategy::option_sell_taker_limit_price_for_quantity(
            &order_book,
            request.quantity,
            contract,
        ) else {
            let error = AppError::RiskRejected(format!(
                "{} 当前买一量不足以一次成交 {} 张，已阻止真实期权卖单",
                contract.symbol, request.quantity
            ));
            progress.emit(
                OrderProgressStage::Validate,
                OrderProgressStatus::Error,
                error.to_string(),
                Some(&contract.symbol),
                None,
            );
            return Err(error);
        };
        request.limit_price = limit_price;
        if let Err(error) = validate_contract_terms(&request, contract, &marks) {
            progress.emit(
                OrderProgressStage::Validate,
                OrderProgressStatus::Error,
                error.to_string(),
                Some(&contract.symbol),
                None,
            );
            return Err(error);
        }
        progress.emit(
            OrderProgressStage::Validate,
            OrderProgressStatus::Success,
            "合约、数量和价格约束已通过",
            Some(&contract.symbol),
            None,
        );

        progress.emit(
            OrderProgressStage::Funding,
            OrderProgressStatus::Running,
            "正在检查并准备期权保证金资金",
            Some(&contract.symbol),
            None,
        );
        account = match self
            .prepare_option_margin_funding(&client, &account, contract, &request)
            .await
        {
            Ok(account) => {
                progress.emit(
                    OrderProgressStage::Funding,
                    OrderProgressStatus::Success,
                    "期权保证金资金已准备完成",
                    Some(&contract.symbol),
                    None,
                );
                account
            }
            Err(error) => {
                progress.emit(
                    OrderProgressStage::Funding,
                    OrderProgressStatus::Error,
                    error.to_string(),
                    Some(&contract.symbol),
                    None,
                );
                return Err(error);
            }
        };
        if let Err(error) = validate_contract_order(&request, contract, &account, &marks) {
            self.sweep_option_idle_usdt_best_effort(&client, "期权下单校验失败后归集")
                .await;
            progress.emit(
                OrderProgressStage::Funding,
                OrderProgressStatus::Error,
                error.to_string(),
                Some(&contract.symbol),
                None,
            );
            return Err(error);
        }
        progress.emit(
            OrderProgressStage::Risk,
            OrderProgressStatus::Running,
            "正在执行本地下单风控",
            Some(&contract.symbol),
            None,
        );
        if let Err(error) = enforce_risk_rules(&request, contract, &account) {
            self.sweep_option_idle_usdt_best_effort(&client, "期权风控拒绝后归集")
                .await;
            progress.emit(
                OrderProgressStage::Risk,
                OrderProgressStatus::Error,
                error.to_string(),
                Some(&contract.symbol),
                None,
            );
            return Err(error);
        }
        progress.emit(
            OrderProgressStage::Risk,
            OrderProgressStatus::Success,
            "本地下单风控已通过",
            Some(&contract.symbol),
            None,
        );

        let client_order_id = make_client_order_id(request.direction);
        let now = Utc::now();
        // 先落恢复记录，再发送真实订单；这样即使本地订单保存失败，也能用客户端订单号恢复。
        let mut attempt = OrderAttempt {
            id: Uuid::new_v4().to_string(),
            client_order_id: client_order_id.clone(),
            symbol: request.symbol.clone(),
            option_symbol: contract.symbol.clone(),
            direction: request.direction,
            status: OrderAttemptStatus::Attempted,
            exchange_order_id: None,
            message: "已通过风控校验，准备发送 Binance 订单".to_string(),
            created_at: now,
            updated_at: now,
        };
        progress.emit(
            OrderProgressStage::Submit,
            OrderProgressStatus::Running,
            "正在写入恢复记录并发送 Binance 订单",
            Some(&contract.symbol),
            None,
        );
        if let Err(error) = self.save_order_attempt(&attempt) {
            progress.emit(
                OrderProgressStage::Submit,
                OrderProgressStatus::Error,
                error.to_string(),
                Some(&contract.symbol),
                None,
            );
            return Err(error);
        }
        if let Err(error) = self.record_log(
            "strategy.order.manual.attempted",
            &format!(
                "{} {} {} 张，合约 {}，价格 {}",
                request.symbol,
                request.direction.display(),
                request.quantity,
                contract.symbol,
                request.limit_price
            ),
        ) {
            progress.emit(
                OrderProgressStage::Submit,
                OrderProgressStatus::Error,
                error.to_string(),
                Some(&contract.symbol),
                None,
            );
            return Err(error);
        }

        let ack = match client.place_option_order(&request, &client_order_id).await {
            Ok(ack) => ack,
            Err(error) => {
                if is_uncertain_submit_error(&error) {
                    self.record_warning(
                        "strategy.order.manual.submit_uncertain",
                        &format!(
                            "{} 下单请求结果不确定，正在按客户端订单号查询 Binance",
                            contract.symbol
                        ),
                    )?;
                    match client
                        .query_option_order(&contract.symbol, None, Some(&client_order_id))
                        .await
                    {
                        Ok(ack) => ack,
                        Err(query_error) => {
                            attempt.message = format!(
                                "Binance 下单结果不确定，按客户端订单号查询失败：{query_error}"
                            );
                            attempt.updated_at = Utc::now();
                            self.save_order_attempt(&attempt)?;
                            progress.emit(
                                OrderProgressStage::Submit,
                                OrderProgressStatus::Error,
                                "Binance 下单结果不确定，请查看恢复记录并按客户端订单号恢复",
                                Some(&contract.symbol),
                                None,
                            );
                            return Err(AppError::Binance(format!(
                                "Binance 下单结果不确定，clientOrderId={client_order_id}。请先查看恢复记录并重新查询 Binance。原始错误：{error}；查询错误：{query_error}"
                            )));
                        }
                    }
                } else {
                    progress.emit(
                        OrderProgressStage::Submit,
                        OrderProgressStatus::Error,
                        error.to_string(),
                        Some(&contract.symbol),
                        None,
                    );
                    return Err(error);
                }
            }
        };
        progress.emit(
            OrderProgressStage::Submit,
            OrderProgressStatus::Success,
            format!("Binance 已返回订单号，状态 {}", ack.status),
            Some(&contract.symbol),
            Some(&ack.order_id),
        );
        attempt.status = OrderAttemptStatus::ExchangeAck;
        attempt.exchange_order_id = Some(ack.order_id.clone());
        attempt.message = format!("Binance 已返回订单号，状态 {}", ack.status);
        attempt.updated_at = Utc::now();
        progress.emit(
            OrderProgressStage::Persist,
            OrderProgressStatus::Running,
            "正在保存本地订单记录",
            Some(&contract.symbol),
            Some(&ack.order_id),
        );
        if let Err(error) = self.save_order_attempt(&attempt) {
            progress.emit(
                OrderProgressStage::Persist,
                OrderProgressStatus::Error,
                error.to_string(),
                Some(&contract.symbol),
                Some(&ack.order_id),
            );
            return Err(error);
        }

        if is_unfilled_terminal_option_ack(&ack) {
            let message = format!(
                "Binance 返回未成交终态 {}，成交数量 0，本地不创建策略订单",
                ack.status
            );
            attempt.status = OrderAttemptStatus::Failed;
            attempt.message.clone_from(&message);
            attempt.updated_at = Utc::now();
            self.save_order_attempt(&attempt)?;
            self.record_log(
                "strategy.order.manual.failed",
                &format!("{} {}", contract.symbol, message),
            )?;
            progress.emit(
                OrderProgressStage::Persist,
                OrderProgressStatus::Error,
                message.clone(),
                Some(&contract.symbol),
                Some(&ack.order_id),
            );
            self.sweep_option_idle_usdt_best_effort(&client, "期权下单未成交后归集")
                .await;
            return Err(AppError::Binance(message));
        }

        let order = strategy::order_from_request(&request, &ack);
        if let Err(error) = self.save_order(&order).await {
            attempt.status = OrderAttemptStatus::PersistFailed;
            attempt.message = "交易所订单已创建，但订单记录保存失败".to_string();
            attempt.updated_at = Utc::now();
            self.save_order_attempt(&attempt)?;
            progress.emit(
                OrderProgressStage::Persist,
                OrderProgressStatus::Error,
                "交易所订单已创建，但订单记录保存失败，请查看恢复记录",
                Some(&contract.symbol),
                Some(&ack.order_id),
            );
            return Err(AppError::Storage(format!(
                "交易所订单已创建但订单记录保存失败，exchangeOrderId={}，clientOrderId={}。请先用 Binance 后台核对后再继续操作。原始错误：{}",
                ack.order_id, client_order_id, error
            )));
        }

        attempt.status = OrderAttemptStatus::Persisted;
        attempt.message = "交易所订单和订单记录均已保存".to_string();
        attempt.updated_at = Utc::now();
        if let Err(error) = self.save_order_attempt(&attempt) {
            progress.emit(
                OrderProgressStage::Persist,
                OrderProgressStatus::Error,
                error.to_string(),
                Some(&contract.symbol),
                Some(&ack.order_id),
            );
            return Err(error);
        }
        if let Err(error) = self.record_log(
            "strategy.order.manual.created",
            &format!(
                "{} {} {} 张，执行价 {}，交易所状态 {}",
                order.symbol,
                order.direction.display(),
                order.quantity,
                order.strike,
                order.exchange_status
            ),
        ) {
            progress.emit(
                OrderProgressStage::Persist,
                OrderProgressStatus::Error,
                error.to_string(),
                Some(&contract.symbol),
                Some(&ack.order_id),
            );
            return Err(error);
        }
        progress.emit(
            OrderProgressStage::Persist,
            OrderProgressStatus::Success,
            format!("订单已保存，交易所状态 {}", order.exchange_status),
            Some(&contract.symbol),
            Some(&ack.order_id),
        );
        self.sweep_option_idle_usdt_best_effort(&client, "期权下单后归集")
            .await;
        Ok(order)
    }

    /// 恢复期权订单尝试记录。
    ///
    /// 用交易所订单号或客户端订单号查询 Binance 后，再重建本地订单。
    async fn recover_option_attempt(&self, mut attempt: OrderAttempt) -> AppResult<StrategyOrder> {
        let client = self.live_client().await?;
        let ack = client
            .query_option_order(
                &attempt.option_symbol,
                attempt.exchange_order_id.as_deref(),
                Some(&attempt.client_order_id),
            )
            .await?;
        if is_unfilled_terminal_option_ack(&ack) {
            let message = format!(
                "Binance 返回未成交终态 {}，成交数量 0，本地不创建策略订单",
                ack.status
            );
            attempt.status = OrderAttemptStatus::Failed;
            attempt.exchange_order_id = Some(ack.order_id.clone());
            attempt.message.clone_from(&message);
            attempt.updated_at = Utc::now();
            self.save_order_attempt(&attempt)?;
            self.record_log(
                "strategy.order.recover_failed",
                &format!("{} {}", attempt.option_symbol, message),
            )?;
            return Err(AppError::Binance(message));
        }
        let contracts = client.exchange_info().await?;
        let contract = contracts
            .iter()
            .find(|contract| contract.symbol == attempt.option_symbol)
            .ok_or_else(|| {
                AppError::RiskRejected(
                    "恢复记录中的 Binance 期权合约已不在 exchangeInfo 中，无法精确恢复到期时间"
                        .to_string(),
                )
            })?;
        let request = request_from_option_attempt(&attempt, &ack, contract)?;
        let order = strategy::order_from_request(&request, &ack);
        self.save_order(&order).await?;
        attempt.status = OrderAttemptStatus::Persisted;
        attempt.exchange_order_id = Some(ack.order_id.clone());
        attempt.message = "已从 Binance 查询订单并恢复到订单记录".to_string();
        attempt.updated_at = Utc::now();
        self.save_order_attempt(&attempt)?;
        self.record_log(
            "strategy.order.recovered",
            &format!("{} 已从恢复记录重建订单记录", attempt.option_symbol),
        )?;
        Ok(order)
    }

    /// 恢复现货补齐尝试记录。
    ///
    /// 现货补齐必须找到对应期权订单，不能单独生成一条脱离策略的现货记录。
    async fn recover_spot_attempt(&self, mut attempt: OrderAttempt) -> AppResult<StrategyOrder> {
        let client = self.live_client().await?;
        let ack = client
            .query_spot_order(
                &attempt.symbol,
                attempt.exchange_order_id.as_deref(),
                Some(&attempt.client_order_id),
            )
            .await?;
        let mut order = {
            let orders = self.orders.read().await;
            orders
                .iter()
                .find(|order| order.spot_client_order_id == attempt.client_order_id)
                .cloned()
        }
        .ok_or_else(|| {
            AppError::Storage("未找到现货补齐对应的期权订单记录，无法自动恢复".to_string())
        })?;

        order.spot_order_id = Some(ack.order_id.clone());
        order.spot_client_order_id = attempt.client_order_id.clone();
        apply_spot_ack(&mut order, &ack);
        attempt.status = OrderAttemptStatus::SpotPersisted;
        attempt.exchange_order_id = Some(ack.order_id.clone());
        attempt.message = "已从 Binance 查询现货订单并恢复补齐状态".to_string();
        attempt.updated_at = Utc::now();
        self.save_settlement_order(&order, &attempt).await?;
        self.record_log(
            "strategy.rebalance.recovered",
            &format!("{} 已从恢复记录重建现货补齐状态", attempt.option_symbol),
        )?;
        Ok(order)
    }

    /// 同步所有仍可能变化的期权订单状态。
    async fn sync_live_orders(&self) -> AppResult<Vec<StrategyOrder>> {
        let client = self.live_client().await?;
        let orders = self.orders.read().await.clone();
        let mut updated = Vec::new();

        // 单个订单同步失败不阻断其他订单，避免一个外部异常拖住整批状态刷新。
        for order in orders.into_iter().filter(should_sync_option_order) {
            match self.sync_single_order(&client, order.clone()).await {
                Ok(Some(order)) => updated.push(order),
                Ok(None) => {}
                Err(error) => {
                    self.record_warning(
                        "strategy.order.sync_failed",
                        &format!("{} 同步失败：{}", order.option_symbol, error),
                    )?;
                }
            }
        }
        Ok(updated)
    }

    /// 同步单个期权订单并保存结果。
    async fn sync_single_order(
        &self,
        client: &BinanceClient,
        mut order: StrategyOrder,
    ) -> AppResult<Option<StrategyOrder>> {
        if has_exchange_order_ref(&order) {
            let ack = client
                .query_option_order(
                    &order.option_symbol,
                    order.exchange_order_id.as_deref(),
                    Some(&order.client_order_id),
                )
                .await?;
            if is_unfilled_terminal_option_ack(&ack) {
                self.delete_order(&order.id).await?;
                self.record_log(
                    "strategy.order.unfilled_removed",
                    &format!(
                        "{} 交易所状态 {} 且未成交，已从本地订单移除",
                        order.option_symbol, ack.status
                    ),
                )?;
                return Ok(None);
            }
            apply_option_ack(&mut order, &ack);
        } else {
            self.sync_adopted_position_order(client, &mut order).await?;
        }
        self.save_order(&order).await?;
        Ok(Some(order))
    }

    /// 同步没有原始订单号的接管记录。
    ///
    /// 未到期前如果 Binance 当前持仓消失，说明外部已经平仓，本机停止后续补齐动作；
    /// 已到期后持仓可能已经被交易所现金结算移除，此时保留订单进入结算扫描。
    async fn sync_adopted_position_order(
        &self,
        client: &BinanceClient,
        order: &mut StrategyOrder,
    ) -> AppResult<()> {
        let positions = client.option_positions().await?;
        let position = positions
            .iter()
            .find(|position| position.symbol == order.option_symbol && is_short_position(position));
        if let Some(position) = position {
            apply_option_position(order, position);
            return Ok(());
        }
        if Utc::now() < order.expiry_at {
            order.status = StrategyStatus::Cancelled;
            order.exchange_status = "POSITION_CLOSED".to_string();
            order.updated_at = Utc::now();
            order.rebalance_message =
                "Binance 当前期权持仓已不存在，本机停止跟踪该接管记录".to_string();
        }
        Ok(())
    }

    /// 后台轮询任务的一次 tick。
    ///
    /// 只做恢复、订单同步和到期结算，不会创建新的策略订单。
    async fn background_tick(&self) -> AppResult<()> {
        if !self.runtime.read().await.connected {
            return Ok(());
        }
        self.recover_due_funding_attempts().await?;
        self.recover_pending_settlement_orders().await?;
        let _trade_guard = self.trade_lock.lock().await;
        self.sync_live_orders().await?;
        self.discover_settlement_batches().await?;
        self.advance_settlement_batches().await?;
        Ok(())
    }

    /// 启动后台轮询任务。
    ///
    /// `worker_started` 防止多次连接或重复调用时启动多个轮询器。
    pub(crate) fn ensure_background_worker(&self) {
        if self.worker_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let state = self.clone();
        tauri::async_runtime::spawn(async move {
            // 维护属于应用生命周期；连接命令本身不发起结算资金动作。
            let period = StdDuration::from_mins(1);
            let mut interval =
                tokio::time::interval_at(tokio::time::Instant::now() + period, period);
            loop {
                interval.tick().await;
                let Ok(_maintenance_guard) = state.maintenance_lock.try_lock() else {
                    // 后台维护不排队堆积；上一轮未结束时直接跳过并记录脱敏日志。
                    if let Err(error) = state.record_warning(
                        "background.tick_skipped",
                        "上一轮后台同步仍在运行，本轮跳过",
                    ) {
                        let _ = state
                            .record_error("background.tick_skip_log_failed", &error.to_string());
                    }
                    continue;
                };
                if let Err(error) = state.background_tick().await {
                    let _ = state.record_error("background.tick_failed", &error.to_string());
                }
            }
        });
    }

    /// 保存订单到 SQLite，并同步更新内存缓存。
    async fn save_order(&self, order: &StrategyOrder) -> AppResult<()> {
        {
            let persistence = self
                .persistence
                .lock()
                .map_err(|error| AppError::Storage(error.to_string()))?;
            persistence.save_order(order)?;
        }
        let mut orders = self.orders.write().await;
        if let Some(stored) = orders.iter_mut().find(|stored| stored.id == order.id) {
            *stored = order.clone();
        } else {
            orders.insert(0, order.clone());
        }
        Ok(())
    }

    /// 删除本地订单快照，并同步更新内存缓存。
    async fn delete_order(&self, order_id: &str) -> AppResult<()> {
        {
            let persistence = self
                .persistence
                .lock()
                .map_err(|error| AppError::Storage(error.to_string()))?;
            persistence.delete_order(order_id)?;
        }
        let mut orders = self.orders.write().await;
        orders.retain(|order| order.id != order_id);
        Ok(())
    }

    /// 写信息日志。
    ///
    /// 调用方传入的 message 必须是脱敏摘要，不应包含原始 Binance 响应。
    fn record_log(&self, event: &str, message: &str) -> AppResult<()> {
        self.logs.append(LogLevel::Info, event, message)
    }

    /// 写警告日志。
    fn record_warning(&self, event: &str, message: &str) -> AppResult<()> {
        self.logs.append(LogLevel::Warn, event, message)
    }

    /// 写错误日志。
    fn record_error(&self, event: &str, message: &str) -> AppResult<()> {
        self.logs.append(LogLevel::Error, event, message)
    }

    fn pending_recovery_count(&self) -> AppResult<usize> {
        let persistence = self
            .persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?;
        Ok(persistence.count_pending_order_attempts()?
            + persistence.count_pending_funding_attempts()?)
    }

    /// 保存下单/补齐恢复记录。
    fn save_order_attempt(&self, attempt: &OrderAttempt) -> AppResult<()> {
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .save_order_attempt(attempt)
    }

    /// 保存资金动作恢复记录。
    fn save_funding_attempt(&self, attempt: &FundingAttempt) -> AppResult<()> {
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .save_funding_attempt(attempt)
    }

    /// 读取同一资金链路上的恢复记录。
    fn funding_attempts_by_chain(&self, chain_id: &str) -> AppResult<Vec<FundingAttempt>> {
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .list_funding_attempts_by_chain(chain_id)
    }

    /// 获取可用 Binance 客户端；缓存缺失时从运行期凭据重建。
    async fn live_client(&self) -> AppResult<BinanceClient> {
        if let Some(client) = self.client.read().await.as_ref().cloned() {
            return Ok(client);
        }
        let runtime = self.runtime.read().await.clone();
        let client = client_from_runtime(&runtime)?;
        {
            let mut cached_client = self.client.write().await;
            *cached_client = Some(client.clone());
        }
        Ok(client)
    }

    /// 已连接时读取期权账户、现货账户和现货行情价格。
    ///
    /// 未连接返回 `None`，让快照保持明确的空状态，而不是前端 fallback 数据。
    async fn live_accounts_if_connected(&self) -> AppResult<Option<LiveAccounts>> {
        let connected = self.runtime.read().await.connected;
        if !connected {
            return Ok(None);
        }
        let client = self.live_client().await?;
        let option_account_task = spawn_binance_request({
            let client = client.clone();
            async move { client.option_account().await }
        });
        let spot_account_task = spawn_binance_request({
            let client = client.clone();
            async move { client.spot_account().await }
        });
        let earn_task = spawn_binance_request({
            let client = client.clone();
            async move { client.simple_earn_snapshot().await }
        });
        let spot_prices_task = spawn_binance_request({
            let client = client.clone();
            async move { client.spot_price_tickers().await }
        });
        let option_account = join_binance_request(option_account_task).await;
        let spot_account = join_binance_request(spot_account_task).await;
        let earn = join_binance_request(earn_task).await;
        let spot_prices = join_binance_request(spot_prices_task).await;
        let option_account = option_account?;
        let spot_account = spot_account?;
        let earn = earn?;
        let spot_prices = spot_prices?;
        Ok(Some(LiveAccounts {
            option: option_account,
            spot: spot_account,
            earn,
            spot_prices,
        }))
    }

    /// 恢复到期的资金动作记录；该流程只依据 Binance 历史推进，不信任本地日志事实。
    async fn recover_due_funding_attempts(&self) -> AppResult<()> {
        // 读取恢复记录也受交易锁保护，避免查询覆盖刚完成的人工处置。
        let _trade_guard = self.trade_lock.lock().await;
        if !self.runtime.read().await.connected {
            return Ok(());
        }
        let client = self.live_client().await?;
        let now = Utc::now();
        let attempts = {
            let persistence = self
                .persistence
                .lock()
                .map_err(|error| AppError::Storage(error.to_string()))?;
            persistence
                .list_funding_attempts(usize::MAX)?
                .into_iter()
                .filter(|attempt| {
                    !attempt.status.is_terminal()
                        && attempt.status != FundingAttemptStatus::Blocked
                        && attempt
                            .query_after_at
                            .is_none_or(|query_after_at| query_after_at <= now)
                })
                .collect::<Vec<_>>()
        };

        for attempt in attempts {
            if let Err(error) = self.recover_funding_attempt_inner(&client, attempt).await {
                self.record_warning(
                    "funding.recovery.failed",
                    &format!("资金恢复查询失败：{error}"),
                )?;
            }
        }
        Ok(())
    }

    async fn recover_funding_attempt_inner(
        &self,
        client: &BinanceClient,
        attempt: FundingAttempt,
    ) -> AppResult<FundingAttempt> {
        // 恢复入口只查询事实，不能因为重新连接或点击恢复而重放请求。
        if attempt.status.is_terminal() {
            return Ok(attempt);
        }
        self.query_funding_attempt(client, attempt).await
    }

    async fn execute_funding_request(
        &self,
        client: &BinanceClient,
        request: FundingRequest,
    ) -> AppResult<FundingAttempt> {
        if let Some(existing) = self
            .funding_attempts_by_chain(&request.chain_id)?
            .into_iter()
            .find(|attempt| {
                attempt.kind == request.kind
                    && attempt.asset == request.asset
                    && attempt.transfer_type == request.transfer_type
                    && attempt.product_id == request.product_id
            })
        {
            return if existing.status == FundingAttemptStatus::Confirmed {
                Ok(existing)
            } else {
                Err(AppError::RiskRejected(
                    "该资金步骤已有恢复记录，只能查询或人工核对，不能重复发送".to_string(),
                ))
            };
        }
        if self.settlement_reserves_assets().await?
            && !self
                .settlement_batches()?
                .iter()
                .any(|batch| request.chain_id.starts_with(&format!("{}-", batch.id)))
        {
            return Err(AppError::RiskRejected(
                "资金已预留给到期结算，暂停其它资金动作".to_string(),
            ));
        }
        if self.pending_recovery_count()? > 0 {
            return Err(AppError::RiskRejected(
                "存在待确认恢复记录，暂停新的资金动作".to_string(),
            ));
        }
        // 先落库再发送真实资金请求，确保请求结果不确定时有可恢复的本地事实。
        let now = Utc::now();
        let attempt = FundingAttempt {
            id: Uuid::new_v4().to_string(),
            chain_id: request.chain_id,
            kind: request.kind,
            purpose: request.purpose,
            asset: request.asset,
            amount: request.amount,
            transfer_type: request.transfer_type,
            product_id: request.product_id,
            exchange_ref: None,
            status: FundingAttemptStatus::Attempted,
            message: "已写入资金恢复记录，尚未开始发送 Binance 请求".to_string(),
            query_after_at: None,
            send_count: 0,
            query_count: 0,
            effective_query_count: 0,
            transport_error_count: 0,
            submit_error_kind: None,
            history_error_kind: None,
            history_window_start_at: None,
            history_window_end_at: None,
            created_at: now,
            updated_at: now,
        };
        let attempt = self.send_funding_attempt(client, attempt).await?;
        if attempt.status == FundingAttemptStatus::Confirmed {
            Ok(attempt)
        } else {
            Err(AppError::RiskRejected(format!(
                "{} {} {} 尚未被 Binance 确认，已暂停后续资金动作",
                attempt.kind.display(),
                attempt.amount,
                attempt.asset
            )))
        }
    }

    async fn send_funding_attempt(
        &self,
        client: &BinanceClient,
        mut attempt: FundingAttempt,
    ) -> AppResult<FundingAttempt> {
        attempt.send_count = attempt.send_count.saturating_add(1);
        attempt.status = FundingAttemptStatus::Attempted;
        attempt.message = format!(
            "正在发送 Binance {} 请求，用于{}",
            attempt.kind.display(),
            attempt.purpose
        );
        attempt.updated_at = Utc::now();
        self.save_funding_attempt(&attempt)?;

        let result = match attempt.kind {
            FundingAttemptKind::UniversalTransfer => {
                let transfer_type = attempt.transfer_type.as_deref().ok_or_else(|| {
                    AppError::Storage("账户划转恢复记录缺少 transfer_type".to_string())
                })?;
                client
                    .universal_transfer(transfer_type, &attempt.asset, attempt.amount)
                    .await
            }
            FundingAttemptKind::SimpleEarnRedeem => {
                let product_id = attempt.product_id.as_deref().ok_or_else(|| {
                    AppError::Storage("活期赎回恢复记录缺少 product_id".to_string())
                })?;
                client
                    .redeem_simple_earn_flexible(product_id, attempt.amount)
                    .await
            }
            FundingAttemptKind::SimpleEarnSubscribe => {
                let product_id = attempt.product_id.as_deref().ok_or_else(|| {
                    AppError::Storage("活期申购恢复记录缺少 product_id".to_string())
                })?;
                client
                    .subscribe_simple_earn_flexible(product_id, attempt.amount)
                    .await
            }
        };

        match result {
            Ok(exchange_ref) => {
                attempt.status = FundingAttemptStatus::ExchangeAck;
                attempt.exchange_ref = Some(exchange_ref);
                attempt.submit_error_kind = None;
                attempt.message = "Binance 已返回资金动作引用，正在查询历史确认".to_string();
                attempt.updated_at = Utc::now();
                self.save_funding_attempt(&attempt)?;
                self.query_funding_attempt(client, attempt).await
            }
            Err(error) if is_uncertain_funding_error(&error) => {
                // 结果不确定时不重发资金动作，改为按 Binance 历史确认。
                attempt.status = FundingAttemptStatus::Uncertain;
                let error_kind = classify_funding_error(&error);
                attempt.submit_error_kind = Some(error_kind.to_string());
                attempt.message =
                    format!("资金请求结果不确定，正在按 Binance 历史确认：{error_kind}");
                attempt.query_after_at = Some(Utc::now());
                attempt.updated_at = Utc::now();
                self.save_funding_attempt(&attempt)?;
                self.query_funding_attempt(client, attempt).await
            }
            Err(error) => {
                attempt.status = FundingAttemptStatus::Failed;
                let error_kind = classify_funding_error(&error);
                attempt.submit_error_kind = Some(error_kind.to_string());
                attempt.message = format!("Binance 明确拒绝或本地校验失败：{error_kind}");
                attempt.query_after_at = None;
                attempt.updated_at = Utc::now();
                self.save_funding_attempt(&attempt)?;
                Err(error)
            }
        }
    }

    async fn query_funding_attempt(
        &self,
        client: &BinanceClient,
        mut attempt: FundingAttempt,
    ) -> AppResult<FundingAttempt> {
        // 总查询次数记录所有恢复查询尝试；有效查询次数只在 Binance 历史接口成功返回后增加。
        let now = Utc::now();
        let (window_start, window_end) = funding_history_window(&attempt, now);
        attempt.query_count = attempt.query_count.saturating_add(1);
        attempt.history_window_start_at = Some(window_start);
        attempt.history_window_end_at = Some(window_end);
        attempt.updated_at = now;
        self.save_funding_attempt(&attempt)?;

        let outcome = match self
            .lookup_funding_history(client, &attempt, window_start, window_end)
            .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                self.apply_funding_history_query_error(&mut attempt, &error)?;
                return Ok(attempt);
            }
        };
        attempt.effective_query_count = attempt.effective_query_count.saturating_add(1);
        attempt.history_error_kind = None;
        self.apply_funding_history_outcome(&mut attempt, outcome)?;
        Ok(attempt)
    }

    fn apply_funding_history_query_error(
        &self,
        attempt: &mut FundingAttempt,
        error: &AppError,
    ) -> AppResult<()> {
        let error_kind = classify_funding_error(error);
        if is_transient_binance_error(error) {
            attempt.transport_error_count = attempt.transport_error_count.saturating_add(1);
            attempt.history_error_kind = Some(error_kind.to_string());
            attempt.query_after_at =
                Some(Utc::now() + funding_backoff_after_count(attempt.transport_error_count));
            attempt.message = format!("Binance 历史查询暂时失败，稍后继续确认：{error_kind}");
        } else {
            attempt.status = FundingAttemptStatus::Blocked;
            attempt.history_error_kind = Some(error_kind.to_string());
            attempt.query_after_at = None;
            attempt.message =
                format!("Binance 历史查询发生不可自动确认错误，已阻断资金动作：{error_kind}");
        }
        attempt.updated_at = Utc::now();
        self.save_funding_attempt(attempt)
    }

    fn apply_funding_history_outcome(
        &self,
        attempt: &mut FundingAttempt,
        outcome: FundingHistoryOutcome,
    ) -> AppResult<()> {
        match outcome {
            FundingHistoryOutcome::Confirmed(exchange_ref, status) => {
                attempt.status = FundingAttemptStatus::Confirmed;
                attempt.exchange_ref = Some(exchange_ref);
                attempt.query_after_at = None;
                attempt.message = format!("已按 Binance 历史确认成功，状态 {status}");
                attempt.updated_at = Utc::now();
                self.save_funding_attempt(attempt)?;
                self.record_log(
                    "funding.recovery.confirmed",
                    &format!(
                        "{} {} {} 已由 Binance 历史确认，用于{}",
                        attempt.kind.display(),
                        attempt.amount,
                        attempt.asset,
                        attempt.purpose
                    ),
                )?;
            }
            FundingHistoryOutcome::Pending(exchange_ref, status) => {
                if let Some(exchange_ref) = exchange_ref {
                    attempt.exchange_ref = Some(exchange_ref);
                }
                attempt.status = FundingAttemptStatus::Pending;
                attempt.query_after_at =
                    Some(Utc::now() + funding_backoff_after_count(attempt.effective_query_count));
                attempt.message = format!("Binance 历史显示仍在处理中，状态 {status}");
                attempt.updated_at = Utc::now();
                self.save_funding_attempt(attempt)?;
            }
            FundingHistoryOutcome::Failed(exchange_ref, status) => {
                if let Some(exchange_ref) = exchange_ref {
                    attempt.exchange_ref = Some(exchange_ref);
                }
                attempt.status = FundingAttemptStatus::Failed;
                attempt.query_after_at = None;
                attempt.message = format!("Binance 历史确认失败，状态 {status}");
                attempt.updated_at = Utc::now();
                self.save_funding_attempt(attempt)?;
            }
            FundingHistoryOutcome::Ambiguous => {
                // 多条匹配比查不到更危险：无法判断哪条是本机请求，继续自动动作可能重复移动资金。
                attempt.status = FundingAttemptStatus::Blocked;
                attempt.query_after_at = None;
                attempt.message =
                    "Binance 历史匹配到多条可能记录，无法唯一确认，已阻断自动资金动作".to_string();
                attempt.updated_at = Utc::now();
                self.save_funding_attempt(attempt)?;
            }
            FundingHistoryOutcome::NotFound => {
                if attempt.effective_query_count >= funding_history_effective_query_limit() {
                    attempt.status = FundingAttemptStatus::Blocked;
                    attempt.query_after_at = None;
                    attempt.message =
                        "多次有效查询 Binance 历史仍无法确认该资金动作是否发生，已阻断自动资金动作"
                            .to_string();
                } else {
                    attempt.status = FundingAttemptStatus::Uncertain;
                    attempt.query_after_at = Some(
                        Utc::now() + funding_backoff_after_count(attempt.effective_query_count),
                    );
                    attempt.message =
                        "暂未在 Binance 历史中确认该资金动作，稍后继续查询".to_string();
                }
                attempt.updated_at = Utc::now();
                self.save_funding_attempt(attempt)?;
            }
        }
        Ok(())
    }

    async fn lookup_funding_history(
        &self,
        client: &BinanceClient,
        attempt: &FundingAttempt,
        window_start: chrono::DateTime<Utc>,
        window_end: chrono::DateTime<Utc>,
    ) -> AppResult<FundingHistoryOutcome> {
        // 每种资金动作走对应 Binance 历史接口，不能把日志或前端进度当作资金事实。
        let start_time_ms = window_start.timestamp_millis();
        let end_time_ms = window_end.timestamp_millis();
        match attempt.kind {
            FundingAttemptKind::UniversalTransfer => {
                let transfer_type = attempt.transfer_type.as_deref().ok_or_else(|| {
                    AppError::Storage("账户划转恢复记录缺少 transfer_type".to_string())
                })?;
                let records = client
                    .universal_transfer_history(transfer_type, start_time_ms, end_time_ms)
                    .await?;
                Ok(match_universal_transfer_record(attempt, &records))
            }
            FundingAttemptKind::SimpleEarnRedeem => {
                let records = client
                    .simple_earn_flexible_redemption_history(
                        attempt.product_id.as_deref(),
                        attempt.exchange_ref.as_deref(),
                        &attempt.asset,
                        start_time_ms,
                        end_time_ms,
                    )
                    .await?;
                Ok(match_simple_earn_redemption_record(attempt, &records))
            }
            FundingAttemptKind::SimpleEarnSubscribe => {
                let records = client
                    .simple_earn_flexible_subscription_history(
                        attempt.product_id.as_deref(),
                        attempt.exchange_ref.as_deref(),
                        &attempt.asset,
                        start_time_ms,
                        end_time_ms,
                    )
                    .await?;
                Ok(match_simple_earn_subscription_record(attempt, &records))
            }
        }
    }

    async fn execute_universal_transfer(
        &self,
        client: &BinanceClient,
        chain_id: &str,
        purpose: &str,
        transfer_type: &str,
        asset: &str,
        amount: Decimal,
    ) -> AppResult<()> {
        // 统一走 execute_funding_request，保证划转前一定有 funding_attempts 可恢复事实。
        self.execute_funding_request(
            client,
            FundingRequest {
                chain_id: chain_id.to_string(),
                kind: FundingAttemptKind::UniversalTransfer,
                purpose: purpose.to_string(),
                asset: asset.to_string(),
                amount,
                transfer_type: Some(transfer_type.to_string()),
                product_id: None,
            },
        )
        .await
        .map(|_| ())
    }

    async fn execute_simple_earn_redeem(
        &self,
        client: &BinanceClient,
        chain_id: &str,
        purpose: &str,
        product_id: &str,
        asset: &str,
        amount: Decimal,
    ) -> AppResult<()> {
        // Simple Earn 赎回是真实资金动作，不能绕过恢复记录和历史确认流程。
        self.execute_funding_request(
            client,
            FundingRequest {
                chain_id: chain_id.to_string(),
                kind: FundingAttemptKind::SimpleEarnRedeem,
                purpose: purpose.to_string(),
                asset: asset.to_string(),
                amount,
                transfer_type: None,
                product_id: Some(product_id.to_string()),
            },
        )
        .await
        .map(|_| ())
    }

    async fn execute_simple_earn_subscribe(
        &self,
        client: &BinanceClient,
        chain_id: &str,
        purpose: &str,
        product_id: &str,
        asset: &str,
        amount: Decimal,
    ) -> AppResult<()> {
        // 归集后的申购也写入同一恢复链路，避免收尾动作结果不确定时丢失上下文。
        self.execute_funding_request(
            client,
            FundingRequest {
                chain_id: chain_id.to_string(),
                kind: FundingAttemptKind::SimpleEarnSubscribe,
                purpose: purpose.to_string(),
                asset: asset.to_string(),
                amount,
                transfer_type: None,
                product_id: Some(product_id.to_string()),
            },
        )
        .await
        .map(|_| ())
    }

    /// 确保期权保证金账户有足够 USDT 支撑本次订单和当前风控缓冲。
    async fn prepare_option_margin_funding(
        &self,
        client: &BinanceClient,
        account: &OptionAccountSnapshot,
        contract: &OptionContract,
        request: &StrategyRequest,
    ) -> AppResult<OptionAccountSnapshot> {
        let shortfall = option_funding_shortfall(account, contract, request);
        if shortfall <= Decimal::ZERO {
            return Ok(account.clone());
        }

        let amount = round_up_funding_amount(shortfall);
        let chain_id = Uuid::new_v4().to_string();
        self.ensure_spot_asset_available(client, "USDT", amount, "期权保证金准备", &chain_id)
            .await?;
        self.execute_universal_transfer(
            client,
            &chain_id,
            "期权保证金准备",
            "MAIN_OPTION",
            "USDT",
            amount,
        )
        .await?;
        self.record_log(
            "funding.transfer.main_option",
            &format!("已从现货划转 {amount} USDT 到期权保证金账户，用于期权下单"),
        )?;
        self.wait_for_option_account_funding(client, contract, request, amount)
            .await
    }

    async fn wait_for_option_account_funding(
        &self,
        client: &BinanceClient,
        contract: &OptionContract,
        request: &StrategyRequest,
        transferred_amount: Decimal,
    ) -> AppResult<OptionAccountSnapshot> {
        // 划转后 Binance 账户快照可能短暂延迟；等待期间只查询账户，不重复发送资金动作。
        let mut account = client.option_account().await?;
        for attempt_index in 0..OPTION_ACCOUNT_FUNDING_REFRESH_ATTEMPTS {
            if option_funding_shortfall(&account, contract, request) <= Decimal::ZERO {
                if attempt_index > 0 {
                    self.record_log(
                        "funding.option_account_refreshed",
                        "期权保证金账户已反映本次划转，继续执行下单风控",
                    )?;
                }
                return Ok(account);
            }
            if attempt_index + 1 == OPTION_ACCOUNT_FUNDING_REFRESH_ATTEMPTS {
                break;
            }
            tokio::time::sleep(StdDuration::from_millis(
                OPTION_ACCOUNT_FUNDING_REFRESH_DELAY_MS,
            ))
            .await;
            account = client.option_account().await?;
        }

        let remaining =
            round_up_funding_amount(option_funding_shortfall(&account, contract, request));
        self.record_warning(
            "funding.option_account_refresh_timeout",
            &format!(
                "已划转 {transferred_amount} USDT 用于期权下单，但期权账户快照仍显示资金或权益缺口约 {remaining} USDT，将按最新账户快照继续风控"
            ),
        )?;
        Ok(account)
    }

    /// 现货余额不足时，从 Simple Earn 活期赎回缺口。
    async fn ensure_spot_asset_available(
        &self,
        client: &BinanceClient,
        asset: &str,
        required: Decimal,
        purpose: &str,
        chain_id: &str,
    ) -> AppResult<()> {
        if required <= Decimal::ZERO {
            return Ok(());
        }
        let spot_account = client.spot_account().await?;
        let available = spot_asset_free(&spot_account, asset);
        if available >= required {
            return Ok(());
        }

        let redeem_amount = round_up_funding_amount(required - available);
        self.redeem_flexible_asset(client, asset, redeem_amount, purpose, chain_id)
            .await
    }

    async fn redeem_flexible_asset(
        &self,
        client: &BinanceClient,
        asset: &str,
        amount: Decimal,
        purpose: &str,
        chain_id: &str,
    ) -> AppResult<()> {
        if amount <= Decimal::ZERO {
            return Ok(());
        }
        let earn = client.simple_earn_snapshot().await?;
        // 多个活期产品可赎回时优先选余额最大的产品，减少同一资金链路的请求次数。
        let position = earn
            .balances
            .iter()
            .filter(|balance| {
                balance.flexible
                    && balance.asset == asset
                    && balance.amount > Decimal::ZERO
                    && balance.can_redeem
                    && !balance.product_id.trim().is_empty()
            })
            .max_by(|left, right| left.amount.cmp(&right.amount))
            .ok_or_else(|| {
                AppError::RiskRejected(format!(
                    "现货 {asset} 不足，且没有可赎回的 Simple Earn 活期持仓"
                ))
            })?;
        if position.amount < amount {
            return Err(AppError::RiskRejected(format!(
                "Simple Earn 活期 {asset} 可赎回数量不足，预计需要 {}，当前可赎回 {}",
                amount, position.amount
            )));
        }

        self.execute_simple_earn_redeem(
            client,
            chain_id,
            purpose,
            &position.product_id,
            asset,
            amount,
        )
        .await?;
        self.record_log(
            "funding.earn.redeemed",
            &format!("已从 Simple Earn 活期赎回 {amount} {asset} 至现货账户，用于{purpose}"),
        )
    }

    async fn sweep_option_idle_usdt_best_effort(&self, client: &BinanceClient, purpose: &str) {
        if self.settlement_reserves_assets().await.unwrap_or(true) {
            return;
        }
        // 归集是收尾优化，失败时只写脱敏告警，不回滚已经完成的下单或结算事实。
        let chain_id = Uuid::new_v4().to_string();
        match self
            .sweep_option_idle_usdt(client, purpose, &chain_id)
            .await
        {
            Ok(
                SweepExecutionOutcome::Completed(message) | SweepExecutionOutcome::Skipped(message),
            ) => {
                let _ = self.record_log("funding.sweep.finished", &message);
            }
            Ok(SweepExecutionOutcome::Waiting(message)) => {
                let _ = self.record_warning("funding.sweep.waiting", &message);
            }
            Err(error) => {
                let _ =
                    self.record_warning("funding.sweep.failed", &format!("{purpose}失败：{error}"));
            }
        }
    }

    /// 把期权账户中不需要继续支撑保证金风控的 USDT 归集回现货并申购活期。
    async fn sweep_option_idle_usdt(
        &self,
        client: &BinanceClient,
        purpose: &str,
        chain_id: &str,
    ) -> AppResult<SweepExecutionOutcome> {
        let chain_attempts = self.funding_attempts_by_chain(chain_id)?;
        if let Some(subscription) = chain_attempts.iter().find(|attempt| {
            attempt.kind == FundingAttemptKind::SimpleEarnSubscribe && attempt.asset == "USDT"
        }) {
            return match subscription.status {
                FundingAttemptStatus::Confirmed => Ok(SweepExecutionOutcome::Completed(
                    "该归集链路的 Simple Earn 活期申购已确认完成".to_string(),
                )),
                FundingAttemptStatus::Failed => Err(AppError::RiskRejected(
                    "该归集链路已有失败的 Simple Earn 申购记录，需要人工复核".to_string(),
                )),
                _ => Err(AppError::RiskRejected(
                    "该归集链路已有待确认的 Simple Earn 申购记录".to_string(),
                )),
            };
        }

        let (transfer_amount, minimum_spot_amount) = if let Some(transfer) =
            chain_attempts.iter().find(|attempt| {
                attempt.kind == FundingAttemptKind::UniversalTransfer
                    && attempt.asset == "USDT"
                    && attempt.transfer_type.as_deref() == Some("OPTION_MAIN")
            }) {
            match transfer.status {
                FundingAttemptStatus::Confirmed => (transfer.amount, transfer.amount),
                FundingAttemptStatus::Failed => {
                    return Err(AppError::RiskRejected(
                        "该归集链路已有失败的 OPTION_MAIN 划转记录，需要人工复核".to_string(),
                    ));
                }
                _ => {
                    return Err(AppError::RiskRejected(
                        "该归集链路已有待确认的 OPTION_MAIN 划转记录".to_string(),
                    ));
                }
            }
        } else {
            let account = client.option_account().await?;
            let amount = round_down_funding_amount(option_idle_usdt_sweep_amount(&account));
            if amount < OPTION_IDLE_USDT_SWEEP_MIN_AMOUNT {
                let message = format!(
                    "{purpose}跳过期权 USDT 归集：可归集数量 {amount} USDT 小于自动归集阈值 {OPTION_IDLE_USDT_SWEEP_MIN_AMOUNT} USDT"
                );
                self.record_log("funding.option_idle_sweep.skipped", &message)?;
                return Ok(SweepExecutionOutcome::Skipped(message));
            }

            let spot_before = client.spot_account().await?;
            let minimum_spot_amount = spot_asset_free(&spot_before, "USDT") + amount;
            self.execute_universal_transfer(
                client,
                chain_id,
                purpose,
                "OPTION_MAIN",
                "USDT",
                amount,
            )
            .await?;
            self.record_log(
                "funding.transfer.option_main",
                &format!("已从期权保证金账户归集 {amount} USDT 到现货账户，{purpose}"),
            )?;
            (amount, minimum_spot_amount)
        };

        if !self
            .wait_for_spot_asset_available(client, "USDT", minimum_spot_amount)
            .await?
        {
            let message = format!(
                "{purpose}已确认 OPTION_MAIN 划转，但现货账户暂未反映 {transfer_amount} USDT，延后 Simple Earn 申购"
            );
            self.record_warning("funding.earn.subscribe_deferred", &message)?;
            return Ok(SweepExecutionOutcome::Waiting(message));
        }

        self.subscribe_spot_asset_to_earn(client, "USDT", transfer_amount, purpose, chain_id)
            .await?;
        Ok(SweepExecutionOutcome::Completed(format!(
            "{purpose}已完成期权闲置 USDT 归集链路"
        )))
    }

    async fn wait_for_spot_asset_available(
        &self,
        client: &BinanceClient,
        asset: &str,
        expected_amount: Decimal,
    ) -> AppResult<bool> {
        for attempt_index in 0..SPOT_BALANCE_CONFIRM_ATTEMPTS {
            let spot_account = client.spot_account().await?;
            if spot_asset_free(&spot_account, asset) >= expected_amount {
                if attempt_index > 0 {
                    self.record_log(
                        "funding.spot_balance_refreshed",
                        &format!("现货账户已反映 {expected_amount} {asset}，继续后续资金收尾动作"),
                    )?;
                }
                return Ok(true);
            }
            if attempt_index + 1 == SPOT_BALANCE_CONFIRM_ATTEMPTS {
                break;
            }
            tokio::time::sleep(StdDuration::from_millis(SPOT_BALANCE_CONFIRM_DELAY_MS)).await;
        }
        Ok(false)
    }

    async fn subscribe_spot_asset_to_earn(
        &self,
        client: &BinanceClient,
        asset: &str,
        amount: Decimal,
        purpose: &str,
        chain_id: &str,
    ) -> AppResult<()> {
        let amount = round_down_funding_amount(amount);
        if amount <= Decimal::ZERO {
            return Ok(());
        }
        if let Some(subscription) =
            self.funding_attempts_by_chain(chain_id)?
                .into_iter()
                .find(|attempt| {
                    attempt.kind == FundingAttemptKind::SimpleEarnSubscribe
                        && attempt.asset == asset
                })
        {
            return match subscription.status {
                FundingAttemptStatus::Confirmed => {
                    self.record_log(
                        "funding.earn.subscribe_already_confirmed",
                        &format!("{purpose}已存在确认完成的 {asset} 活期申购记录，跳过重复申购"),
                    )?;
                    Ok(())
                }
                FundingAttemptStatus::Failed => Err(AppError::RiskRejected(format!(
                    "{purpose}已有失败的 {asset} 活期申购记录，需要人工复核"
                ))),
                _ => Err(AppError::RiskRejected(format!(
                    "{purpose}已有待确认的 {asset} 活期申购记录"
                ))),
            };
        }
        let products = client.simple_earn_flexible_products(asset).await?;
        let product = products
            .into_iter()
            .find(|product| {
                product.asset == asset
                    && product.can_purchase
                    && !product.is_sold_out
                    && product.status == "PURCHASING"
            })
            .ok_or_else(|| {
                AppError::Binance(format!("未找到可申购的 Simple Earn 活期 {asset} 产品"))
            })?;
        if amount < product.min_purchase_amount {
            self.record_log(
                "funding.earn.subscribe_skipped",
                &format!(
                    "{purpose}跳过 {asset} 活期申购：数量 {} 小于最小申购 {}",
                    amount, product.min_purchase_amount
                ),
            )?;
            return Ok(());
        }

        // Simple Earn 活期申购/赎回限制为同账户约 3 秒一次；归集是下单后的收尾动作，优先避免触发限频。
        tokio::time::sleep(StdDuration::from_secs(3)).await;
        self.execute_simple_earn_subscribe(
            client,
            chain_id,
            purpose,
            &product.product_id,
            asset,
            amount,
        )
        .await?;
        self.record_log(
            "funding.earn.subscribed",
            &format!("已把 {amount} {asset} 从现货账户重新申购 Simple Earn 活期，{purpose}"),
        )
    }
}
