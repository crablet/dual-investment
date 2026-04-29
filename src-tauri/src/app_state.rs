//! 应用状态编排层。
//!
//! 这个模块连接 Tauri command、Binance 客户端、策略计算和 `SQLite` 持久化。
//! 它负责实盘连接、快照聚合、下单门禁、订单恢复、到期结算、现货补齐和后台轮询。
//! 业务计算放在 `strategy.rs`，交易所请求放在 `binance.rs`，凭据读写放在 `secrets.rs`。

mod adoption;
mod funding;
mod orders;
mod runtime;
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

use chrono::{Duration, Local, NaiveDate, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex as AsyncMutex, RwLock};
use uuid::Uuid;

use self::adoption::{
    adoptable_position_view, ensure_position_can_be_adopted, is_short_position,
    normalized_unique_symbols,
};
use self::funding::{
    flexible_earn_asset_amount, option_funding_shortfall, option_idle_usdt_sweep_amount,
    round_down_funding_amount, round_up_funding_amount, spot_asset_free,
    spot_rebalance_required_asset,
};
use self::orders::{
    apply_option_ack, apply_option_position, apply_spot_ack, combined_balances, is_spot_attempt,
    is_unfilled_terminal_option_ack, request_from_option_attempt,
};
use self::runtime::{
    client_from_runtime, join_binance_request, make_client_order_id, make_spot_client_order_id,
    parse_schedule_time, spawn_binance_request, verify_binance_access,
};
use self::validation::{
    enforce_risk_rules, has_exchange_order_ref, is_in_the_money, is_trackable_order,
    is_uncertain_submit_error, resolve_contract, should_consume_auto_day_after_error,
    should_settle_order, should_sync_option_order, validate_auto_rule, validate_contract_order,
    validate_contract_terms, validate_request, validate_spot_rebalance,
};
use crate::binance::BinanceClient;
use crate::domain::{
    AdoptOptionPositionsRequest, AdoptableOptionPositionView, AppError, AppResult, AppSettings,
    AppSnapshot, AutoInvestmentRule, CandidateScanResult, ConnectBinanceRequest, FundingAttempt,
    FundingAttemptKind, FundingAttemptManualResolution, FundingAttemptStatus, FundingAttemptView,
    LogLevel, LogSnapshot, OptionAccountSnapshot, OptionContract, OrderAttempt, OrderAttemptStatus,
    OrderAttemptView, OrderProgressEvent, OrderProgressStage, OrderProgressStatus,
    ResolveFundingAttemptRequest, RuntimeHealth, RuntimeHealthStatus, SimpleEarnRedemptionRecord,
    SimpleEarnSnapshot, SimpleEarnSubscriptionRecord, SpotAccountSnapshot, StrategyCandidate,
    StrategyDirection, StrategyOrder, StrategyRequest, StrategyStatus, ToggleAutoRequest,
    UniversalTransferRecord,
};
use crate::logs::LogStore;
use crate::persistence::Persistence;
use crate::secrets;
use crate::strategy;

struct LiveAccounts {
    option: OptionAccountSnapshot,
    spot: SpotAccountSnapshot,
    earn: SimpleEarnSnapshot,
    spot_prices: HashMap<String, Decimal>,
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

#[derive(Debug, Default)]
struct SettlementSweepPlan {
    option_idle_usdt_needed: bool,
    settled_symbols: Vec<String>,
    deferred_reasons: Vec<String>,
}

impl SettlementSweepPlan {
    fn request_option_idle_usdt(&mut self, option_symbol: &str) {
        self.option_idle_usdt_needed = true;
        self.settled_symbols.push(option_symbol.to_string());
    }

    fn defer(&mut self, option_symbol: &str, reason: impl Into<String>) {
        self.deferred_reasons
            .push(format!("{option_symbol}：{}", reason.into()));
    }

