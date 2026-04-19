use std::sync::Mutex;

use chrono::Utc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::binance::BinanceClient;
use crate::domain::{
    AppError, AppResult, AppSettings, AppSnapshot, AuditEvent, AutoInvestmentRule, BalanceRow,
    ConnectBinanceRequest, PnlReport, RiskSnapshot, StrategyCandidate, StrategyOrder,
    StrategyRequest, ToggleAutoRequest,
};
use crate::persistence::Persistence;
use crate::strategy;

#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub connected: bool,
    pub api_key_masked: Option<String>,
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
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
            auto_enabled: self.auto_enabled,
            quick_execution: self.quick_execution,
            default_symbol: self.default_symbol.clone(),
        }
    }
}

pub struct AppState {
    runtime: RwLock<RuntimeConfig>,
    auto_rule: RwLock<AutoInvestmentRule>,
    orders: RwLock<Vec<StrategyOrder>>,
    persistence: Mutex<Persistence>,
}

impl AppState {
    pub fn new() -> AppResult<Self> {
        let persistence = Persistence::open_default()?;
        let orders = persistence
            .list_orders()?
            .into_iter()
            .filter(|order| order.exchange_order_id.is_some())
            .collect();

        let state = Self {
            runtime: RwLock::new(RuntimeConfig::default()),
            auto_rule: RwLock::new(strategy::default_auto_rule(false)),
            orders: RwLock::new(orders),
            persistence: Mutex::new(persistence),
        };
        state.record_audit("app.started", "应用已启动，等待 Binance 实盘 API 连接")?;
        Ok(state)
    }

    pub async fn snapshot(&self) -> AppResult<AppSnapshot> {
        let settings = self.runtime.read().await.settings();
        let auto_rule = self.auto_rule.read().await.clone();
        let orders = self.orders.read().await.clone();
        let audit_recent = self
            .persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .recent_audit(20)?;

        Ok(AppSnapshot {
            metrics: strategy::build_metrics(&settings),
            balances: Vec::new(),
            candidates: Vec::new(),
            pnl: strategy::empty_pnl_report(),
            risk: strategy::empty_risk_snapshot(),
            settings,
            auto_rule,
            orders,
            audit_recent,
        })
    }

    pub async fn connect_binance(&self, request: ConnectBinanceRequest) -> AppResult<AppSettings> {
        if request.api_key.trim().is_empty() || request.api_secret.trim().is_empty() {
            return Err(AppError::Config("API Key 和 Secret 不能为空".to_string()));
        }

        let mut runtime = self.runtime.write().await;
        runtime.connected = true;
        runtime.api_key_masked = Some(mask_api_key(&request.api_key));
        runtime.api_key = Some(request.api_key);
        runtime.api_secret = Some(request.api_secret);
        let settings = runtime.settings();
        drop(runtime);

        self.record_audit("account.connected", "已连接 Binance 实盘账户")?;
        Ok(settings)
    }

    pub async fn scan_candidates(&self) -> AppResult<Vec<StrategyCandidate>> {
        self.ensure_connected().await?;
        Err(AppError::Config(
            "实盘候选扫描尚未接入 Binance 行情数据源".to_string(),
        ))
    }

    pub async fn preview_strategy(
        &self,
        request: StrategyRequest,
    ) -> AppResult<crate::domain::StrategyPreview> {
        validate_request(&request)?;
        Ok(strategy::preview_strategy(&request))
    }

    pub async fn place_manual_strategy(
        &self,
        request: StrategyRequest,
    ) -> AppResult<StrategyOrder> {
        validate_request(&request)?;
        let runtime = self.runtime.read().await.clone();
        if !runtime.connected {
            return Err(AppError::Config("请先连接 Binance 实盘 API".to_string()));
        }
        if !request.confirm_live {
            return Err(AppError::RiskRejected(
                "实盘手动下单需要显式确认".to_string(),
            ));
        }

        let api_key = runtime
            .api_key
            .ok_or_else(|| AppError::Config("缺少 Binance API Key".to_string()))?;
        let api_secret = runtime
            .api_secret
            .ok_or_else(|| AppError::Config("缺少 Binance API Secret".to_string()))?;
        let client = BinanceClient::new(api_key, api_secret);
        let ack = client.place_option_order(&request).await?;
        let exchange_order_id = Some(ack.order_id);

        let order = strategy::order_from_request(&request, exchange_order_id);
        {
            let mut orders = self.orders.write().await;
            orders.insert(0, order.clone());
        }
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .save_order(&order)?;
        self.record_audit(
            "strategy.order.created",
            &format!(
                "{} {} {} 张，执行价 {}",
                order.symbol,
                order.direction.display(),
                order.quantity,
                order.strike
            ),
        )?;
        Ok(order)
    }

