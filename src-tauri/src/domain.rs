use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("配置错误：{0}")]
    Config(String),
    #[error("交易被风控拒绝：{0}")]
    RiskRejected(String),
    #[error("币安接口错误：{0}")]
    Binance(String),
    #[error("数据库错误：{0}")]
    Storage(String),
}

pub type AppResult<T> = Result<T, AppError>;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StrategyDirection {
    SellPut,
    SellCall,
}

impl StrategyDirection {
    pub const fn option_type(self) -> &'static str {
        match self {
            Self::SellPut => "PUT",
            Self::SellCall => "CALL",
        }
    }

    pub const fn display(self) -> &'static str {
        match self {
            Self::SellPut => "卖 Put 抄底",
            Self::SellCall => "卖 Call 止盈",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StrategyStatus {
    Pending,
    Running,
    Settled,
    Cancelled,
    Rejected,
    Expired,
    PartiallyFilled,
    RebalanceFailed,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecommendationLevel {
    Recommended,
    Watch,
    Reject,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RiskSeverity {
    Low,
    Medium,
    High,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[allow(clippy::struct_excessive_bools)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub connected: bool,
    pub api_key_masked: Option<String>,
    #[serde(default)]
    pub saved_credentials: bool,
    pub auto_enabled: bool,
    pub quick_execution: bool,
    pub default_symbol: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricCard {
    pub id: String,
    pub title: String,
    pub value: String,
    pub sub_value: String,
    pub change: String,
    pub icon: String,
    pub tone: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceRow {
    pub asset: String,
    #[serde(default)]
    pub source: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub free: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub locked: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub usdt_value: Decimal,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyCandidate {
    pub id: String,
    pub symbol: String,
    pub option_symbol: String,
    pub direction: StrategyDirection,
    pub expiry: NaiveDate,
    #[serde(with = "rust_decimal::serde::str")]
    pub strike: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub limit_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub premium: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub annualized: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub delta_abs: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub iv_rank: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub exercise_probability: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub breakeven: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub spot_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub strike_distance_percent: Decimal,
    pub recommendation: RecommendationLevel,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateScanDiagnostics {
    pub raw_contracts: usize,
    pub raw_marks: usize,
    pub btc_contracts: usize,
    pub trading_contracts: usize,
    pub direction_matched: usize,
    pub expiry_matched: usize,
    pub mark_matched: usize,
    pub quantity_matched: usize,
    pub returned_candidates: usize,
    pub returned_sell_put: usize,
    pub returned_sell_call: usize,
    pub browse_min_days: i64,
    pub browse_max_days: i64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateScanResult {
    pub candidates: Vec<StrategyCandidate>,
    pub diagnostics: CandidateScanDiagnostics,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyOrder {
    pub id: String,
    pub exchange_order_id: Option<String>,
    #[serde(default)]
    pub client_order_id: String,
    pub symbol: String,
    pub option_symbol: String,
    pub direction: StrategyDirection,
    pub status: StrategyStatus,
    pub expiry: NaiveDate,
    #[serde(with = "rust_decimal::serde::str")]
    pub strike: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
    #[serde(default, with = "rust_decimal::serde::str")]
    pub filled_quantity: Decimal,
    #[serde(default, with = "rust_decimal::serde::str")]
    pub avg_price: Decimal,
    #[serde(default, with = "rust_decimal::serde::str")]
    pub fee: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub premium: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub estimated_pnl: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub breakeven: Decimal,
    #[serde(default)]
    pub exchange_status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub settlement_action: String,
    #[serde(default, with = "rust_decimal::serde::str")]
    pub settlement_price: Decimal,
    #[serde(default)]
    pub settled_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub spot_order_id: Option<String>,
    #[serde(default)]
    pub spot_client_order_id: String,
    #[serde(default)]
    pub spot_order_status: String,
    #[serde(default)]
    pub rebalance_message: String,
    #[serde(default, with = "rust_decimal::serde::str")]
    pub spot_filled_quantity: Decimal,
    #[serde(default, with = "rust_decimal::serde::str")]
    pub spot_avg_price: Decimal,
    #[serde(default, with = "rust_decimal::serde::str")]
    pub spot_fee: Decimal,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoInvestmentRule {
    pub enabled: bool,
    pub schedule: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub min_annualized: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub max_delta_abs: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub max_iv_rank: Decimal,
    pub min_days_to_expiry: i64,
    pub max_days_to_expiry: i64,
    pub include_btc: bool,
    pub exclude_same_direction: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskRuleSet {
    #[serde(with = "rust_decimal::serde::str")]
    pub max_single_order_usdt: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub max_daily_orders: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub max_margin_usage: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub max_drawdown: Decimal,
    pub stop_after_consecutive_losses: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskAlert {
    pub id: String,
    pub severity: RiskSeverity,
    pub title: String,
    pub message: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskSnapshot {
    pub score: u32,
    pub level: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub margin_usage: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub exercise_risk: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub daily_drawdown: Decimal,
    pub rules: RiskRuleSet,
    pub alerts: Vec<RiskAlert>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartPoint {
    pub label: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub value: Decimal,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PnlBreakdown {
    pub label: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub value: Decimal,
    pub color: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PnlReport {
    #[serde(with = "rust_decimal::serde::str")]
    pub cumulative: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub today: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub option_premium: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub realized_spot: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub floating_spot: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub fees: Decimal,
    pub points: Vec<ChartPoint>,
    pub breakdown: Vec<PnlBreakdown>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEvent {
    pub id: String,
    pub kind: String,
    pub message: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderAttemptStatus {
    Attempted,
    ExchangeAck,
    Persisted,
    PersistFailed,
    SpotAttempted,
    SpotExchangeAck,
    SpotPersisted,
    SpotPersistFailed,
}

impl OrderAttemptStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Attempted => "attempted",
            Self::ExchangeAck => "exchange_ack",
            Self::Persisted => "persisted",
            Self::PersistFailed => "persist_failed",
            Self::SpotAttempted => "spot_attempted",
            Self::SpotExchangeAck => "spot_exchange_ack",
            Self::SpotPersisted => "spot_persisted",
            Self::SpotPersistFailed => "spot_persist_failed",
        }
    }
}

#[derive(Clone, Debug)]
pub struct OrderAttempt {
    pub id: String,
    pub client_order_id: String,
    pub symbol: String,
    pub option_symbol: String,
    pub direction: StrategyDirection,
    pub status: OrderAttemptStatus,
    pub exchange_order_id: Option<String>,
    pub message: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderAttemptView {
    pub id: String,
    pub client_order_id: String,
    pub symbol: String,
    pub option_symbol: String,
    pub direction: StrategyDirection,
    pub status: String,
    pub exchange_order_id: Option<String>,
    pub message: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<OrderAttempt> for OrderAttemptView {
    fn from(attempt: OrderAttempt) -> Self {
        Self {
            id: attempt.id,
            client_order_id: attempt.client_order_id,
            symbol: attempt.symbol,
            option_symbol: attempt.option_symbol,
            direction: attempt.direction,
            status: attempt.status.as_str().to_string(),
            exchange_order_id: attempt.exchange_order_id,
            message: attempt.message,
            created_at: attempt.created_at,
            updated_at: attempt.updated_at,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OptionAccountSnapshot {
    pub assets: Vec<OptionAssetBalance>,
    pub can_trade: bool,
    pub time: Option<DateTime<Utc>>,
}

impl OptionAccountSnapshot {
    pub fn asset(&self, asset: &str) -> Option<&OptionAssetBalance> {
        self.assets.iter().find(|row| row.asset == asset)
    }
}

#[derive(Clone, Debug)]
pub struct SpotAccountSnapshot {
    pub balances: Vec<SpotAssetBalance>,
    pub can_trade: bool,
}

impl SpotAccountSnapshot {
    pub fn asset(&self, asset: &str) -> Option<&SpotAssetBalance> {
        self.balances.iter().find(|row| row.asset == asset)
    }
}

#[derive(Clone, Debug)]
pub struct SpotAssetBalance {
    pub asset: String,
    pub free: Decimal,
    pub locked: Decimal,
}

#[derive(Clone, Debug)]
pub struct OptionAssetBalance {
    pub asset: String,
    pub margin_balance: Decimal,
    pub equity: Decimal,
    pub available: Decimal,
    pub initial_margin: Decimal,
    pub maint_margin: Decimal,
    pub unrealized_pnl: Decimal,
    pub adjusted_equity: Decimal,
}

#[derive(Clone, Debug)]
pub struct OptionContract {
    pub symbol: String,
    pub side: String,
    pub strike_price: Decimal,
    pub underlying: String,
    pub unit: Decimal,
    pub min_qty: Decimal,
    pub max_qty: Decimal,
    pub initial_margin: Decimal,
    pub maintenance_margin: Decimal,
    pub min_initial_margin: Decimal,
    pub min_maintenance_margin: Decimal,
    pub price_scale: u32,
    pub quantity_scale: u32,
    pub quote_asset: String,
    pub status: String,
    pub expiry: NaiveDate,
}

#[derive(Clone, Debug)]
pub struct OptionMarkPrice {
    pub symbol: String,
    pub mark_price: Decimal,
    pub mark_iv: Decimal,
    pub delta: Decimal,
    pub high_price_limit: Decimal,
    pub low_price_limit: Decimal,
}

#[derive(Clone, Debug)]
pub struct ExchangeOrderAck {
    pub order_id: String,
    pub client_order_id: String,
    pub symbol: String,
    pub status: String,
    pub executed_qty: Decimal,
    pub avg_price: Decimal,
    pub price: Decimal,
    pub quantity: Decimal,
    pub fee: Decimal,
    pub create_time: Option<DateTime<Utc>>,
    pub update_time: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct ExerciseRecord {
    pub symbol: String,
    pub settlement_price: Decimal,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyPreview {
    pub candidate: StrategyCandidate,
    #[serde(with = "rust_decimal::serde::str")]
    pub max_profit: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub reserved_asset: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub protected_limit_price: Decimal,
    pub summary: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub settings: AppSettings,
    pub metrics: Vec<MetricCard>,
    pub balances: Vec<BalanceRow>,
    pub candidates: Vec<StrategyCandidate>,
    pub scan_diagnostics: CandidateScanDiagnostics,
    pub orders: Vec<StrategyOrder>,
    pub pnl: PnlReport,
    pub risk: RiskSnapshot,
    pub auto_rule: AutoInvestmentRule,
    pub audit_recent: Vec<AuditEvent>,
    pub order_attempts: Vec<OrderAttemptView>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectBinanceRequest {
    pub api_key: String,
    pub api_secret: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyRequest {
    pub candidate_id: Option<String>,
    pub symbol: String,
    pub option_symbol: Option<String>,
    pub direction: StrategyDirection,
    pub expiry: NaiveDate,
    #[serde(with = "rust_decimal::serde::str")]
    pub strike: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub limit_price: Decimal,
    pub confirm_live: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleAutoRequest {
    pub enabled: bool,
    pub confirm_live: bool,
}