    fn has_deferred_work(&self) -> bool {
        !self.deferred_reasons.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
struct RuntimeHealthState {
    consecutive_failures: u32,
    skipped_ticks: u32,
}

const ORDER_PROGRESS_EVENT: &str = "order-progress";
const FUNDING_HISTORY_LOOKBACK_MINUTES: i64 = 5;
const FUNDING_HISTORY_LOOKAHEAD_DAYS: i64 = 30;
const FUNDING_QUERY_RETRY_LIMIT: u32 = 3;
const OPTION_ACCOUNT_FUNDING_REFRESH_ATTEMPTS: usize = 6;
const OPTION_ACCOUNT_FUNDING_REFRESH_DELAY_MS: u64 = 700;

struct OrderProgressEmitter {
    app: Option<AppHandle>,
    flow_id: Option<String>,
    source: String,
}

impl OrderProgressEmitter {
    fn manual(app: AppHandle, request: &StrategyRequest) -> Self {
        Self {
            app: Some(app),
            flow_id: request.client_flow_id.clone(),
            source: "manual".to_string(),
        }
    }

    fn disabled(source: &str) -> Self {
        Self {
            app: None,
            flow_id: None,
            source: source.to_string(),
        }
    }

    /// 向前端发送脱敏进度事件。
    ///
    /// 自动策略没有前端 flow id，因此使用 disabled emitter 静默执行。
    fn emit(
        &self,
        stage: OrderProgressStage,
        status: OrderProgressStatus,
        message: impl Into<String>,
        option_symbol: Option<&str>,
        exchange_order_id: Option<&str>,
    ) {
        let (Some(app), Some(flow_id)) = (&self.app, &self.flow_id) else {
            return;
        };
        let event = OrderProgressEvent {
            flow_id: flow_id.clone(),
            source: self.source.clone(),
            stage,
            status,
            message: message.into(),
            timestamp: Utc::now(),
            option_symbol: option_symbol.map(str::to_string),
            exchange_order_id: exchange_order_id.map(str::to_string),
        };
        let _ = app.emit(ORDER_PROGRESS_EVENT, event);
    }
}

fn funding_history_window(created_at: chrono::DateTime<Utc>) -> (i64, i64) {
    // 历史查询窗口从本地写入恢复记录的时间开始，兼顾接口延迟又限制扫描范围。
    let start = created_at - Duration::minutes(FUNDING_HISTORY_LOOKBACK_MINUTES);
    let max_end = start + Duration::days(FUNDING_HISTORY_LOOKAHEAD_DAYS);
    let near_now = Utc::now() + Duration::minutes(5);
    let mut end = if near_now < max_end {
        near_now
    } else {
        max_end
    };
    if end <= start {
        end = start + Duration::minutes(1);
    }
    (start.timestamp_millis(), end.timestamp_millis())
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

#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct RuntimeConfig {
    pub connected: bool,
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub saved_credentials: bool,
    pub auto_enabled: bool,
    pub quick_execution: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            connected: false,
            api_key: None,
            api_secret: None,
            saved_credentials: secrets::has_binance_credentials(),
            auto_enabled: false,
            quick_execution: true,
        }
    }
}

impl RuntimeConfig {
    /// 转成前端可见设置，过滤掉明文 API Key 和 Secret。
    pub fn settings(&self) -> AppSettings {
        AppSettings {
            connected: self.connected,
            saved_credentials: self.saved_credentials,
            auto_enabled: self.auto_enabled,
            quick_execution: self.quick_execution,
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
    auto_rule: Arc<RwLock<AutoInvestmentRule>>,
    orders: Arc<RwLock<Vec<StrategyOrder>>>,
    persistence: Arc<Mutex<Persistence>>,
    logs: Arc<LogStore>,
    worker_started: Arc<AtomicBool>,
    last_auto_scan_day: Arc<RwLock<Option<NaiveDate>>>,
    health: Arc<RwLock<RuntimeHealthState>>,
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
    /// 启动时只加载仍可跟踪的订单，包括没有原始订单号的接管订单；自动策略强制关闭，
    /// 避免应用重启后直接自动下单。
    fn from_persistence(persistence: Persistence, logs: LogStore) -> AppResult<Self> {
        let orders = persistence
            .list_orders()?
            .into_iter()
            .filter(is_trackable_order)
            .collect();
        let mut auto_rule = persistence
            .load_auto_rule()?
            .unwrap_or_else(|| strategy::default_auto_rule(false));
        auto_rule.enabled = false;

        let state = Self {
            runtime: Arc::new(RwLock::new(RuntimeConfig::default())),
            client: Arc::new(RwLock::new(None)),
            candidates: Arc::new(RwLock::new(Vec::new())),
            auto_rule: Arc::new(RwLock::new(auto_rule)),
            orders: Arc::new(RwLock::new(orders)),
            persistence: Arc::new(Mutex::new(persistence)),
            logs: Arc::new(logs),
            worker_started: Arc::new(AtomicBool::new(false)),
            last_auto_scan_day: Arc::new(RwLock::new(None)),
            health: Arc::new(RwLock::new(RuntimeHealthState::default())),
            maintenance_lock: Arc::new(AsyncMutex::new(())),
            trade_lock: Arc::new(AsyncMutex::new(())),
        };
        state.record_log("app.started", "应用已启动，等待 Binance API 连接")?;
        Ok(state)
    }

    /// 聚合前端所需完整快照。
    ///
    /// 已连接时会实时读取 Binance 账户和现货价格；未连接时返回本地缓存和明确的空风险/收益状态。
    pub async fn snapshot(&self) -> AppResult<AppSnapshot> {
        let settings = self.runtime.read().await.settings();
        let auto_rule = self.auto_rule.read().await.clone();
        let candidates = self.candidates.read().await.clone();
        let orders = self.orders.read().await.clone();
        let accounts = self.live_accounts_if_connected().await?;
        let (order_attempts, funding_attempts, pending_recovery_count) = {
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
            let pending_recovery_count = persistence.count_pending_order_attempts()?
                + persistence.count_pending_funding_attempts()?;
            (attempts, funding_attempts, pending_recovery_count)
        };
        let health = self
            .health_snapshot(settings.connected, pending_recovery_count)
            .await;
        let option_account = accounts.as_ref().map(|accounts| &accounts.option);
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
            health,
            pnl: strategy::pnl_report_from_orders_with_spot(&orders, spot_price),
            risk: option_account.map_or_else(
                strategy::empty_risk_snapshot,
                strategy::risk_snapshot_from_account,
            ),
            settings,
            auto_rule,
            orders,
            order_attempts,
            funding_attempts,
        })
    }

    /// 使用用户输入凭据连接 Binance，并保存到系统 keyring。
    ///
    /// 连接阶段同时验证期权和现货账户权限，因为到期 ITM 后需要真实现货补齐能力。
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

        self.ensure_background_worker();
        self.record_log("account.connected", "已连接 Binance 账户")?;
        Ok(settings)
    }

    /// 使用系统 keyring 中的凭据连接 Binance。
    pub async fn connect_saved_binance(&self) -> AppResult<AppSettings> {
        let result = self.connect_saved_binance_inner().await;
        if let Err(error) = &result {
            let _ = self.record_warning("account.connect_saved_failed", &error.to_string());
        }
        result
    }

    async fn connect_saved_binance_inner(&self) -> AppResult<AppSettings> {
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

        self.ensure_background_worker();
        self.record_log(
            "account.connected.saved",
            "已使用系统 keyring 凭据连接 Binance 账户",
        )?;
        Ok(settings)
    }

    /// 删除系统 keyring 凭据并清空运行期连接状态。
    pub async fn forget_binance_credentials(&self) -> AppResult<AppSettings> {
        secrets::delete_binance_credentials()?;
        let mut runtime = self.runtime.write().await;
        runtime.connected = false;
        runtime.api_key = None;
        runtime.api_secret = None;
        runtime.auto_enabled = false;
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
        {
            let mut rule = self.auto_rule.write().await;
            rule.enabled = false;
        }
        self.record_log(
            "account.credentials.deleted",
            "已从系统 keyring 删除 Binance API 凭据",
        )?;
        Ok(settings)
    }