    pub async fn save_auto_rule(&self, rule: AutoInvestmentRule) -> AppResult<AutoInvestmentRule> {
        validate_auto_rule(&rule)?;
        {
            let mut stored = self.auto_rule.write().await;
            *stored = rule.clone();
        }
        self.record_audit("strategy.rule.saved", "自动定投规则已保存")?;
        Ok(rule)
    }

    pub async fn toggle_auto_strategy(&self, request: ToggleAutoRequest) -> AppResult<AppSettings> {
        let mut runtime = self.runtime.write().await;
        if request.enabled && !runtime.connected {
            return Err(AppError::Config("请先连接 Binance 实盘 API".to_string()));
        }
        if request.enabled && !request.confirm_live {
            return Err(AppError::RiskRejected(
                "实盘自动策略开启需要二次确认".to_string(),
            ));
        }
        runtime.auto_enabled = request.enabled;
        let settings = runtime.settings();
        drop(runtime);

        {
            let mut rule = self.auto_rule.write().await;
            rule.enabled = request.enabled;
        }
        self.record_audit(
            "strategy.auto.toggled",
            if request.enabled {
                "自动定投已启用"
            } else {
                "自动定投已暂停"
            },
        )?;
        Ok(settings)
    }

    pub async fn list_orders(&self) -> AppResult<Vec<StrategyOrder>> {
        Ok(self.orders.read().await.clone())
    }

    pub fn balances(&self) -> Vec<BalanceRow> {
        Vec::new()
    }

    pub fn pnl_report(&self) -> PnlReport {
        strategy::empty_pnl_report()
    }

    pub fn risk_snapshot(&self) -> RiskSnapshot {
        strategy::empty_risk_snapshot()
    }

    pub fn export_audit_logs(&self) -> AppResult<String> {
        let events = self
            .persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .recent_audit(500)?;
        serde_json::to_string_pretty(&events).map_err(|error| AppError::Storage(error.to_string()))
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

    async fn ensure_connected(&self) -> AppResult<()> {
        if self.runtime.read().await.connected {
            Ok(())
        } else {
            Err(AppError::Config("请先连接 Binance 实盘 API".to_string()))
        }
    }
}

fn validate_request(request: &StrategyRequest) -> AppResult<()> {
    if request.symbol != "BTCUSDT" {
        return Err(AppError::RiskRejected(
            "v1 仅支持 BTC/USDT 策略".to_string(),
        ));
    }
    if request.quantity < rust_decimal_macros::dec!(0.01) {
        return Err(AppError::RiskRejected(
            "币安期权最小交易单位按 0.01 张处理".to_string(),
        ));
    }
    if request.strike <= rust_decimal::Decimal::ZERO {
        return Err(AppError::RiskRejected("执行价必须大于 0".to_string()));
    }
    if request.limit_price <= rust_decimal::Decimal::ZERO {
        return Err(AppError::RiskRejected("实盘限价必须大于 0".to_string()));
    }
    Ok(())
}

fn validate_auto_rule(rule: &AutoInvestmentRule) -> AppResult<()> {
    if rule.quantity < rust_decimal_macros::dec!(0.01) {
        return Err(AppError::RiskRejected(
            "自动定投数量不能小于 0.01 张".to_string(),
        ));
    }
    if rule.min_days_to_expiry < 1 || rule.max_days_to_expiry < rule.min_days_to_expiry {
        return Err(AppError::RiskRejected("到期天数区间无效".to_string()));
    }
    Ok(())
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
