use std::{
    future::Future,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration as StdDuration,
};

use chrono::{Duration, Local, NaiveDate, NaiveTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::binance::BinanceClient;
use crate::domain::{
    AppError, AppResult, AppSettings, AppSnapshot, AuditEvent, AutoInvestmentRule, BalanceRow,
    CandidateScanDiagnostics, CandidateScanResult, ConnectBinanceRequest, ExchangeOrderAck,
    OptionAccountSnapshot, OptionContract, OptionMarkPrice, OrderAttempt, OrderAttemptStatus,
    OrderAttemptView, PnlReport, RiskSnapshot, SpotAccountSnapshot, StrategyCandidate,
    StrategyDirection, StrategyOrder, StrategyRequest, StrategyStatus, ToggleAutoRequest,
};
use crate::persistence::Persistence;
use crate::secrets;
use crate::strategy;

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

#[derive(Clone)]
pub struct AppState {
    runtime: Arc<RwLock<RuntimeConfig>>,
    client: Arc<RwLock<Option<BinanceClient>>>,
    candidates: Arc<RwLock<Vec<StrategyCandidate>>>,
    scan_diagnostics: Arc<RwLock<CandidateScanDiagnostics>>,
    auto_rule: Arc<RwLock<AutoInvestmentRule>>,
    orders: Arc<RwLock<Vec<StrategyOrder>>>,
    persistence: Arc<Mutex<Persistence>>,
    worker_started: Arc<AtomicBool>,
    last_auto_scan_day: Arc<RwLock<Option<NaiveDate>>>,
}

impl AppState {
    pub fn new_in(data_dir: impl AsRef<Path>) -> AppResult<Self> {
        Self::from_persistence(Persistence::open_app_data(data_dir)?)
    }

    fn from_persistence(persistence: Persistence) -> AppResult<Self> {
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
            worker_started: Arc::new(AtomicBool::new(false)),
            last_auto_scan_day: Arc::new(RwLock::new(None)),
        };
        state.record_audit("app.started", "应用已启动，等待 Binance API 连接")?;
        Ok(state)
    }

    pub async fn snapshot(&self) -> AppResult<AppSnapshot> {
        let settings = self.runtime.read().await.settings();
        let auto_rule = self.auto_rule.read().await.clone();
        let candidates = self.candidates.read().await.clone();
        let scan_diagnostics = self.scan_diagnostics.read().await.clone();
        let orders = self.orders.read().await.clone();
        let accounts = self.live_accounts_if_connected().await?;
        let (audit_recent, order_attempts) = {
            let persistence = self
                .persistence
                .lock()
                .map_err(|error| AppError::Storage(error.to_string()))?;
            (
                persistence.recent_audit(20)?,
                persistence
                    .list_order_attempts(20)?
                    .into_iter()
                    .map(OrderAttemptView::from)
                    .collect::<Vec<_>>(),
            )
        };
        let option_account = accounts.as_ref().map(|(option, _spot, _price)| option);
        let balances = accounts
            .as_ref()
            .map_or_else(Vec::new, |(option, spot, price)| {
                combined_balances(option, spot, *price)
            });
        let spot_price = accounts
            .as_ref()
            .map(|(_option, _spot, price)| *price)
            .filter(|price| *price > Decimal::ZERO);

        Ok(AppSnapshot {
            metrics: strategy::build_metrics(&settings, option_account, &orders),
            balances,
            candidates,
            scan_diagnostics,
            pnl: strategy::pnl_report_from_orders_with_spot(&orders, spot_price),
            risk: option_account.map_or_else(
                strategy::empty_risk_snapshot,
                strategy::risk_snapshot_from_account,
            ),
            settings,
            auto_rule,
            orders,
            audit_recent,
            order_attempts,
        })
    }

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
        self.record_audit("account.connected", "已连接 Binance 账户")?;
        Ok(settings)
    }

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
        self.record_audit(
            "account.connected.saved",
            "已使用系统 keyring 凭据连接 Binance 账户",
        )?;
        Ok(settings)
    }

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
        self.record_audit(
            "account.credentials.deleted",
            "已从系统 keyring 删除 Binance API 凭据",
        )?;
        Ok(settings)
    }

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
        let spot_price_task = spawn_binance_request({
            let client = client.clone();
            async move { client.spot_price("BTCUSDT").await }
        });
        let auto_rule = self.auto_rule.read().await.clone();
        let orders = self.orders.read().await.clone();
        let account = join_binance_request(account_task).await;
        let contracts = join_binance_request(contracts_task).await;
        let marks = join_binance_request(marks_task).await;
        let spot_price = join_binance_request(spot_price_task).await;
        let account = account?;
        let contracts = contracts?;
        let marks = marks?;
        let spot_price = spot_price?;
        let result = strategy::build_candidates(
            &contracts, &marks, &auto_rule, &account, &orders, spot_price,
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

    pub fn preview_strategy(
        request: &StrategyRequest,
    ) -> AppResult<crate::domain::StrategyPreview> {
        validate_request(request)?;
        Ok(strategy::preview_strategy(request))
    }

    pub async fn place_manual_strategy(
        &self,
        request: StrategyRequest,
    ) -> AppResult<StrategyOrder> {
        validate_request(&request)?;
        if !request.confirm_live {
            return Err(AppError::RiskRejected("手动下单需要显式确认".to_string()));
        }
        self.place_live_strategy(request, "manual").await
    }

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
        self.record_audit("strategy.rule.saved", "自动定投规则已保存")?;
        Ok(rule)
    }

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
        self.record_audit(
            "strategy.auto.toggled",
            if request.enabled {
                "自动定投已启用，后台调度器会按规则扫描并自动下单"
            } else {
                "自动定投已暂停"
            },
        )?;
        Ok(settings)
    }

    pub async fn list_orders(&self) -> AppResult<Vec<StrategyOrder>> {
        Ok(self.orders.read().await.clone())
    }

    pub fn list_order_attempts(&self) -> AppResult<Vec<OrderAttemptView>> {
        Ok(self
            .persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .list_order_attempts(100)?
            .into_iter()
            .map(OrderAttemptView::from)
            .collect())
    }

    pub async fn recover_order_attempt(&self, attempt_id: String) -> AppResult<StrategyOrder> {
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

    pub async fn balances(&self) -> AppResult<Vec<BalanceRow>> {
        let client = self.live_client().await?;
        let option_task = spawn_binance_request({
            let client = client.clone();
            async move { client.option_account().await }
        });
        let spot_task = spawn_binance_request({
            let client = client.clone();
            async move { client.spot_account().await }
        });
        let spot_price_task = spawn_binance_request({
            let client = client.clone();
            async move { client.spot_price("BTCUSDT").await }
        });
        let option = join_binance_request(option_task).await;
        let spot = join_binance_request(spot_task).await;
        let spot_price = join_binance_request(spot_price_task).await;
        let option = option?;
        let spot = spot?;
        let spot_price = spot_price?;
        Ok(combined_balances(&option, &spot, spot_price))
    }

    pub fn pnl_report(&self) -> PnlReport {
        let orders = self
            .orders
            .try_read()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        strategy::pnl_report_from_orders(&orders)
    }

    pub async fn risk_snapshot(&self) -> AppResult<RiskSnapshot> {
        let account = self.live_client().await?.option_account().await?;
        Ok(strategy::risk_snapshot_from_account(&account))
    }

    pub async fn sync_live_state(&self) -> AppResult<AppSnapshot> {
        self.sync_live_orders().await?;
        self.settle_due_orders().await?;
        self.snapshot().await
    }

    pub async fn run_settlement_scan(&self) -> AppResult<Vec<StrategyOrder>> {
        self.sync_live_orders().await?;
        self.settle_due_orders().await
    }

    pub async fn run_auto_once(&self) -> AppResult<Option<StrategyOrder>> {
        let runtime = self.runtime.read().await.clone();
        if !runtime.connected {
            return Err(AppError::Config("请先连接 Binance API".to_string()));
        }
        if !runtime.auto_enabled {
            return Err(AppError::RiskRejected("自动定投未启用".to_string()));
        }
        self.execute_auto_strategy().await
    }

    pub fn export_audit_logs(&self) -> AppResult<String> {
        let events = self
            .persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .recent_audit(500)?;
        serde_json::to_string_pretty(&events).map_err(|error| AppError::Storage(error.to_string()))
    }

    #[allow(clippy::too_many_lines)]
    async fn place_live_strategy(
        &self,
        mut request: StrategyRequest,
        source: &str,
    ) -> AppResult<StrategyOrder> {
        validate_request(&request)?;
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
        let account = join_binance_request(account_task).await;
        let contracts = join_binance_request(contracts_task).await;
        let marks = join_binance_request(marks_task).await;
        let account = account?;
        let contracts = contracts?;
        let marks = marks?;
        let orders = self.orders.read().await.clone();
        let contract = resolve_contract(&request, &contracts)?;
        request.option_symbol = Some(contract.symbol.clone());
        validate_contract_order(&request, contract, &account, &marks)?;
        enforce_risk_rules(&request, contract, &account, &orders)?;

        let client_order_id = make_client_order_id(request.direction);
        let now = Utc::now();
        let mut attempt = OrderAttempt {
            id: Uuid::new_v4().to_string(),
            client_order_id: client_order_id.clone(),
            symbol: request.symbol.clone(),
            option_symbol: contract.symbol.clone(),
            direction: request.direction,
            status: OrderAttemptStatus::Attempted,
            exchange_order_id: None,
            message: "已通过本地风控，准备发送 Binance 订单".to_string(),
            created_at: now,
            updated_at: now,
        };
        self.save_order_attempt(&attempt)?;
        self.record_audit(
            &format!("strategy.order.{source}.attempted"),
            &format!(
                "{} {} {} 张，合约 {}，单位限价 {}",
                request.symbol,
                request.direction.display(),
                request.quantity,
                contract.symbol,
                request.limit_price
            ),
        )?;

        let ack = client
            .place_option_order(&request, &client_order_id)
            .await?;
        attempt.status = OrderAttemptStatus::ExchangeAck;
        attempt.exchange_order_id = Some(ack.order_id.clone());
        attempt.message = format!("Binance 已返回订单号，状态 {}", ack.status);
        attempt.updated_at = Utc::now();
        self.save_order_attempt(&attempt)?;

        let order = strategy::order_from_request(&request, &ack);
        if let Err(error) = self.save_order(&order).await {
            attempt.status = OrderAttemptStatus::PersistFailed;
            attempt.message = "交易所订单已创建，但本地订单保存失败".to_string();
            attempt.updated_at = Utc::now();
            self.save_order_attempt(&attempt)?;
            return Err(AppError::Storage(format!(
                "交易所订单已创建但本地保存失败，exchangeOrderId={}，clientOrderId={}。请先用 Binance 后台核对后再继续操作。原始错误：{}",
                ack.order_id, client_order_id, error
            )));
        }

        attempt.status = OrderAttemptStatus::Persisted;
        attempt.message = "交易所订单和本地订单均已记录".to_string();
        attempt.updated_at = Utc::now();
        self.save_order_attempt(&attempt)?;
        self.record_audit(
            &format!("strategy.order.{source}.created"),
            &format!(
                "{} {} {} 张，执行价 {}，交易所状态 {}",
                order.symbol,
                order.direction.display(),
                order.quantity,
                order.strike,
                order.exchange_status
            ),
        )?;
        Ok(order)
    }

    async fn recover_option_attempt(&self, mut attempt: OrderAttempt) -> AppResult<StrategyOrder> {
        let client = self.live_client().await?;
        let ack = client
            .query_option_order(
                &attempt.option_symbol,
                attempt.exchange_order_id.as_deref(),
                Some(&attempt.client_order_id),
            )
            .await?;
        let request = request_from_option_attempt(&attempt, &ack)?;
        let order = strategy::order_from_request(&request, &ack);
        self.save_order(&order).await?;
        attempt.status = OrderAttemptStatus::Persisted;
        attempt.exchange_order_id = Some(ack.order_id.clone());
        attempt.message = "已从 Binance 查询订单并恢复到本地订单表".to_string();
        attempt.updated_at = Utc::now();
        self.save_order_attempt(&attempt)?;
        self.record_audit(
            "strategy.order.recovered",
            &format!("{} 已从恢复记录重建本地订单", attempt.option_symbol),
        )?;
        Ok(order)
    }

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
            AppError::Storage("未找到现货补齐对应的本地期权订单，无法自动恢复".to_string())
        })?;

        order.spot_order_id = Some(ack.order_id.clone());
        order.spot_client_order_id = attempt.client_order_id.clone();
        apply_spot_ack(&mut order, &ack);
        self.save_order(&order).await?;
        attempt.status = OrderAttemptStatus::SpotPersisted;
        attempt.exchange_order_id = Some(ack.order_id.clone());
        attempt.message = "已从 Binance 查询现货订单并恢复本地补齐状态".to_string();
        attempt.updated_at = Utc::now();
        self.save_order_attempt(&attempt)?;
        self.record_audit(
            "strategy.rebalance.recovered",
            &format!("{} 已从恢复记录重建现货补齐状态", attempt.option_symbol),
        )?;
        Ok(order)
    }

    async fn sync_live_orders(&self) -> AppResult<Vec<StrategyOrder>> {
        let client = self.live_client().await?;
        let orders = self.orders.read().await.clone();
        let mut updated = Vec::new();

        for order in orders.into_iter().filter(should_sync_option_order) {
            match self.sync_single_order(&client, order.clone()).await {
                Ok(order) => updated.push(order),
                Err(error) => {
                    self.record_audit(
                        "strategy.order.sync_failed",
                        &format!("{} 同步失败：{}", order.option_symbol, error),
                    )?;
                }
            }
        }
        Ok(updated)
    }

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

            let settlement_price = self.settlement_price(&client, &order).await?;
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
                self.record_audit(
                    "strategy.settlement.otm",
                    &format!("{} 到期 OTM，无需现货补齐", order.option_symbol),
                )?;
                updated.push(order);
                continue;
            }

            let order = self.rebalance_spot_if_needed(&client, order).await?;
            updated.push(order);
        }
        Ok(updated)
    }

    async fn settlement_price(
        &self,
        client: &BinanceClient,
        order: &StrategyOrder,
    ) -> AppResult<Decimal> {
        let start = order
            .expiry
            .and_time(NaiveTime::MIN)
            .and_utc()
            .timestamp_millis();
        let end = (order.expiry + Duration::days(2))
            .and_time(NaiveTime::MIN)
            .and_utc()
            .timestamp_millis();
        if let Ok(records) = client
            .exercise_records(&order.option_symbol, start, end)
            .await
            && let Some(record) = records.into_iter().find(|record| {
                record.symbol == order.option_symbol && record.settlement_price > Decimal::ZERO
            })
        {
            return Ok(record.settlement_price);
        }
        client.spot_price("BTCUSDT").await
    }

    async fn rebalance_spot_if_needed(
        &self,
        client: &BinanceClient,
        mut order: StrategyOrder,
    ) -> AppResult<StrategyOrder> {
        if order.spot_order_id.is_some() {
            self.sync_spot_rebalance(client, &mut order).await?;
            self.save_order(&order).await?;
            return Ok(order);
        }

        let spot_account = client.spot_account().await?;
        if let Err(error) = validate_spot_rebalance(&order, &spot_account) {
            order.status = StrategyStatus::RebalanceFailed;
            order.updated_at = Utc::now();
            order.rebalance_message = format!("现货补齐风控失败：{error}");
            self.save_order(&order).await?;
            self.record_audit(
                "strategy.rebalance.failed",
                &format!("{} {}", order.option_symbol, order.rebalance_message),
            )?;
            return Ok(order);
        }
        let side = crate::binance::spot_rebalance_side(order.direction);
        let client_order_id = make_spot_client_order_id(order.direction);
        let now = Utc::now();
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
                order.status = StrategyStatus::RebalanceFailed;
                order.updated_at = Utc::now();
                order.rebalance_message = format!("现货补齐下单失败：{error}");
                self.save_order(&order).await?;
                self.record_audit(
                    "strategy.rebalance.failed",
                    &format!("{} {}", order.option_symbol, order.rebalance_message),
                )?;
                return Ok(order);
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
            attempt.message = "交易所现货订单已创建，但本地补齐状态保存失败".to_string();
            attempt.updated_at = Utc::now();
            self.save_order_attempt(&attempt)?;
            return Err(AppError::Storage(format!(
                "交易所现货订单已创建但本地保存失败，spotOrderId={}，clientOrderId={}。请先用 Binance 后台核对后再继续操作。原始错误：{}",
                order.spot_order_id.as_deref().unwrap_or(""),
                order.spot_client_order_id,
                error
            )));
        }
        attempt.status = OrderAttemptStatus::SpotPersisted;
        attempt.message = "交易所现货订单和本地补齐状态均已记录".to_string();
        attempt.updated_at = Utc::now();
        self.save_order_attempt(&attempt)?;
        self.record_audit(
            "strategy.rebalance.created",
            &format!("{} {}", order.option_symbol, order.rebalance_message),
        )?;
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

    async fn execute_auto_strategy(&self) -> AppResult<Option<StrategyOrder>> {
        let result = self.scan_candidates().await?;
        let Some(candidate) = result.candidates.into_iter().find(|candidate| {
            candidate.recommendation == crate::domain::RecommendationLevel::Recommended
        }) else {
            self.record_audit("strategy.auto.no_candidate", "本次自动扫描无推荐候选")?;
            return Ok(None);
        };

        let order = self
            .place_live_strategy(
                StrategyRequest {
                    candidate_id: Some(candidate.id),
                    symbol: candidate.symbol,
                    option_symbol: Some(candidate.option_symbol),
                    direction: candidate.direction,
                    expiry: candidate.expiry,
                    strike: candidate.strike,
                    quantity: candidate.quantity,
                    limit_price: candidate.limit_price,
                    confirm_live: true,
                },
                "auto",
            )
            .await?;
        Ok(Some(order))
    }

    async fn background_tick(&self) -> AppResult<()> {
        if !self.runtime.read().await.connected {
            return Ok(());
        }
        self.sync_live_orders().await?;
        self.settle_due_orders().await?;
        if self.should_run_auto_now().await {
            let _ = self.execute_auto_strategy().await?;
        }
        Ok(())
    }

    async fn should_run_auto_now(&self) -> bool {
        let runtime = self.runtime.read().await.clone();
        if !runtime.connected || !runtime.auto_enabled {
            return false;
        }
        let rule = self.auto_rule.read().await.clone();
        if !rule.enabled {
            return false;
        }
        let now = Local::now();
        let scheduled = parse_schedule_time(&rule.schedule);
        if now.time() < scheduled {
            return false;
        }
        let today = now.date_naive();
        let mut last = self.last_auto_scan_day.write().await;
        if *last == Some(today) {
            return false;
        }
        *last = Some(today);
        true
    }

    fn ensure_background_worker(&self) {
        if self.worker_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let state = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut interval = tokio::time::interval(StdDuration::from_mins(1));
            loop {
                interval.tick().await;
                if let Err(error) = state.background_tick().await {
                    let _ = state.record_audit("background.tick_failed", &error.to_string());
                }
            }
        });
    }

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

    fn record_audit(&self, kind: &str, message: &str) -> AppResult<()> {
        let event = AuditEvent {
            id: Uuid::new_v4().to_string(),
            kind: kind.to_string(),
            message: message.to_string(),
            created_at: Utc::now(),
        };
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .append_audit(&event)
    }

    fn save_order_attempt(&self, attempt: &OrderAttempt) -> AppResult<()> {
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .save_order_attempt(attempt)
    }

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

    async fn live_accounts_if_connected(
        &self,
    ) -> AppResult<Option<(OptionAccountSnapshot, SpotAccountSnapshot, Decimal)>> {
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
        let spot_price_task = spawn_binance_request({
            let client = client.clone();
            async move { client.spot_price("BTCUSDT").await }
        });
        let option_account = join_binance_request(option_account_task).await;
        let spot_account = join_binance_request(spot_account_task).await;
        let spot_price = join_binance_request(spot_price_task).await;
        let option_account = option_account?;
        let spot_account = spot_account?;
        let spot_price = spot_price?;
        Ok(Some((option_account, spot_account, spot_price)))
    }
}