    /// 从 Binance 实盘数据扫描候选，并缓存给前端快照使用。
    ///
    /// 账户、合约、mark price 和现货价格并发读取，减少一次刷新候选的等待时间。
    pub async fn scan_candidates(&self) -> AppResult<CandidateScanResult> {
        let client = self.live_client().await?;
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
        let spot_account_task = spawn_binance_request({
            let client = client.clone();
            async move { client.spot_account().await }
        });
        let earn_task = spawn_binance_request({
            let client = client.clone();
            async move { client.simple_earn_snapshot().await }
        });
        let spot_price_task = spawn_binance_request({
            let client = client.clone();
            async move { client.spot_price("BTCUSDT").await }
        });
        let auto_rule = self.auto_rule.read().await.clone();
        let orders = self.orders.read().await.clone();
        let account = join_binance_request(account_task).await;
        let contracts = join_binance_request(contracts_task).await;
        let marks = join_binance_request(marks_task).await;
        let spot_account = join_binance_request(spot_account_task).await;
        let earn = join_binance_request(earn_task).await;
        let spot_price = join_binance_request(spot_price_task).await;
        let account = account?;
        let contracts = contracts?;
        let marks = marks?;
        let spot_account = spot_account?;
        let earn = earn?;
        let spot_price = spot_price?;
        let funding_available_usdt =
            spot_asset_free(&spot_account, "USDT") + flexible_earn_asset_amount(&earn, "USDT");
        let result = strategy::build_candidates(
            &contracts,
            &marks,
            &auto_rule,
            &account,
            &orders,
            spot_price,
            funding_available_usdt,
        );
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
                request.option_symbol.as_deref(),
                None,
            );
            return Err(AppError::RiskRejected("手动下单需要显式确认".to_string()));
        }
        if self.pending_recovery_count()? > 0 {
            progress.emit(
                OrderProgressStage::Validate,
                OrderProgressStatus::Error,
                "存在待确认恢复记录，已暂停新的真实下单",
                request.option_symbol.as_deref(),
                None,
            );
            return Err(AppError::RiskRejected(
                "存在待确认恢复记录，已暂停新的真实下单".to_string(),
            ));
        }
        self.place_live_strategy(request, "manual", &progress).await
    }

    /// 保存自动复投规则。
    ///
    /// 保存规则不改变自动策略开启状态；开启/关闭必须走 `toggle_auto_strategy`。
    pub async fn save_auto_rule(&self, rule: AutoInvestmentRule) -> AppResult<AutoInvestmentRule> {
        let mut rule = rule;
        validate_auto_rule(&rule)?;
        rule.enabled = self.runtime.read().await.auto_enabled;
        {
            let mut stored = self.auto_rule.write().await;
            *stored = rule.clone();
        }
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .save_auto_rule(&rule)?;
        self.record_log("strategy.rule.saved", "自动定投规则已保存")?;
        Ok(rule)
    }

    /// 开启或暂停自动复投。
    ///
    /// 开启时必须已连接 Binance，并且请求里带有二次确认。
    pub async fn toggle_auto_strategy(&self, request: ToggleAutoRequest) -> AppResult<AppSettings> {
        let mut runtime = self.runtime.write().await;
        if request.enabled && !runtime.connected {
            return Err(AppError::Config("请先连接 Binance API".to_string()));
        }
        if request.enabled && !request.confirm_live {
            return Err(AppError::RiskRejected(
                "自动策略开启需要二次确认".to_string(),
            ));
        }
        if request.enabled && self.pending_recovery_count()? > 0 {
            return Err(AppError::RiskRejected(
                "存在待确认恢复记录，已暂停开启自动策略".to_string(),
            ));
        }
        runtime.auto_enabled = request.enabled;
        let settings = runtime.settings();
        drop(runtime);

        {
            let mut rule = self.auto_rule.write().await;
            rule.enabled = request.enabled;
            self.persistence
                .lock()
                .map_err(|error| AppError::Storage(error.to_string()))?
                .save_auto_rule(&rule)?;
        }
        if request.enabled {
            self.ensure_background_worker();
        }
        self.record_log(
            "strategy.auto.toggled",
            if request.enabled {
                "自动定投已启用，后台调度器会按规则扫描并自动下单"
            } else {
                "自动定投已暂停"
            },
        )?;
        Ok(settings)
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
            .list_order_attempts(500)?
            .into_iter()
            .find(|attempt| attempt.id == attempt_id)
            .ok_or_else(|| AppError::Storage("未找到指定恢复记录".to_string()))?;

        if is_spot_attempt(attempt.status) {
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
        let client = self.live_client().await?;
        let attempt = self
            .persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .list_funding_attempts(500)?
            .into_iter()
            .find(|attempt| attempt.id == attempt_id)
            .ok_or_else(|| AppError::Storage("未找到指定资金恢复记录".to_string()))?;

        let _trade_guard = self.trade_lock.lock().await;
        let recovered = self.recover_funding_attempt_inner(&client, attempt).await?;
        Ok(FundingAttemptView::from(recovered))
    }

    /// 人工核对 Binance 事实后解除已阻断的资金恢复记录。
    pub fn resolve_funding_attempt(
        &self,
        request: &ResolveFundingAttemptRequest,
    ) -> AppResult<FundingAttemptView> {
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
            .list_funding_attempts(500)?
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

    /// 同步订单、执行到期结算，然后返回完整快照。
    pub async fn sync_live_state(&self) -> AppResult<AppSnapshot> {
        let _maintenance_guard = self.maintenance_lock.lock().await;
        self.recover_due_funding_attempts().await?;
        self.sync_live_orders().await?;
        if self.pending_recovery_count()? == 0 {
            self.settle_due_orders().await?;
        }
        self.snapshot().await
    }

    /// 手动触发到期结算扫描。
    pub async fn run_settlement_scan(&self) -> AppResult<Vec<StrategyOrder>> {
        let _maintenance_guard = self.maintenance_lock.lock().await;
        self.recover_due_funding_attempts().await?;
        if self.pending_recovery_count()? > 0 {
            return Err(AppError::RiskRejected(
                "存在待确认资金恢复记录，已暂停结算资金动作".to_string(),
            ));
        }
        self.sync_live_orders().await?;
        self.settle_due_orders().await
    }

    /// 手动执行一次自动策略。
    ///
    /// 如果存在推荐候选，会直接发送 Binance 订单，因此必须要求自动策略已启用。
    pub async fn run_auto_once(&self) -> AppResult<Option<StrategyOrder>> {
        let runtime = self.runtime.read().await.clone();
        if !runtime.connected {
            return Err(AppError::Config("请先连接 Binance API".to_string()));
        }
        if !runtime.auto_enabled {
            return Err(AppError::RiskRejected("自动定投未启用".to_string()));
        }
        let _maintenance_guard = self.maintenance_lock.lock().await;
        self.recover_due_funding_attempts().await?;
        if self.pending_recovery_count()? > 0 {
            return Err(AppError::RiskRejected(
                "存在待确认恢复记录，已暂停自动策略".to_string(),
            ));
        }
        self.execute_auto_strategy().await
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

    async fn health_snapshot(
        &self,
        connected: bool,
        pending_recovery_count: usize,
    ) -> RuntimeHealth {
        let health = self.health.read().await.clone();
        // health 是运行观测信号：恢复记录和连续失败会提升状态，但不改变交易事实。
        let status = if !connected {
            RuntimeHealthStatus::Idle
        } else if pending_recovery_count > 0 || health.consecutive_failures >= 3 {
            RuntimeHealthStatus::Attention
        } else if health.consecutive_failures > 0 || health.skipped_ticks > 0 {
            RuntimeHealthStatus::Degraded
        } else {
            RuntimeHealthStatus::Healthy
        };
        RuntimeHealth {
            status,
            consecutive_failures: health.consecutive_failures,
            skipped_ticks: health.skipped_ticks,
            pending_recovery_count,
        }
    }

    async fn record_background_tick_success(&self) {
        // 成功完成一整轮维护后清空失败/跳过计数，避免旧错误持续污染健康状态。
        let mut health = self.health.write().await;
        health.consecutive_failures = 0;
        health.skipped_ticks = 0;
    }

    async fn record_background_tick_failure(&self) {
        // 下次 tick 仍会继续尝试，除非恢复记录阻断自动动作。
        let mut health = self.health.write().await;
        health.consecutive_failures = health.consecutive_failures.saturating_add(1);
    }

    async fn record_background_tick_skipped(&self) -> AppResult<()> {
        let skipped_ticks = {
            let mut health = self.health.write().await;
            health.skipped_ticks = health.skipped_ticks.saturating_add(1);
            health.skipped_ticks
        };
        self.record_warning(
            "background.tick_skipped",
            &format!("上一轮后台同步仍在运行，本轮跳过，连续跳过 {skipped_ticks} 次"),
        )
    }

    /// 执行真实期权下单的核心流程。
    ///
    /// 顺序是：请求校验、读取实盘数据、合约匹配、风控校验、写入恢复记录、发送 Binance 订单、
    /// 保存本地订单。交易所接单后如果本地保存失败，会保留恢复记录并返回明确错误。
    #[allow(clippy::too_many_lines)]
    async fn place_live_strategy(
        &self,
        mut request: StrategyRequest,
        source: &str,
        progress: &OrderProgressEmitter,
    ) -> AppResult<StrategyOrder> {
        progress.emit(
            OrderProgressStage::Validate,
            OrderProgressStatus::Running,
            "正在校验下单参数",
            request.option_symbol.as_deref(),
            None,
        );
        if let Err(error) = validate_request(&request) {
            progress.emit(
                OrderProgressStage::Validate,
                OrderProgressStatus::Error,
                error.to_string(),
                request.option_symbol.as_deref(),
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
                    request.option_symbol.as_deref(),
                    None,
                );
                return Err(error);
            }
        };
        progress.emit(
            OrderProgressStage::Validate,
            OrderProgressStatus::Running,
            "正在读取 Binance 账户、合约和行情",
            request.option_symbol.as_deref(),
            None,
        );
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
        let mut account = match account {
            Ok(account) => account,
            Err(error) => {
                progress.emit(
                    OrderProgressStage::Validate,
                    OrderProgressStatus::Error,
                    error.to_string(),
                    request.option_symbol.as_deref(),
                    None,
                );
                return Err(error);
            }
        };
        let contracts = match contracts {
            Ok(contracts) => contracts,
            Err(error) => {
                progress.emit(
                    OrderProgressStage::Validate,
                    OrderProgressStatus::Error,
                    error.to_string(),
                    request.option_symbol.as_deref(),
                    None,
                );
                return Err(error);
            }
        };
        let marks = match marks {
            Ok(marks) => marks,
            Err(error) => {
                progress.emit(
                    OrderProgressStage::Validate,
                    OrderProgressStatus::Error,
                    error.to_string(),
                    request.option_symbol.as_deref(),
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
                    request.option_symbol.as_deref(),
                    None,
                );
                return Err(error);
            }
        };
        request.option_symbol = Some(contract.symbol.clone());
        request.expiry_at = contract.expiry_at;
        request.limit_price = strategy::option_sell_limit_price(request.limit_price, contract);
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
            &format!("strategy.order.{source}.attempted"),
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
                        &format!("strategy.order.{source}.submit_uncertain"),
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
                &format!("strategy.order.{source}.failed"),
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
            &format!("strategy.order.{source}.created"),
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
                .find(|order| {
                    order.spot_client_order_id == attempt.client_order_id
                        || order.option_symbol == attempt.option_symbol
                })
                .cloned()
        }
        .ok_or_else(|| {
            AppError::Storage("未找到现货补齐对应的期权订单记录，无法自动恢复".to_string())
        })?;

        order.spot_order_id = Some(ack.order_id.clone());
        order.spot_client_order_id = attempt.client_order_id.clone();
        apply_spot_ack(&mut order, &ack);
        self.save_order(&order).await?;
        attempt.status = OrderAttemptStatus::SpotPersisted;
        attempt.exchange_order_id = Some(ack.order_id.clone());
        attempt.message = "已从 Binance 查询现货订单并恢复补齐状态".to_string();
        attempt.updated_at = Utc::now();
        self.save_order_attempt(&attempt)?;
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

    /// 扫描已经到期的订单并执行结算/补齐。
    ///
    /// 未成交订单会直接过期；OTM 订单结算完成；ITM 订单进入现货补齐流程。
    async fn settle_due_orders(&self) -> AppResult<Vec<StrategyOrder>> {
        let client = self.live_client().await?;
        let orders = self.orders.read().await.clone();
        let mut updated = Vec::new();
        let mut sweep_plan = SettlementSweepPlan::default();

        for mut order in orders.into_iter().filter(should_settle_order) {
            if order.filled_quantity <= Decimal::ZERO {
                self.delete_order(&order.id).await?;
                self.record_log(
                    "strategy.settlement.unfilled_removed",
                    &format!("{} 未成交，到期扫描已从本地订单移除", order.option_symbol),
                )?;
                continue;
            }

            let settlement_price = match self.settlement_price(&client, &order).await {
                Ok(Some(price)) => price,
                Ok(None) => {
                    sweep_plan.defer(&order.option_symbol, "Binance 行权记录暂未给出结算价");
                    continue;
                }
                Err(error) => {
                    self.record_warning(
                        "strategy.settlement.price_failed",
                        &format!("{} 结算价读取失败：{}", order.option_symbol, error),
                    )?;
                    sweep_plan.defer(&order.option_symbol, "结算价读取失败，等待下一轮重试");
                    continue;
                }
            };
            order.settlement_price = settlement_price;
            if !is_in_the_money(&order, settlement_price) {
                order.status = StrategyStatus::Settled;
                order.settled_at = Some(Utc::now());
                order.updated_at = Utc::now();
                order.rebalance_message = format!(
                    "到期未触发换币，结算参考价 {}",
                    settlement_price.round_dp(2)
                );
                self.save_order(&order).await?;
                self.record_log(
                    "strategy.settlement.otm",
                    &format!("{} 到期 OTM，无需现货补齐", order.option_symbol),
                )?;
                sweep_plan.request_option_idle_usdt(&order.option_symbol);
                updated.push(order);
                continue;
            }

            let option_symbol = order.option_symbol.clone();
            match self.rebalance_spot_if_needed(&client, order).await {
                Ok(order) => {
                    if order.status == StrategyStatus::Settled {
                        sweep_plan.request_option_idle_usdt(&order.option_symbol);
                    } else {
                        sweep_plan.defer(&order.option_symbol, "现货补齐未完成，暂不归集期权资金");
                    }
                    updated.push(order);
                }
                Err(error) => {
                    self.record_warning(
                        "strategy.rebalance.scan_failed",
                        &format!("到期补齐流程失败：{error}"),
                    )?;
                    sweep_plan.defer(&option_symbol, "现货补齐流程失败，等待人工或下一轮恢复");
                }
            }
        }
        self.sweep_settlement_option_idle_usdt_once(&client, &sweep_plan)
            .await?;
        Ok(updated)
    }

    async fn sweep_settlement_option_idle_usdt_once(
        &self,
        client: &BinanceClient,
        plan: &SettlementSweepPlan,
    ) -> AppResult<()> {
        if !plan.option_idle_usdt_needed {
            return Ok(());
        }
        if plan.has_deferred_work() {
            self.record_warning(
                "funding.settlement_sweep.deferred",
                &format!(
                    "本轮到期结算仍有未完成项，暂不归集期权账户 USDT：{}",
                    plan.deferred_reasons.join("；")
                ),
            )?;
            return Ok(());
        }
        if self.pending_recovery_count()? > 0 {
            self.record_warning(
                "funding.settlement_sweep.deferred",
                "存在待确认恢复记录，暂不执行本轮结算后的期权 USDT 归集",
            )?;
            return Ok(());
        }

        let _trade_guard = self.trade_lock.lock().await;
        if self.pending_recovery_count()? > 0 {
            self.record_warning(
                "funding.settlement_sweep.deferred",
                "等待归集锁期间出现待确认恢复记录，已跳过本轮结算后的期权 USDT 归集",
            )?;
            return Ok(());
        }
        self.record_log(
            "funding.settlement_sweep.started",
            &format!(
                "本轮到期结算已完成，准备归集期权账户闲置 USDT，涉及 {}",
                plan.settled_symbols.join("、")
            ),
        )?;
        self.sweep_option_idle_usdt_best_effort(client, "期权到期结算后归集")
            .await;
        Ok(())
    }

    async fn settlement_price(
        &self,
        client: &BinanceClient,
        order: &StrategyOrder,
    ) -> AppResult<Option<Decimal>> {
        // 到期结算价只来自 Binance 行权记录。现货价不能替代，否则会污染现金结算事实。
        let expiry_at = order.expiry_at;
        let start = (expiry_at - Duration::hours(1)).timestamp_millis();
        let end = (expiry_at + Duration::days(2)).timestamp_millis();
        let records = client
            .exercise_records(&order.option_symbol, start, end)
            .await?;
        Ok(records
            .into_iter()
            .find(|record| {
                record.symbol == order.option_symbol && record.settlement_price > Decimal::ZERO
            })
            .map(|record| record.settlement_price))
    }

    /// 对 ITM 到期订单执行现货补齐。
    ///
    /// 补齐失败会把订单置为失败状态，阻止后续自动动作继续假设该订单已完成。
    #[allow(clippy::too_many_lines)]
    async fn rebalance_spot_if_needed(
        &self,
        client: &BinanceClient,
        mut order: StrategyOrder,
    ) -> AppResult<StrategyOrder> {
        let _trade_guard = self.trade_lock.lock().await;
        if order.spot_order_id.is_some() {
            self.sync_spot_rebalance(client, &mut order).await?;
            self.save_order(&order).await?;
            if order.status == StrategyStatus::Settled {
                self.sweep_spot_rebalance_assets_best_effort(client, order.direction)
                    .await;
            }
            return Ok(order);
        }

        if let Err(error) = self.prepare_spot_rebalance_funding(client, &order).await {
            order.status = StrategyStatus::RebalanceFailed;
            order.updated_at = Utc::now();
            order.rebalance_message = format!("现货补齐资金准备失败：{error}");
            self.save_order(&order).await?;
            self.record_warning(
                "strategy.rebalance.failed",
                &format!("{} {}", order.option_symbol, order.rebalance_message),
            )?;
            return Ok(order);
        }

        let spot_account = client.spot_account().await?;
        if let Err(error) = validate_spot_rebalance(&order, &spot_account) {
            order.status = StrategyStatus::RebalanceFailed;
            order.updated_at = Utc::now();
            order.rebalance_message = format!("现货补齐风控失败：{error}");
            self.save_order(&order).await?;
            self.record_warning(
                "strategy.rebalance.failed",
                &format!("{} {}", order.option_symbol, order.rebalance_message),
            )?;
            return Ok(order);
        }
        let side = crate::binance::spot_rebalance_side(order.direction);
        let client_order_id = make_spot_client_order_id(order.direction);
        let now = Utc::now();
        // 现货补齐也是实盘订单，同样先保存恢复记录再发送交易所请求。
        let mut attempt = OrderAttempt {
            id: Uuid::new_v4().to_string(),
            client_order_id: client_order_id.clone(),
            symbol: "BTCUSDT".to_string(),
            option_symbol: order.option_symbol.clone(),
            direction: order.direction,
            status: OrderAttemptStatus::SpotAttempted,
            exchange_order_id: None,
            message: format!("到期 ITM，准备发送现货 {side} 补齐订单"),
            created_at: now,
            updated_at: now,
        };
        self.save_order_attempt(&attempt)?;
        let ack = match client
            .place_spot_market_order("BTCUSDT", side, order.filled_quantity, &client_order_id)
            .await
        {
            Ok(ack) => ack,
            Err(error) => {
                if is_uncertain_submit_error(&error) {
                    self.record_warning(
                        "strategy.rebalance.submit_uncertain",
                        &format!(
                            "{} 现货补齐请求结果不确定，正在按客户端订单号查询 Binance",
                            order.option_symbol
                        ),
                    )?;
                    match client
                        .query_spot_order("BTCUSDT", None, Some(&client_order_id))
                        .await
                    {
                        Ok(ack) => ack,
                        Err(query_error) => {
                            attempt.message = format!(
                                "Binance 现货补齐结果不确定，按客户端订单号查询失败：{query_error}"
                            );
                            attempt.updated_at = Utc::now();
                            self.save_order_attempt(&attempt)?;
                            order.status = StrategyStatus::RebalanceFailed;
                            order.updated_at = Utc::now();
                            order.rebalance_message = format!(
                                "现货补齐结果不确定，请按恢复记录重新查询 Binance：{query_error}"
                            );
                            self.save_order(&order).await?;
                            self.record_warning(
                                "strategy.rebalance.failed",
                                &format!("{} {}", order.option_symbol, order.rebalance_message),
                            )?;
                            return Ok(order);
                        }
                    }
                } else {
                    order.status = StrategyStatus::RebalanceFailed;
                    order.updated_at = Utc::now();
                    order.rebalance_message = format!("现货补齐下单失败：{error}");
                    self.save_order(&order).await?;
                    self.record_warning(
                        "strategy.rebalance.failed",
                        &format!("{} {}", order.option_symbol, order.rebalance_message),
                    )?;
                    return Ok(order);
                }
            }
        };
        attempt.status = OrderAttemptStatus::SpotExchangeAck;
        attempt.exchange_order_id = Some(ack.order_id.clone());
        attempt.message = format!("Binance 已返回现货订单号，状态 {}", ack.status);
        attempt.updated_at = Utc::now();
        self.save_order_attempt(&attempt)?;

        order.spot_order_id = Some(ack.order_id.clone());
        order.spot_client_order_id = client_order_id;
        apply_spot_ack(&mut order, &ack);
        if let Err(error) = self.save_order(&order).await {
            attempt.status = OrderAttemptStatus::SpotPersistFailed;
            attempt.message = "交易所现货订单已创建，但补齐状态保存失败".to_string();
            attempt.updated_at = Utc::now();
            self.save_order_attempt(&attempt)?;
            return Err(AppError::Storage(format!(
                "交易所现货订单已创建但补齐状态保存失败，spotOrderId={}，clientOrderId={}。请先用 Binance 后台核对后再继续操作。原始错误：{}",
                order.spot_order_id.as_deref().unwrap_or(""),
                order.spot_client_order_id,
                error
            )));
        }
        attempt.status = OrderAttemptStatus::SpotPersisted;
        attempt.message = "交易所现货订单和补齐状态均已保存".to_string();
        attempt.updated_at = Utc::now();
        self.save_order_attempt(&attempt)?;
        self.record_log(
            "strategy.rebalance.created",
            &format!("{} {}", order.option_symbol, order.rebalance_message),
        )?;
        if order.status == StrategyStatus::Settled {
            self.sweep_spot_rebalance_assets_best_effort(client, order.direction)
                .await;
        }
        Ok(order)
    }

    async fn sync_spot_rebalance(
        &self,
        client: &BinanceClient,
        order: &mut StrategyOrder,
    ) -> AppResult<()> {
        let ack = client
            .query_spot_order(
                "BTCUSDT",
                order.spot_order_id.as_deref(),
                Some(&order.spot_client_order_id),
            )
            .await?;
        apply_spot_ack(order, &ack);
        Ok(())
    }

    /// 执行一次自动策略：扫描候选并选择第一条推荐机会下单。
    async fn execute_auto_strategy(&self) -> AppResult<Option<StrategyOrder>> {
        let result = self.scan_candidates().await?;
        // 自动策略只接受 Rust 策略层标记为 Recommended 的第一条候选，不在前端重新排名。
        let Some(candidate) = result.candidates.into_iter().find(|candidate| {
            candidate.recommendation == crate::domain::RecommendationLevel::Recommended
        }) else {
            self.record_log("strategy.auto.no_candidate", "本次自动扫描无推荐候选")?;
            return Ok(None);
        };

        let _trade_guard = self.trade_lock.lock().await;
        // 进入 place_live_strategy 后不再逐笔确认；是否允许自动下单由开启自动策略时的门禁决定。
        let order = self
            .place_live_strategy(
                StrategyRequest {
                    candidate_id: Some(candidate.id),
                    client_flow_id: None,
                    symbol: candidate.symbol,
                    option_symbol: Some(candidate.option_symbol),
                    direction: candidate.direction,
                    expiry: candidate.expiry,
                    expiry_at: candidate.expiry_at,
                    strike: candidate.strike,
                    quantity: candidate.quantity,
                    limit_price: candidate.limit_price,
                    confirm_live: true,
                },
                "auto",
                &OrderProgressEmitter::disabled("auto"),
            )
            .await?;
        Ok(Some(order))
    }

    /// 后台轮询任务的一次 tick。
    ///
    /// 只做同步、结算和按计划自动扫描，不会在未启用自动策略时下单。
    async fn background_tick(&self) -> AppResult<()> {
        if !self.runtime.read().await.connected {
            return Ok(());
        }
        self.recover_due_funding_attempts().await?;
        self.sync_live_orders().await?;
        if self.pending_recovery_count()? > 0 {
            return Ok(());
        }
        self.settle_due_orders().await?;
        if self.should_run_auto_now().await {
            match self.execute_auto_strategy().await {
                Ok(_) => self.mark_auto_scan_day().await,
                Err(error) => {
                    if should_consume_auto_day_after_error(&error) {
                        self.mark_auto_scan_day().await;
                    }
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    /// 判断当前是否到了当天自动策略执行时间。
    ///
    /// `last_auto_scan_day` 确保每天最多自动执行一次。
    async fn should_run_auto_now(&self) -> bool {
        let runtime = self.runtime.read().await.clone();
        if !runtime.connected || !runtime.auto_enabled {
            return false;
        }
        let rule = self.auto_rule.read().await.clone();
        if !rule.enabled {
            return false;
        }
        if self.pending_recovery_count().unwrap_or(1) > 0 {
            return false;
        }
        let now = Local::now();
        let scheduled = parse_schedule_time(&rule.schedule);
        if now.time() < scheduled {
            return false;
        }
        let today = now.date_naive();
        *self.last_auto_scan_day.read().await != Some(today)
    }

    async fn mark_auto_scan_day(&self) {
        let today = Local::now().date_naive();
        let mut last = self.last_auto_scan_day.write().await;
        *last = Some(today);
    }

    /// 启动后台轮询任务。
    ///
    /// `worker_started` 防止多次连接或重复调用时启动多个轮询器。
    fn ensure_background_worker(&self) {
        if self.worker_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let state = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(StdDuration::from_mins(1));
            loop {
                interval.tick().await;
                let Ok(_maintenance_guard) = state.maintenance_lock.try_lock() else {
                    // 后台维护不排队堆积；上一轮未结束时直接跳过并记录健康状态。
                    if let Err(error) = state.record_background_tick_skipped().await {
                        let _ = state
                            .record_error("background.tick_skip_log_failed", &error.to_string());
                    }
                    continue;
                };
                match state.background_tick().await {
                    Ok(()) => state.record_background_tick_success().await,
                    Err(error) => {
                        state.record_background_tick_failure().await;
                        let _ = state.record_error("background.tick_failed", &error.to_string());
                    }
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
                .list_funding_attempts(200)?
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
            let _trade_guard = self.trade_lock.lock().await;
            // 资金恢复和新资金动作共用 trade_lock，防止恢复查询期间并发发送新的资金请求。
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
        // 从未发送过的 attempted 记录可以继续发送一次；其他状态只能查询历史推进。
        if attempt.status.is_terminal() {
            return Ok(attempt);
        }
        if attempt.status == FundingAttemptStatus::Attempted && attempt.send_count == 0 {
            return self.send_funding_attempt(client, attempt).await;
        }
        self.query_funding_attempt(client, attempt).await
    }

    async fn execute_funding_request(
        &self,
        client: &BinanceClient,
        request: FundingRequest,
    ) -> AppResult<FundingAttempt> {
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
            created_at: now,
            updated_at: now,
        };
        self.save_funding_attempt(&attempt)?;
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
                attempt.message = "Binance 已返回资金动作引用，正在查询历史确认".to_string();
                attempt.updated_at = Utc::now();
                self.save_funding_attempt(&attempt)?;
                self.query_funding_attempt(client, attempt).await
            }
            Err(error) if is_uncertain_funding_error(&error) => {
                // 结果不确定时不重发资金动作，改为按 Binance 历史确认。
                attempt.status = FundingAttemptStatus::Uncertain;
                attempt.message = format!("资金请求结果不确定，正在按 Binance 历史确认：{error}");
                attempt.query_after_at = Some(Utc::now());
                attempt.updated_at = Utc::now();
                self.save_funding_attempt(&attempt)?;
                self.query_funding_attempt(client, attempt).await
            }
            Err(error) => {
                attempt.status = FundingAttemptStatus::Failed;
                attempt.message = format!("Binance 明确拒绝或本地校验失败：{error}");
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
        // 查询次数也是恢复事实的一部分；多次无法唯一确认后会进入 blocked。
        attempt.query_count = attempt.query_count.saturating_add(1);
        attempt.updated_at = Utc::now();
        self.save_funding_attempt(&attempt)?;

        let outcome = self.lookup_funding_history(client, &attempt).await?;
        match outcome {
            FundingHistoryOutcome::Confirmed(exchange_ref, status) => {
                attempt.status = FundingAttemptStatus::Confirmed;
                attempt.exchange_ref = Some(exchange_ref);
                attempt.query_after_at = None;
                attempt.message = format!("已按 Binance 历史确认成功，状态 {status}");
                attempt.updated_at = Utc::now();
                self.save_funding_attempt(&attempt)?;
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
                attempt.query_after_at = Some(Utc::now() + Duration::minutes(1));
                attempt.message = format!("Binance 历史显示仍在处理中，状态 {status}");
                attempt.updated_at = Utc::now();
                self.save_funding_attempt(&attempt)?;
            }
            FundingHistoryOutcome::Failed(exchange_ref, status) => {
                if let Some(exchange_ref) = exchange_ref {
                    attempt.exchange_ref = Some(exchange_ref);
                }
                attempt.status = FundingAttemptStatus::Failed;
                attempt.query_after_at = None;
                attempt.message = format!("Binance 历史确认失败，状态 {status}");
                attempt.updated_at = Utc::now();
                self.save_funding_attempt(&attempt)?;
            }
            FundingHistoryOutcome::NotFound | FundingHistoryOutcome::Ambiguous => {
                let ambiguous = matches!(outcome, FundingHistoryOutcome::Ambiguous);
                // 多条匹配比查不到更危险：无法判断哪条是本机请求，继续自动动作可能重复移动资金。
                if attempt.query_count >= FUNDING_QUERY_RETRY_LIMIT {
                    attempt.status = FundingAttemptStatus::Blocked;
                    attempt.query_after_at = None;
                    attempt.message = if ambiguous {
                        "Binance 历史匹配到多条可能记录，无法唯一确认，已阻断自动资金动作"
                            .to_string()
                    } else {
                        "多次查询 Binance 历史仍无法确认该资金动作是否发生，已阻断自动资金动作"
                            .to_string()
                    };
                } else {
                    attempt.status = FundingAttemptStatus::Uncertain;
                    attempt.query_after_at = Some(Utc::now() + Duration::minutes(1));
                    attempt.message = if ambiguous {
                        "Binance 历史匹配到多条可能记录，稍后重查以避免重复执行".to_string()
                    } else {
                        "暂未在 Binance 历史中确认该资金动作，稍后继续查询".to_string()
                    };
                }
                attempt.updated_at = Utc::now();
                self.save_funding_attempt(&attempt)?;
            }
        }
        Ok(attempt)
    }

    async fn lookup_funding_history(
        &self,
        client: &BinanceClient,
        attempt: &FundingAttempt,
    ) -> AppResult<FundingHistoryOutcome> {
        // 每种资金动作走对应 Binance 历史接口，不能把日志或前端进度当作资金事实。
        let (start_time_ms, end_time_ms) = funding_history_window(attempt.created_at);
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

    /// 确保现货账户有足够资产执行到期 ITM 补齐。
    async fn prepare_spot_rebalance_funding(
        &self,
        client: &BinanceClient,
        order: &StrategyOrder,
    ) -> AppResult<()> {
        let (asset, required) = spot_rebalance_required_asset(order);
        let chain_id = format!("spot-rebalance-{}", order.id);
        self.ensure_spot_asset_available(client, asset, required, "到期现货补齐", &chain_id)
            .await
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
        // 归集是收尾优化，失败时只写脱敏告警，不回滚已经完成的下单或结算事实。
        if let Err(error) = self.sweep_option_idle_usdt(client, purpose).await {
            let _ = self.record_warning("funding.sweep.failed", &format!("{purpose}失败：{error}"));
        }
    }

    /// 把期权账户中不需要继续支撑保证金风控的 USDT 归集回现货并申购活期。
    async fn sweep_option_idle_usdt(&self, client: &BinanceClient, purpose: &str) -> AppResult<()> {
        let account = client.option_account().await?;
        let amount = round_down_funding_amount(option_idle_usdt_sweep_amount(&account));
        if amount < dec!(0.01) {
            return Ok(());
        }

        let chain_id = Uuid::new_v4().to_string();
        self.execute_universal_transfer(client, &chain_id, purpose, "OPTION_MAIN", "USDT", amount)
            .await?;
        self.record_log(
            "funding.transfer.option_main",
            &format!("已从期权保证金账户归集 {amount} USDT 到现货账户，{purpose}"),
        )?;
        self.subscribe_spot_asset_to_earn(client, "USDT", amount, purpose, &chain_id)
            .await
    }

    async fn sweep_spot_rebalance_assets_best_effort(
        &self,
        client: &BinanceClient,
        direction: StrategyDirection,
    ) {
        // 现货补齐后的归集失败不应把已完成补齐改成失败，只提示后续人工处理。
        if let Err(error) = self.sweep_spot_rebalance_assets(client, direction).await {
            let _ = self.record_warning(
                "funding.sweep.failed",
                &format!("现货补齐后归集失败：{error}"),
            );
        }
    }

    /// 现货补齐完成后，把本流程会用到的 BTC/USDT 空闲余额重新放入活期赚币。
    async fn sweep_spot_rebalance_assets(
        &self,
        client: &BinanceClient,
        direction: StrategyDirection,
    ) -> AppResult<()> {
        let spot_account = client.spot_account().await?;
        let assets = match direction {
            StrategyDirection::SellPut | StrategyDirection::SellCall => ["USDT", "BTC"],
        };
        let chain_id = Uuid::new_v4().to_string();
        for asset in assets {
            // 这里只归集本策略会触碰的 BTC/USDT，避免误移动用户其他现货资产。
            let amount = spot_asset_free(&spot_account, asset);
            if amount <= Decimal::ZERO {
                continue;
            }
            self.subscribe_spot_asset_to_earn(client, asset, amount, "现货补齐后归集", &chain_id)
                .await?;
        }
        Ok(())
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
