//! 应用状态编排层。
//!
//! 这个模块连接 Tauri command、Binance 客户端、策略计算和 SQLite 持久化。
//! 它负责实盘连接、快照聚合、下单门禁、订单恢复、到期结算、现货补齐和后台轮询。
//! 业务计算放在 `strategy.rs`，交易所请求放在 `binance.rs`，凭据读写放在 `secrets.rs`。

use std::{
    collections::HashMap,
    future::Future,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration as StdDuration,
};

use chrono::{Duration, Local, NaiveDate, NaiveTime, Utc};
use rust_decimal::{Decimal, RoundingStrategy};
use rust_decimal_macros::dec;
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex as AsyncMutex, RwLock};
use uuid::Uuid;

use crate::binance::BinanceClient;
use crate::domain::{
    AppError, AppResult, AppSettings, AppSnapshot, AutoInvestmentRule, BalanceRow,
    CandidateScanDiagnostics, CandidateScanResult, ConnectBinanceRequest, ExchangeOrderAck,
    LogLevel, LogSnapshot, OptionAccountSnapshot, OptionContract, OptionMarkPrice, OrderAttempt,
    OrderAttemptStatus, OrderAttemptView, OrderProgressEvent, OrderProgressStage,
    OrderProgressStatus, RuntimeHealth, RuntimeHealthStatus, SimpleEarnSnapshot,
    SpotAccountSnapshot, StrategyCandidate, StrategyDirection, StrategyOrder, StrategyRequest,
    StrategyStatus, ToggleAutoRequest,
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

#[derive(Clone, Debug, Default)]
struct RuntimeHealthState {
    last_tick_at: Option<chrono::DateTime<Utc>>,
    last_success_at: Option<chrono::DateTime<Utc>>,
    consecutive_failures: u32,
    skipped_ticks: u32,
    last_error: Option<String>,
}

const ORDER_PROGRESS_EVENT: &str = "order-progress";
const SELL_PUT_REBALANCE_USDT_BUFFER: Decimal = dec!(1.10);

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

#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct RuntimeConfig {
    pub connected: bool,
    pub api_key_masked: Option<String>,
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub saved_credentials: bool,
    pub auto_enabled: bool,
    pub quick_execution: bool,
    pub default_symbol: String,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            connected: false,
            api_key_masked: None,
            api_key: None,
            api_secret: None,
            saved_credentials: secrets::has_binance_credentials(),
            auto_enabled: false,
            quick_execution: true,
            default_symbol: "BTCUSDT".to_string(),
        }
    }
}

