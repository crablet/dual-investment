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
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub connected: bool,
    pub api_key_masked: Option<String>,
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
    pub recommendation: RecommendationLevel,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyOrder {
    pub id: String,
    pub exchange_order_id: Option<String>,
    pub symbol: String,
    pub option_symbol: String,
    pub direction: StrategyDirection,
    pub status: StrategyStatus,
    pub expiry: NaiveDate,
    #[serde(with = "rust_decimal::serde::str")]
    pub strike: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub premium: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub estimated_pnl: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub breakeven: Decimal,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub settlement_action: String,
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
    pub orders: Vec<StrategyOrder>,
    pub pnl: PnlReport,
    pub risk: RiskSnapshot,
    pub auto_rule: AutoInvestmentRule,
    pub audit_recent: Vec<AuditEvent>,
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