fn spawn_binance_request<T, F>(future: F) -> tauri::async_runtime::JoinHandle<AppResult<T>>
where
    T: Send + 'static,
    F: Future<Output = AppResult<T>> + Send + 'static,
{
    tauri::async_runtime::spawn(future)
}

async fn join_binance_request<T>(
    handle: tauri::async_runtime::JoinHandle<AppResult<T>>,
) -> AppResult<T> {
    handle
        .await
        .map_err(|error| AppError::Binance(format!("Binance 异步请求任务失败：{error}")))?
}

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

fn request_from_option_attempt(
    attempt: &OrderAttempt,
    ack: &ExchangeOrderAck,
) -> AppResult<StrategyRequest> {
    let (expiry, strike, direction) = parse_option_symbol(&attempt.option_symbol)?;
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
        symbol: attempt.symbol.clone(),
        option_symbol: Some(attempt.option_symbol.clone()),
        direction,
        expiry,
        strike,
        quantity,
        limit_price,
        confirm_live: true,
    })
}

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

fn validate_contract_order(
    request: &StrategyRequest,
    contract: &OptionContract,
    account: &OptionAccountSnapshot,
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
    if decimal_scale(request.limit_price) > contract.price_scale {
        return Err(AppError::RiskRejected(format!(
            "限价精度超过 Binance 合约限制 {} 位",
            contract.price_scale
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

fn enforce_risk_rules(
    request: &StrategyRequest,
    contract: &OptionContract,
    account: &OptionAccountSnapshot,
    orders: &[StrategyOrder],
) -> AppResult<()> {
    let rules = strategy::risk_snapshot_from_account(account).rules;
    let required = strategy::required_margin(contract, request.direction, request.quantity);
    if required > rules.max_single_order_usdt {
        return Err(AppError::RiskRejected(format!(
            "单笔预计保证金 {} 超过风控上限 {}",
            required.round_dp(2),
            rules.max_single_order_usdt.round_dp(2)
        )));
    }

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

fn validate_spot_rebalance(order: &StrategyOrder, account: &SpotAccountSnapshot) -> AppResult<()> {
    if !account.can_trade {
        return Err(AppError::RiskRejected(
            "现货账户不可交易，停止到期补齐".to_string(),
        ));
    }
    match order.direction {
        StrategyDirection::SellPut => {
            let available = account
                .asset("USDT")
                .map_or(Decimal::ZERO, |asset| asset.free);
            let required = order.settlement_price * order.filled_quantity * dec!(1.01);
            if available < required {
                return Err(AppError::RiskRejected(format!(
                    "现货补齐 USDT 不足，预计需要 {}，当前可用 {}",
                    required.round_dp(2),
                    available.round_dp(2)
                )));
            }
        }
        StrategyDirection::SellCall => {
            let available = account
                .asset("BTC")
                .map_or(Decimal::ZERO, |asset| asset.free);
            if available < order.filled_quantity {
                return Err(AppError::RiskRejected(format!(
                    "现货补齐 BTC 不足，预计需要 {}，当前可用 {}",
                    order.filled_quantity, available
                )));
            }
        }
    }
    Ok(())
}

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

fn should_settle_order(order: &StrategyOrder) -> bool {
    Utc::now().date_naive() > order.expiry
        && matches!(
            order.status,
            StrategyStatus::Running | StrategyStatus::PartiallyFilled | StrategyStatus::Pending
        )
}

fn is_in_the_money(order: &StrategyOrder, settlement_price: Decimal) -> bool {
    match order.direction {
        StrategyDirection::SellPut => settlement_price < order.strike,
        StrategyDirection::SellCall => settlement_price > order.strike,
    }
}

fn combined_balances(
    option: &OptionAccountSnapshot,
    spot: &SpotAccountSnapshot,
    spot_price: Decimal,
) -> Vec<BalanceRow> {
    let mut balances = strategy::build_balances(option);
    balances.extend(strategy::build_spot_balances(spot, spot_price));
    balances
}

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

fn decimal_scale(value: Decimal) -> u32 {
    value.normalize().scale()
}

fn make_client_order_id(direction: StrategyDirection) -> String {
    format!(
        "di-{}-{}",
        direction.option_type().to_lowercase(),
        Utc::now().timestamp_millis()
    )
}

fn make_spot_client_order_id(direction: StrategyDirection) -> String {
    format!(
        "di-spot-{}-{}",
        direction.option_type().to_lowercase(),
        Utc::now().timestamp_millis()
    )
}

fn mask_api_key(api_key: &str) -> String {
    let trimmed = api_key.trim();
    if trimmed.len() <= 8 {
        return "****".to_string();
    }
    let prefix = &trimmed[..4];
    let suffix = &trimmed[trimmed.len() - 4..];
    format!("{prefix}...{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;

    #[test]
    fn risk_rules_reject_single_order_over_usdt_limit() {
        let expiry = Utc::now().date_naive() + ChronoDuration::days(14);
        let request = strategy_request(StrategyDirection::SellPut, expiry);
        let contract = option_contract("PUT", expiry);
        let account = option_account(dec!(10_000), Decimal::ZERO, dec!(10_000));

        let error = enforce_risk_rules(&request, &contract, &account, &[])
            .expect_err("sell put notional should exceed default single-order limit");

        assert!(error.to_string().contains("单笔预计保证金"));
    }

    #[test]
    fn spot_rebalance_rejects_missing_btc_for_sell_call() {
        let mut order = strategy_order(StrategyDirection::SellCall);
        order.filled_quantity = dec!(0.01);
        order.settlement_price = dec!(80_000);
        let account = spot_account(dec!(10_000), dec!(0.005), true);

        let error = validate_spot_rebalance(&order, &account)
            .expect_err("sell call rebalance needs enough BTC");

        assert!(error.to_string().contains("BTC 不足"));
    }

    #[test]
    fn itm_detection_matches_direction() {
        let mut sell_put = strategy_order(StrategyDirection::SellPut);
        sell_put.strike = dec!(72_000);
        let mut sell_call = strategy_order(StrategyDirection::SellCall);
        sell_call.strike = dec!(72_000);

        assert!(is_in_the_money(&sell_put, dec!(70_000)));
        assert!(!is_in_the_money(&sell_put, dec!(73_000)));
        assert!(is_in_the_money(&sell_call, dec!(73_000)));
        assert!(!is_in_the_money(&sell_call, dec!(70_000)));
    }

    #[test]
    fn option_ack_updates_partial_fill_amounts() {
        let mut order = strategy_order(StrategyDirection::SellPut);
        let ack = ExchangeOrderAck {
            order_id: "4611875134427365378".to_string(),
            client_order_id: "di-put-2".to_string(),
            symbol: "BTC-260626-72000-P".to_string(),
            status: "CANCELED".to_string(),
            executed_qty: dec!(0.005),
            avg_price: dec!(200),
            price: dec!(200),
            quantity: dec!(0.01),
            fee: dec!(0.01),
            create_time: None,
            update_time: Some(Utc::now()),
        };

        apply_option_ack(&mut order, &ack);

        assert_eq!(order.status, StrategyStatus::PartiallyFilled);
        assert_eq!(order.filled_quantity, dec!(0.005));
        assert_eq!(order.premium, dec!(1.00));
        assert_eq!(order.estimated_pnl, dec!(0.99));
    }

    #[test]
    fn spot_ack_partial_fill_does_not_settle_order() {
        let mut order = strategy_order(StrategyDirection::SellPut);
        order.filled_quantity = dec!(0.01);
        let ack = ExchangeOrderAck {
            order_id: "8751755144427365001".to_string(),
            client_order_id: "di-spot-put-1".to_string(),
            symbol: "BTCUSDT".to_string(),
            status: "PARTIALLY_FILLED".to_string(),
            executed_qty: dec!(0.005),
            avg_price: dec!(70000),
            price: Decimal::ZERO,
            quantity: dec!(0.01),
            fee: dec!(0.1),
            create_time: None,
            update_time: Some(Utc::now()),
        };

        apply_spot_ack(&mut order, &ack);

        assert_eq!(order.status, StrategyStatus::RebalanceFailed);
        assert_eq!(order.spot_filled_quantity, dec!(0.005));
        assert_eq!(order.spot_avg_price, dec!(70000));
        assert_eq!(order.spot_fee, dec!(0.1));
        assert!(order.settled_at.is_none());
        assert!(order.rebalance_message.contains("尚未足额成交"));
    }

    #[test]
    fn parse_option_symbol_recovers_expiry_strike_and_direction() {
        let (expiry, strike, direction) =
            parse_option_symbol("BTC-260626-72000-P").expect("valid option symbol");

        assert_eq!(
            expiry,
            NaiveDate::from_ymd_opt(2026, 6, 26).expect("valid test date"),
        );
        assert_eq!(strike, dec!(72000));
        assert_eq!(direction, StrategyDirection::SellPut);
    }

    #[test]
    fn schedule_parser_uses_configured_time_or_default() {
        assert_eq!(
            parse_schedule_time("每天 11:30"),
            NaiveTime::from_hms_opt(11, 30, 0).expect("valid test time"),
        );
        assert_eq!(
            parse_schedule_time("bad schedule"),
            NaiveTime::from_hms_opt(11, 30, 0).expect("valid default time"),
        );
    }

    fn strategy_request(direction: StrategyDirection, expiry: NaiveDate) -> StrategyRequest {
        StrategyRequest {
            candidate_id: None,
            symbol: "BTCUSDT".to_string(),
            option_symbol: Some("BTC-260626-72000-P".to_string()),
            direction,
            expiry,
            strike: dec!(72_000),
            quantity: dec!(0.01),
            limit_price: dec!(200),
            confirm_live: true,
        }
    }

    fn strategy_order(direction: StrategyDirection) -> StrategyOrder {
        let expiry = Utc::now().date_naive() - ChronoDuration::days(1);
        StrategyOrder {
            id: "LIVE-test".to_string(),
            exchange_order_id: Some("4611875134427365378".to_string()),
            client_order_id: "di-test".to_string(),
            symbol: "BTCUSDT".to_string(),
            option_symbol: "BTC-260626-72000-P".to_string(),
            direction,
            status: StrategyStatus::Running,
            expiry,
            strike: dec!(72_000),
            quantity: dec!(0.01),
            filled_quantity: dec!(0.01),
            avg_price: dec!(200),
            fee: Decimal::ZERO,
            premium: dec!(2),
            estimated_pnl: dec!(2),
            breakeven: dec!(71_800),
            exchange_status: "FILLED".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            settlement_action: "test".to_string(),
            settlement_price: Decimal::ZERO,
            settled_at: None,
            spot_order_id: None,
            spot_client_order_id: String::new(),
            spot_order_status: String::new(),
            rebalance_message: String::new(),
            spot_filled_quantity: Decimal::ZERO,
            spot_avg_price: Decimal::ZERO,
            spot_fee: Decimal::ZERO,
        }
    }

    fn option_account(
        available: Decimal,
        initial_margin: Decimal,
        adjusted_equity: Decimal,
    ) -> OptionAccountSnapshot {
        OptionAccountSnapshot {
            assets: vec![crate::domain::OptionAssetBalance {
                asset: "USDT".to_string(),
                margin_balance: adjusted_equity,
                equity: adjusted_equity,
                available,
                initial_margin,
                maint_margin: Decimal::ZERO,
                unrealized_pnl: Decimal::ZERO,
                adjusted_equity,
            }],
            can_trade: true,
            time: None,
        }
    }

    fn spot_account(usdt_free: Decimal, btc_free: Decimal, can_trade: bool) -> SpotAccountSnapshot {
        SpotAccountSnapshot {
            can_trade,
            balances: vec![
                crate::domain::SpotAssetBalance {
                    asset: "USDT".to_string(),
                    free: usdt_free,
                    locked: Decimal::ZERO,
                },
                crate::domain::SpotAssetBalance {
                    asset: "BTC".to_string(),
                    free: btc_free,
                    locked: Decimal::ZERO,
                },
            ],
        }
    }

    fn option_contract(side: &str, expiry: NaiveDate) -> OptionContract {
        OptionContract {
            symbol: "BTC-260626-72000-P".to_string(),
            side: side.to_string(),
            strike_price: dec!(72_000),
            underlying: "BTCUSDT".to_string(),
            unit: Decimal::ONE,
            min_qty: dec!(0.01),
            max_qty: dec!(10),
            initial_margin: dec!(0.10),
            maintenance_margin: dec!(0.05),
            min_initial_margin: dec!(10),
            min_maintenance_margin: dec!(5),
            price_scale: 2,
            quantity_scale: 2,
            quote_asset: "USDT".to_string(),
            status: "TRADING".to_string(),
            expiry,
        }
    }
}