impl RuntimeConfig {
    /// 转成前端可见设置，过滤掉明文 API Key 和 Secret。
    pub fn settings(&self) -> AppSettings {
        AppSettings {
            connected: self.connected,
            api_key_masked: self.api_key_masked.clone(),
            saved_credentials: self.saved_credentials,
            auto_enabled: self.auto_enabled,
            quick_execution: self.quick_execution,
            default_symbol: self.default_symbol.clone(),
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
    scan_diagnostics: Arc<RwLock<CandidateScanDiagnostics>>,
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
    /// 启动时只加载已经绑定交易所订单号的订单，并强制关闭自动策略，避免应用重启后自动下单。
    fn from_persistence(persistence: Persistence, logs: LogStore) -> AppResult<Self> {
        let orders = persistence
            .list_orders()?
            .into_iter()
            .filter(|order| order.exchange_order_id.is_some())
            .collect();
        let mut auto_rule = persistence
            .load_auto_rule()?
            .unwrap_or_else(|| strategy::default_auto_rule(false));
        auto_rule.enabled = false;

        let state = Self {
            runtime: Arc::new(RwLock::new(RuntimeConfig::default())),
            client: Arc::new(RwLock::new(None)),
            candidates: Arc::new(RwLock::new(Vec::new())),
            scan_diagnostics: Arc::new(RwLock::new(CandidateScanDiagnostics::default())),
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
        let scan_diagnostics = self.scan_diagnostics.read().await.clone();
        let orders = self.orders.read().await.clone();
        let accounts = self.live_accounts_if_connected().await?;
        let (order_attempts, pending_recovery_count) = {
            let persistence = self
                .persistence
                .lock()
                .map_err(|error| AppError::Storage(error.to_string()))?;
            let attempts = persistence
                .list_order_attempts(20)?
                .into_iter()
                .map(OrderAttemptView::from)
                .collect::<Vec<_>>();
            let pending_recovery_count = persistence.count_pending_order_attempts()?;
            (attempts, pending_recovery_count)
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
            scan_diagnostics,
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
        })
    }

    /// 使用用户输入凭据连接 Binance，并保存到系统 keyring。
    ///
    /// 连接阶段同时验证期权和现货账户权限，因为到期 ITM 后需要真实现货补齐能力。
    pub async fn connect_binance(&self, request: ConnectBinanceRequest) -> AppResult<AppSettings> {
        if request.api_key.trim().is_empty() || request.api_secret.trim().is_empty() {
            return Err(AppError::Config("API Key 和 Secret 不能为空".to_string()));
        }

        let api_key = request.api_key.trim().to_string();
        let api_secret = request.api_secret;
        let client = BinanceClient::new(api_key.clone(), api_secret.clone())?;
        let option_account_task = spawn_binance_request({
            let client = client.clone();
            async move { client.verify_account().await }
        });
        let spot_account_task = spawn_binance_request({
            let client = client.clone();
            async move { client.spot_account().await }
        });
        let option_account = join_binance_request(option_account_task).await;
        let spot_account = join_binance_request(spot_account_task).await;
        option_account?;
        let spot_account = spot_account?;
        if !spot_account.can_trade {
            return Err(AppError::Config(
                "Binance 现货账户当前不可交易，无法执行到期补齐".to_string(),
            ));
        }
        secrets::save_binance_credentials(&api_key, &api_secret)?;

        let mut runtime = self.runtime.write().await;
        runtime.connected = true;
        runtime.api_key_masked = Some(mask_api_key(&api_key));
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
        let Some((api_key, api_secret)) = secrets::load_binance_credentials()? else {
            return Err(AppError::Config(
                "系统 keyring 中没有已保存的 Binance API 凭据".to_string(),
            ));
        };
        let client = BinanceClient::new(api_key.clone(), api_secret.clone())?;
        let option_account_task = spawn_binance_request({
            let client = client.clone();
            async move { client.verify_account().await }
        });
        let spot_account_task = spawn_binance_request({
            let client = client.clone();
            async move { client.spot_account().await }
        });
        let option_account = join_binance_request(option_account_task).await;
        let spot_account = join_binance_request(spot_account_task).await;
        option_account?;
        let spot_account = spot_account?;
        if !spot_account.can_trade {
            return Err(AppError::Config(
                "Binance 现货账户当前不可交易，无法执行到期补齐".to_string(),
            ));
        }

        let mut runtime = self.runtime.write().await;
        runtime.connected = true;
        runtime.api_key_masked = Some(mask_api_key(&api_key));
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
        runtime.api_key_masked = None;
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
            let mut scan_diagnostics = self.scan_diagnostics.write().await;
            *scan_diagnostics = CandidateScanDiagnostics::default();
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
        {
            let mut scan_diagnostics = self.scan_diagnostics.write().await;
            scan_diagnostics.clone_from(&result.diagnostics);
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

    /// 同步订单、执行到期结算，然后返回完整快照。
    pub async fn sync_live_state(&self) -> AppResult<AppSnapshot> {
        let _maintenance_guard = self.maintenance_lock.lock().await;
        self.sync_live_orders().await?;
        self.settle_due_orders().await?;
        self.snapshot().await
    }

    /// 手动触发到期结算扫描。
    pub async fn run_settlement_scan(&self) -> AppResult<Vec<StrategyOrder>> {
        let _maintenance_guard = self.maintenance_lock.lock().await;
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
            last_tick_at: health.last_tick_at,
            last_success_at: health.last_success_at,
            consecutive_failures: health.consecutive_failures,
            skipped_ticks: health.skipped_ticks,
            last_error: health.last_error,
            pending_recovery_count,
        }
    }

    async fn record_background_tick_started(&self) {
        let mut health = self.health.write().await;
        health.last_tick_at = Some(Utc::now());
    }

    async fn record_background_tick_success(&self) {
        let mut health = self.health.write().await;
        health.last_success_at = Some(Utc::now());
        health.consecutive_failures = 0;
        health.skipped_ticks = 0;
        health.last_error = None;
    }

    async fn record_background_tick_failure(&self, error: &AppError) {
        let mut health = self.health.write().await;
        health.consecutive_failures = health.consecutive_failures.saturating_add(1);
        health.last_error = Some(error.to_string());
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
        let orders = self.orders.read().await.clone();
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

        if let Err(error) = enforce_pre_funding_risk_rules(&account, &orders) {
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
        if let Err(error) = enforce_risk_rules(&request, contract, &account, &orders) {
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

        for order in orders.into_iter().filter(should_sync_option_order) {
            match self.sync_single_order(&client, order.clone()).await {
                Ok(order) => updated.push(order),
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
    ) -> AppResult<StrategyOrder> {
        let ack = client
            .query_option_order(
                &order.option_symbol,
                order.exchange_order_id.as_deref(),
                Some(&order.client_order_id),
            )
            .await?;
        apply_option_ack(&mut order, &ack);
        self.save_order(&order).await?;
        Ok(order)
    }

    /// 扫描已经到期的订单并执行结算/补齐。
    ///
    /// 未成交订单会直接过期；OTM 订单结算完成；ITM 订单进入现货补齐流程。
    async fn settle_due_orders(&self) -> AppResult<Vec<StrategyOrder>> {
        let client = self.live_client().await?;
        let orders = self.orders.read().await.clone();
        let mut updated = Vec::new();

        for mut order in orders.into_iter().filter(should_settle_order) {
            if order.filled_quantity <= Decimal::ZERO {
                order.status = StrategyStatus::Expired;
                order.updated_at = Utc::now();
                order.rebalance_message = "期权未成交，无需到期补齐".to_string();
                self.save_order(&order).await?;
                updated.push(order);
                continue;
            }

            let settlement_price = match self.settlement_price(&client, &order).await {
                Ok(Some(price)) => price,
                Ok(None) => continue,
                Err(error) => {
                    self.record_warning(
                        "strategy.settlement.price_failed",
                        &format!("{} 结算价读取失败：{}", order.option_symbol, error),
                    )?;
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
                let _trade_guard = self.trade_lock.lock().await;
                self.sweep_option_idle_usdt_best_effort(&client, "期权到期结算后归集")
                    .await;
                updated.push(order);
                continue;
            }

            match self.rebalance_spot_if_needed(&client, order).await {
                Ok(order) => {
                    let _trade_guard = self.trade_lock.lock().await;
                    self.sweep_option_idle_usdt_best_effort(&client, "期权到期补齐后归集")
                        .await;
                    updated.push(order);
                }
                Err(error) => {
                    self.record_warning(
                        "strategy.rebalance.scan_failed",
                        &format!("到期补齐流程失败：{error}"),
                    )?;
                }
            }
        }
        Ok(updated)
    }

    async fn settlement_price(
        &self,
        client: &BinanceClient,
        order: &StrategyOrder,
    ) -> AppResult<Option<Decimal>> {
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
        let Some(candidate) = result.candidates.into_iter().find(|candidate| {
            candidate.recommendation == crate::domain::RecommendationLevel::Recommended
        }) else {
            self.record_log("strategy.auto.no_candidate", "本次自动扫描无推荐候选")?;
            return Ok(None);
        };

        let _trade_guard = self.trade_lock.lock().await;
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
        self.sync_live_orders().await?;
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
                    if let Err(error) = state.record_background_tick_skipped().await {
                        let _ = state
                            .record_error("background.tick_skip_log_failed", &error.to_string());
                    }
                    continue;
                };
                state.record_background_tick_started().await;
                match state.background_tick().await {
                    Ok(()) => state.record_background_tick_success().await,
                    Err(error) => {
                        state.record_background_tick_failure(&error).await;
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
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .count_pending_order_attempts()
    }

    /// 保存下单/补齐恢复记录。
    fn save_order_attempt(&self, attempt: &OrderAttempt) -> AppResult<()> {
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .save_order_attempt(attempt)
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
        self.ensure_spot_asset_available(client, "USDT", amount, "期权保证金准备")
            .await?;
        client
            .universal_transfer("MAIN_OPTION", "USDT", amount)
            .await?;
        self.record_log(
            "funding.transfer.main_option",
            &format!("已从现货划转 {amount} USDT 到期权保证金账户，用于期权下单"),
        )?;
        client.option_account().await
    }

    /// 确保现货账户有足够资产执行到期 ITM 补齐。
    async fn prepare_spot_rebalance_funding(
        &self,
        client: &BinanceClient,
        order: &StrategyOrder,
    ) -> AppResult<()> {
        let (asset, required) = spot_rebalance_required_asset(order);
        self.ensure_spot_asset_available(client, asset, required, "到期现货补齐")
            .await
    }

    /// 现货余额不足时，从 Simple Earn 活期赎回缺口。
    async fn ensure_spot_asset_available(
        &self,
        client: &BinanceClient,
        asset: &str,
        required: Decimal,
        purpose: &str,
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
        self.redeem_flexible_asset(client, asset, redeem_amount, purpose)
            .await
    }

    async fn redeem_flexible_asset(
        &self,
        client: &BinanceClient,
        asset: &str,
        amount: Decimal,
        purpose: &str,
    ) -> AppResult<()> {
        if amount <= Decimal::ZERO {
            return Ok(());
        }
        let earn = client.simple_earn_snapshot().await?;
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

        client
            .redeem_simple_earn_flexible(&position.product_id, amount)
            .await?;
        self.record_log(
            "funding.earn.redeemed",
            &format!("已从 Simple Earn 活期赎回 {amount} {asset} 至现货账户，用于{purpose}"),
        )
    }

    async fn sweep_option_idle_usdt_best_effort(&self, client: &BinanceClient, purpose: &str) {
        if let Err(error) = self.sweep_option_idle_usdt(client, purpose).await {
            let _ = self.record_warning("funding.sweep.failed", &format!("{purpose}失败：{error}"));
        }
    }

    /// 把期权账户中不需要继续支撑保证金风控的 USDT 归集回现货并申购活期。
    async fn sweep_option_idle_usdt(&self, client: &BinanceClient, purpose: &str) -> AppResult<()> {
        let account = client.option_account().await?;
        let amount = option_idle_usdt_sweep_amount(&account);
        if amount < dec!(0.01) {
            return Ok(());
        }

        client
            .universal_transfer("OPTION_MAIN", "USDT", amount)
            .await?;
        self.record_log(
            "funding.transfer.option_main",
            &format!("已从期权保证金账户归集 {amount} USDT 到现货账户，{purpose}"),
        )?;
        self.subscribe_spot_asset_to_earn(client, "USDT", amount, purpose)
            .await
    }

    async fn sweep_spot_rebalance_assets_best_effort(
        &self,
        client: &BinanceClient,
        direction: StrategyDirection,
    ) {
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
        for asset in assets {
            let amount = spot_asset_free(&spot_account, asset);
            if amount <= Decimal::ZERO {
                continue;
            }
            self.subscribe_spot_asset_to_earn(client, asset, amount, "现货补齐后归集")
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
    ) -> AppResult<()> {
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
        client
            .subscribe_simple_earn_flexible(&product.product_id, amount)
            .await?;
        self.record_log(
            "funding.earn.subscribed",
            &format!("已把 {amount} {asset} 从现货账户重新申购 Simple Earn 活期，{purpose}"),
        )
    }
}

fn spawn_binance_request<T, F>(future: F) -> tauri::async_runtime::JoinHandle<AppResult<T>>
where
    T: Send + 'static,
    F: Future<Output = AppResult<T>> + Send + 'static,
{
    tauri::async_runtime::spawn(future)
}

/// 等待 Binance 子任务并把 join 错误转换成业务错误。
async fn join_binance_request<T>(
    handle: tauri::async_runtime::JoinHandle<AppResult<T>>,
) -> AppResult<T> {
    handle
        .await
        .map_err(|error| AppError::Binance(format!("Binance 异步请求任务失败：{error}")))?
}

/// 校验下单请求的基础业务边界。
fn validate_request(request: &StrategyRequest) -> AppResult<()> {
    if request.symbol != "BTCUSDT" {
        return Err(AppError::RiskRejected(
            "v1 仅支持 BTC/USDT 策略".to_string(),
        ));
    }
    if request.quantity < dec!(0.01) {
        return Err(AppError::RiskRejected(
            "币安期权最小交易单位按 0.01 张处理".to_string(),
        ));
    }
    if request.strike <= Decimal::ZERO {
        return Err(AppError::RiskRejected("执行价必须大于 0".to_string()));
    }
    if request.limit_price <= Decimal::ZERO {
        return Err(AppError::RiskRejected("限价必须大于 0".to_string()));
    }
    Ok(())
}

/// 校验自动复投规则边界。
fn validate_auto_rule(rule: &AutoInvestmentRule) -> AppResult<()> {
    if rule.quantity < dec!(0.01) {
        return Err(AppError::RiskRejected(
            "自动定投数量不能小于 0.01 张".to_string(),
        ));
    }
    if rule.min_days_to_expiry < 1 || rule.max_days_to_expiry < rule.min_days_to_expiry {
        return Err(AppError::RiskRejected("到期天数区间无效".to_string()));
    }
    Ok(())
}

/// 从运行期配置重建 Binance 客户端。
///
/// 这里读取的是内存凭据；持久化凭据只允许通过 `secrets.rs` 从 keyring 读取。
fn client_from_runtime(runtime: &RuntimeConfig) -> AppResult<BinanceClient> {
    if !runtime.connected {
        return Err(AppError::Config("请先连接 Binance API".to_string()));
    }
    let api_key = runtime
        .api_key
        .clone()
        .ok_or_else(|| AppError::Config("缺少 Binance API Key".to_string()))?;
    let api_secret = runtime
        .api_secret
        .clone()
        .ok_or_else(|| AppError::Config("缺少 Binance API Secret".to_string()))?;
    BinanceClient::new(api_key, api_secret)
}

/// 从期权恢复记录和交易所回报重建下单请求。
fn request_from_option_attempt(
    attempt: &OrderAttempt,
    ack: &ExchangeOrderAck,
    contract: &OptionContract,
) -> AppResult<StrategyRequest> {
    let (_expiry, strike, direction) = parse_option_symbol(&attempt.option_symbol)?;
    let quantity = if ack.quantity > Decimal::ZERO {
        ack.quantity
    } else {
        ack.executed_qty
    };
    let limit_price = if ack.price > Decimal::ZERO {
        ack.price
    } else {
        ack.avg_price
    };
    Ok(StrategyRequest {
        candidate_id: None,
        client_flow_id: None,
        symbol: attempt.symbol.clone(),
        option_symbol: Some(attempt.option_symbol.clone()),
        direction,
        expiry: contract.expiry,
        expiry_at: contract.expiry_at,
        strike,
        quantity,
        limit_price,
        confirm_live: true,
    })
}

/// 解析 Binance 期权合约代码中的到期日、执行价和方向。
fn parse_option_symbol(value: &str) -> AppResult<(NaiveDate, Decimal, StrategyDirection)> {
    let parts = value.split('-').collect::<Vec<_>>();
    if parts.len() != 4 {
        return Err(AppError::Storage(format!("无法解析期权合约代码：{value}")));
    }
    let expiry = NaiveDate::parse_from_str(parts[1], "%y%m%d")
        .map_err(|error| AppError::Storage(format!("期权合约到期日无法解析：{error}")))?;
    let strike = parts[2]
        .parse::<Decimal>()
        .map_err(|error| AppError::Storage(format!("期权合约执行价无法解析：{error}")))?;
    let direction = match parts[3] {
        "P" | "PUT" => StrategyDirection::SellPut,
        "C" | "CALL" => StrategyDirection::SellCall,
        side => {
            return Err(AppError::Storage(format!("期权合约方向无法解析：{side}")));
        }
    };
    Ok((expiry, strike, direction))
}

/// 找到与下单请求匹配的 Binance 合约。
///
/// 优先使用请求里的 option symbol；没有时才按方向、到期日和执行价搜索。
fn resolve_contract<'a>(
    request: &StrategyRequest,
    contracts: &'a [OptionContract],
) -> AppResult<&'a OptionContract> {
    if let Some(symbol) = request
        .option_symbol
        .as_deref()
        .map(str::trim)
        .filter(|symbol| !symbol.is_empty())
    {
        let contract = contracts
            .iter()
            .find(|contract| contract.symbol == symbol)
            .ok_or_else(|| AppError::RiskRejected("未找到指定的 Binance 期权合约".to_string()))?;
        ensure_contract_matches_request(request, contract)?;
        return Ok(contract);
    }

    contracts
        .iter()
        .find(|contract| {
            contract.underlying == request.symbol
                && contract.expiry == request.expiry
                && contract.strike_price == request.strike
                && contract.side == request.direction.option_type()
        })
        .ok_or_else(|| {
            AppError::RiskRejected("未找到与方向、到期日和执行价匹配的 Binance 合约".to_string())
        })
}

/// 确认前端传来的合约代码与请求参数一致。
fn ensure_contract_matches_request(
    request: &StrategyRequest,
    contract: &OptionContract,
) -> AppResult<()> {
    if contract.underlying != request.symbol {
        return Err(AppError::RiskRejected(
            "期权合约标的与下单标的不一致".to_string(),
        ));
    }
    if contract.expiry != request.expiry {
        return Err(AppError::RiskRejected(
            "期权合约到期日与下单到期日不一致".to_string(),
        ));
    }
    if contract.strike_price != request.strike {
        return Err(AppError::RiskRejected(
            "期权合约执行价与下单执行价不一致".to_string(),
        ));
    }
    if contract.side != request.direction.option_type() {
        return Err(AppError::RiskRejected(
            "期权合约方向与下单方向不一致".to_string(),
        ));
    }
    Ok(())
}

/// 下单前合约与交易所约束校验。
///
/// 包含交易状态、v1 标的限制、数量精度、价格精度、价格上下限和保证金检查。
fn validate_contract_order(
    request: &StrategyRequest,
    contract: &OptionContract,
    account: &OptionAccountSnapshot,
    marks: &[OptionMarkPrice],
) -> AppResult<()> {
    validate_contract_terms(request, contract, marks)?;
    let available = account
        .asset("USDT")
        .map_or(Decimal::ZERO, |asset| asset.available);
    let required = strategy::required_margin(contract, request.direction, request.quantity);
    if available < required {
        return Err(AppError::RiskRejected(format!(
            "可用 USDT 保证金不足，预计至少需要 {}，当前可用 {}",
            required.round_dp(2),
            available.round_dp(2)
        )));
    }
    Ok(())
}

/// 校验不需要移动资金的合约与订单条件。
fn validate_contract_terms(
    request: &StrategyRequest,
    contract: &OptionContract,
    marks: &[OptionMarkPrice],
) -> AppResult<()> {
    if contract.status != "TRADING" {
        return Err(AppError::RiskRejected(format!(
            "期权合约当前不可交易，状态 {}",
            contract.status
        )));
    }
    if contract.underlying != "BTCUSDT" || contract.quote_asset != "USDT" {
        return Err(AppError::RiskRejected(
            "v1 仅允许 BTCUSDT/USDT 期权合约".to_string(),
        ));
    }
    if request.quantity < contract.min_qty || request.quantity > contract.max_qty {
        return Err(AppError::RiskRejected(format!(
            "下单数量必须在 Binance 合约范围 {} - {} 内",
            contract.min_qty, contract.max_qty
        )));
    }
    if decimal_scale(request.quantity) > contract.quantity_scale {
        return Err(AppError::RiskRejected(format!(
            "下单数量精度超过 Binance 合约限制 {} 位",
            contract.quantity_scale
        )));
    }
    if !strategy::decimal_matches_step(request.quantity, contract.quantity_step_size) {
        return Err(AppError::RiskRejected(format!(
            "下单数量必须按 Binance step size {} 递增",
            contract.quantity_step_size
        )));
    }
    if contract.min_price > Decimal::ZERO && request.limit_price < contract.min_price {
        return Err(AppError::RiskRejected(format!(
            "限价低于 Binance 合约最小价格 {}",
            contract.min_price
        )));
    }
    if contract.max_price > Decimal::ZERO && request.limit_price > contract.max_price {
        return Err(AppError::RiskRejected(format!(
            "限价高于 Binance 合约最大价格 {}",
            contract.max_price
        )));
    }
    if decimal_scale(request.limit_price) > contract.price_scale {
        return Err(AppError::RiskRejected(format!(
            "限价精度超过 Binance 合约限制 {} 位",
            contract.price_scale
        )));
    }
    if !strategy::decimal_matches_step(request.limit_price, contract.price_tick_size) {
        return Err(AppError::RiskRejected(format!(
            "限价必须按 Binance tick size {} 递增",
            contract.price_tick_size
        )));
    }

    if let Some(mark) = marks.iter().find(|mark| mark.symbol == contract.symbol) {
        if mark.low_price_limit > Decimal::ZERO && request.limit_price < mark.low_price_limit {
            return Err(AppError::RiskRejected(format!(
                "限价低于 Binance 当前价格下限 {}",
                mark.low_price_limit
            )));
        }
        if mark.high_price_limit > Decimal::ZERO && request.limit_price > mark.high_price_limit {
            return Err(AppError::RiskRejected(format!(
                "限价高于 Binance 当前价格上限 {}",
                mark.high_price_limit
            )));
        }
    }
    Ok(())
}

/// 执行本地风控规则。
fn enforce_risk_rules(
    request: &StrategyRequest,
    contract: &OptionContract,
    account: &OptionAccountSnapshot,
    orders: &[StrategyOrder],
) -> AppResult<()> {
    let rules = strategy::risk_snapshot_from_account(account).rules;
    let required = strategy::required_margin(contract, request.direction, request.quantity);
    enforce_pre_funding_risk_rules(account, orders)?;

    if let Some(usdt) = account.asset("USDT") {
        let equity = if usdt.adjusted_equity > Decimal::ZERO {
            usdt.adjusted_equity
        } else {
            usdt.equity
        };
        if equity > Decimal::ZERO {
            let next_usage = ((usdt.initial_margin + required) / equity * dec!(100)).round_dp(2);
            if next_usage > rules.max_margin_usage {
                return Err(AppError::RiskRejected(format!(
                    "下单后保证金占用预计 {}%，超过风控上限 {}%",
                    next_usage, rules.max_margin_usage
                )));
            }
        }
    }

    Ok(())
}

/// 校验不会因为资金划转而改变的风控条件，避免为必然拒绝的订单移动资金。
fn enforce_pre_funding_risk_rules(
    account: &OptionAccountSnapshot,
    orders: &[StrategyOrder],
) -> AppResult<()> {
    let rules = strategy::risk_snapshot_from_account(account).rules;

    let today = Utc::now().date_naive();
    let daily_orders = orders
        .iter()
        .filter(|order| order.created_at.date_naive() == today)
        .count();
    if Decimal::from(daily_orders) >= rules.max_daily_orders {
        return Err(AppError::RiskRejected(format!(
            "今日下单次数已达到风控上限 {}",
            rules.max_daily_orders
        )));
    }

    let consecutive_losses = orders
        .iter()
        .take_while(|order| order.estimated_pnl < Decimal::ZERO)
        .count();
    if consecutive_losses
        >= usize::try_from(rules.stop_after_consecutive_losses).unwrap_or(usize::MAX)
    {
        return Err(AppError::RiskRejected(
            "连续亏损次数已达到自动停止阈值".to_string(),
        ));
    }
    Ok(())
}

/// 校验到期 ITM 后的现货补齐能力。
fn validate_spot_rebalance(order: &StrategyOrder, account: &SpotAccountSnapshot) -> AppResult<()> {
    if !account.can_trade {
        return Err(AppError::RiskRejected(
            "现货账户不可交易，停止到期补齐".to_string(),
        ));
    }
    let (asset, required) = spot_rebalance_required_asset(order);
    let available = spot_asset_free(account, asset);
    if available < required {
        return Err(AppError::RiskRejected(format!(
            "现货补齐 {asset} 不足，预计需要 {}，当前可用 {}",
            required.round_dp(2),
            available.round_dp(2)
        )));
    }
    Ok(())
}

fn option_funding_shortfall(
    account: &OptionAccountSnapshot,
    contract: &OptionContract,
    request: &StrategyRequest,
) -> Decimal {
    let required = strategy::required_margin(contract, request.direction, request.quantity);
    let available = account
        .asset("USDT")
        .map_or(Decimal::ZERO, |asset| asset.available);
    let margin_shortfall = positive_decimal(required - available);

    let rules = strategy::risk_snapshot_from_account(account).rules;
    let Some(usdt) = account.asset("USDT") else {
        return max_decimal(
            margin_shortfall,
            risk_equity_target(required, rules.max_margin_usage),
        );
    };
    let equity = option_usdt_equity(usdt);
    let target_equity = risk_equity_target(usdt.initial_margin + required, rules.max_margin_usage);
    max_decimal(margin_shortfall, positive_decimal(target_equity - equity))
}

fn option_idle_usdt_sweep_amount(account: &OptionAccountSnapshot) -> Decimal {
    let Some(usdt) = account.asset("USDT") else {
        return Decimal::ZERO;
    };
    let rules = strategy::risk_snapshot_from_account(account).rules;
    let equity = option_usdt_equity(usdt);
    let min_equity = risk_equity_target(usdt.initial_margin, rules.max_margin_usage);
    let excess_equity = positive_decimal(equity - min_equity);
    min_decimal(usdt.available, excess_equity)
}

fn option_usdt_equity(usdt: &crate::domain::OptionAssetBalance) -> Decimal {
    if usdt.adjusted_equity > Decimal::ZERO {
        usdt.adjusted_equity
    } else {
        usdt.equity
    }
}

fn risk_equity_target(margin: Decimal, max_usage_percent: Decimal) -> Decimal {
    if margin <= Decimal::ZERO || max_usage_percent <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    margin * dec!(100) / max_usage_percent
}

fn spot_rebalance_required_asset(order: &StrategyOrder) -> (&'static str, Decimal) {
    match order.direction {
        StrategyDirection::SellPut => (
            "USDT",
            order.settlement_price * order.filled_quantity * SELL_PUT_REBALANCE_USDT_BUFFER,
        ),
        StrategyDirection::SellCall => ("BTC", order.filled_quantity),
    }
}

fn spot_asset_free(account: &SpotAccountSnapshot, asset: &str) -> Decimal {
    account.asset(asset).map_or(Decimal::ZERO, |row| row.free)
}

fn flexible_earn_asset_amount(earn: &SimpleEarnSnapshot, asset: &str) -> Decimal {
    earn.balances
        .iter()
        .filter(|balance| balance.flexible && balance.can_redeem && balance.asset == asset)
        .map(|balance| balance.amount)
        .sum()
}

fn round_up_funding_amount(value: Decimal) -> Decimal {
    if value <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    value.round_dp_with_strategy(2, RoundingStrategy::ToPositiveInfinity)
}

fn positive_decimal(value: Decimal) -> Decimal {
    max_decimal(value, Decimal::ZERO)
}

fn max_decimal(left: Decimal, right: Decimal) -> Decimal {
    if left >= right { left } else { right }
}

fn min_decimal(left: Decimal, right: Decimal) -> Decimal {
    if left <= right { left } else { right }
}

/// 把期权订单回报应用到本地订单。
fn apply_option_ack(order: &mut StrategyOrder, ack: &ExchangeOrderAck) {
    let requested_qty = if ack.quantity > Decimal::ZERO {
        ack.quantity
    } else {
        order.quantity
    };
    order.status = strategy::status_from_exchange(&ack.status, ack.executed_qty, requested_qty);
    order.exchange_status.clone_from(&ack.status);
    order.filled_quantity = ack.executed_qty;
    if ack.avg_price > Decimal::ZERO {
        order.avg_price = ack.avg_price;
    }
    order.fee = ack.fee;
    order.premium = (order.avg_price * order.filled_quantity).round_dp(2);
    order.estimated_pnl = order.premium - order.fee;
    order.updated_at = ack.update_time.or(ack.create_time).unwrap_or_else(Utc::now);
}

/// 把现货补齐订单回报应用到本地订单。
///
/// 现货没有足额成交时，订单进入 `RebalanceFailed`，避免系统继续自动执行相关动作。
fn apply_spot_ack(order: &mut StrategyOrder, ack: &ExchangeOrderAck) {
    order.spot_order_status.clone_from(&ack.status);
    order.spot_filled_quantity = ack.executed_qty;
    order.spot_avg_price = ack.avg_price;
    order.spot_fee = ack.fee;
    order.updated_at = ack.update_time.or(ack.create_time).unwrap_or_else(Utc::now);
    let expected = order.filled_quantity;
    if ack.status == "FILLED" && ack.executed_qty >= expected {
        order.status = StrategyStatus::Settled;
        order.settled_at = Some(Utc::now());
        order.rebalance_message = format!(
            "现货补齐已足额成交，数量 {}，均价 {}，状态 {}",
            ack.executed_qty, ack.avg_price, ack.status
        );
    } else if matches!(
        ack.status.as_str(),
        "REJECTED" | "EXPIRED" | "CANCELED" | "CANCELLED"
    ) {
        order.status = StrategyStatus::RebalanceFailed;
        order.rebalance_message = format!(
            "现货补齐未完成，成交 {} / {}，状态 {}",
            ack.executed_qty, expected, ack.status
        );
    } else {
        order.status = StrategyStatus::RebalanceFailed;
        order.rebalance_message = format!(
            "现货补齐尚未足额成交，成交 {} / {}，状态 {}",
            ack.executed_qty, expected, ack.status
        );
    }
}

fn is_spot_attempt(status: OrderAttemptStatus) -> bool {
    matches!(
        status,
        OrderAttemptStatus::SpotAttempted
            | OrderAttemptStatus::SpotExchangeAck
            | OrderAttemptStatus::SpotPersisted
            | OrderAttemptStatus::SpotPersistFailed
    )
}

fn should_consume_auto_day_after_error(error: &AppError) -> bool {
    !matches!(error, AppError::Binance(_))
}

fn is_uncertain_submit_error(error: &AppError) -> bool {
    matches!(
        error,
        AppError::Binance(message)
            if message.contains("请求超时")
                || message.contains("连接失败")
                || message.contains("响应体读取失败")
                || message.contains("网络请求失败")
    )
}

/// 判断期权订单是否仍需要向交易所同步。
fn should_sync_option_order(order: &StrategyOrder) -> bool {
    order.exchange_order_id.is_some()
        && !matches!(
            order.status,
            StrategyStatus::Settled
                | StrategyStatus::Rejected
                | StrategyStatus::Expired
                | StrategyStatus::Cancelled
                | StrategyStatus::RebalanceFailed
        )
}

/// 判断订单是否已经过期并需要结算处理。
fn should_settle_order(order: &StrategyOrder) -> bool {
    Utc::now() >= order.expiry_at
        && matches!(
            order.status,
            StrategyStatus::Running | StrategyStatus::PartiallyFilled | StrategyStatus::Pending
        )
}

/// 判断订单到期是否 ITM。
fn is_in_the_money(order: &StrategyOrder, settlement_price: Decimal) -> bool {
    match order.direction {
        StrategyDirection::SellPut => settlement_price < order.strike,
        StrategyDirection::SellCall => settlement_price > order.strike,
    }
}

/// 合并期权保证金和现货资产行。
fn combined_balances(
    option: &OptionAccountSnapshot,
    spot: &SpotAccountSnapshot,
    earn: &SimpleEarnSnapshot,
    spot_prices: &HashMap<String, Decimal>,
) -> Vec<BalanceRow> {
    let mut balances = strategy::build_balances(option, spot_prices);
    balances.extend(strategy::build_spot_balances(spot, spot_prices));
    balances.extend(strategy::build_simple_earn_balances(earn, spot_prices));
    balances
}

/// 从中文计划字符串中提取执行时间，无法解析时使用默认时间。
fn parse_schedule_time(schedule: &str) -> NaiveTime {
    for segment in schedule
        .split(|character: char| character.is_whitespace() || character == ',' || character == '，')
    {
        if let Some((hour, minute)) = segment.split_once(':')
            && let (Ok(hour), Ok(minute)) = (hour.parse::<u32>(), minute.parse::<u32>())
            && let Some(time) = NaiveTime::from_hms_opt(hour, minute, 0)
        {
            return time;
        }
    }
    NaiveTime::from_hms_opt(11, 30, 0).expect("default auto schedule time is valid")
}

/// 返回 Decimal 规范化后的精度，用于匹配 Binance 合约精度限制。
fn decimal_scale(value: Decimal) -> u32 {
    value.normalize().scale()
}

/// 生成期权订单客户端订单号。
fn make_client_order_id(direction: StrategyDirection) -> String {
    format!(
        "di-{}-{}",
        direction.option_type().to_lowercase(),
        Utc::now().timestamp_millis()
    )
}

/// 生成现货补齐订单客户端订单号。
fn make_spot_client_order_id(direction: StrategyDirection) -> String {
    format!(
        "di-spot-{}-{}",
        direction.option_type().to_lowercase(),
        Utc::now().timestamp_millis()
    )
}

/// 脱敏 API Key，只保留首尾少量字符用于确认连接身份。
fn mask_api_key(api_key: &str) -> String {
    let trimmed = api_key.trim();
    if trimmed.len() <= 8 {
        return "****".to_string();
    }
    let prefix = &trimmed[..4];
    let suffix = &trimmed[trimmed.len() - 4..];
    format!("{prefix}...{suffix}")
}
