//! 双币赢客户端的领域模型与 IPC 数据契约。
//!
//! 这个模块集中定义 Rust 内部策略模型、Binance 响应归一化后的结构，以及
//! 通过 Tauri IPC 暴露给前端的 DTO。所有金额、价格、数量、收益率都使用
//! [`Decimal`]，序列化给前端时统一走字符串，避免 JavaScript `number`
//! 在金融数据上产生隐式精度损失。

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// 应用内部错误类型。
///
/// Tauri command 最终会把这些错误转成 `String` 返回前端；在 Rust 内部保留
/// 分类是为了区分配置错误、风控拒绝、交易所错误和本地持久化错误。
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

/// Rust 内部统一结果类型。
pub type AppResult<T> = Result<T, AppError>;

/// 双币赢策略方向。
///
/// 当前 v1 只做卖出期权：卖 Put 用 USDT 承接低买 BTC，卖 Call 用 BTC 承接高卖。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StrategyDirection {
    SellPut,
    SellCall,
}

impl StrategyDirection {
    /// 返回 Binance 合约中的期权方向字符串。
    pub const fn option_type(self) -> &'static str {
        match self {
            Self::SellPut => "PUT",
            Self::SellCall => "CALL",
        }
    }

    /// 返回面向 UI 和审计日志的中文策略名称。
    pub const fn display(self) -> &'static str {
        match self {
            Self::SellPut => "卖 Put 抄底",
            Self::SellCall => "卖 Call 止盈",
        }
    }
}

/// 本地策略订单状态。
///
/// 状态既包含 Binance 期权订单状态的归一化，也包含到期补齐失败这类本地风控状态。
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

/// 候选机会的推荐等级。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RecommendationLevel {
    Recommended,
    Watch,
    Reject,
}

/// 风险提醒严重程度。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RiskSeverity {
    Low,
    Medium,
    High,
}

/// 前端设置页和导航状态需要展示的运行配置。
///
/// 这里不包含明文 Secret；`api_key_masked` 只用于显示脱敏连接状态。
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

/// 应用日志等级。
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

/// 文件日志中的单条记录。
///
/// 日志记录用于本机排障和设置页展示，内容必须保持脱敏，不能包含 Secret、签名 query
/// 或 Binance 原始响应。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub event: String,
    pub message: String,
}

/// 一个每日日志文件的展示信息。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFileInfo {
    pub file_name: String,
    pub path: String,
    pub date: String,
    pub size_bytes: u64,
    pub entry_count: usize,
    pub modified_at: Option<DateTime<Utc>>,
}

/// 设置页日志卡片需要的日志快照。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSnapshot {
    pub directory: String,
    pub current_file: String,
    pub files: Vec<LogFileInfo>,
    pub recent_entries: Vec<LogEntry>,
}

/// 账户资产行。
///
/// `free`、`locked` 和 `usdt_value` 以字符串序列化给前端，前端只负责展示或
/// 派生 UI 指标，不反向作为真实资金事实。
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
    pub usdt_value_known: bool,
}

/// 一条可浏览、可下单的策略候选。
///
/// 候选来自 Binance 实盘合约与 mark price，再结合当前自动规则计算推荐等级。
/// 浏览范围和可见性在策略层决定，前端筛选只影响展示。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyCandidate {
    pub id: String,
    pub symbol: String,
    pub option_symbol: String,
    pub direction: StrategyDirection,
    pub expiry: NaiveDate,
    pub expiry_at: DateTime<Utc>,
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

/// 候选扫描漏斗诊断。
///
/// 前端用这些计数解释为什么没有候选，避免把过滤过程隐藏成一个空列表。
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

/// 候选扫描结果，包含候选列表和过滤漏斗。
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateScanResult {
    pub candidates: Vec<StrategyCandidate>,
    pub diagnostics: CandidateScanDiagnostics,
}

/// 本地持久化的真实策略订单。
///
/// 订单必须来自 Binance 接单结果或恢复查询结果，不能由前端或本地推断凭空创建。
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
    pub expiry_at: DateTime<Utc>,
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

/// 自动复投规则。
///
/// `enabled` 会在应用启动时强制重置为 false；保存规则不等于开启自动下单。
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

/// 风控规则集合。
///
/// 规则值使用 Decimal 字符串跨 IPC，Rust 端负责真实风控拦截。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskRuleSet {
    #[serde(with = "rust_decimal::serde::str")]
    pub max_daily_orders: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub max_margin_usage: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub max_drawdown: Decimal,
    pub stop_after_consecutive_losses: u32,
}

/// 风控提醒。
///
/// `message` 应该是可安全展示的中文摘要，不应包含签名请求、Secret 或订单响应原文。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskAlert {
    pub id: String,
    pub severity: RiskSeverity,
    pub title: String,
    pub message: String,
    pub created_at: DateTime<Utc>,
}

/// 当前风险快照。
///
/// 由账户保证金、义务仓和风控规则计算得出，用于 UI 展示和下单前风控判断。
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

/// 小型图表的数据点。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartPoint {
    pub label: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub value: Decimal,
}

/// 收益汇总。
///
/// 这里保留真实订单和现货补齐派生出的收益字段；展示用构成比例由前端从这些字段再派生。
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
}

/// 下单恢复记录的状态机。
///
/// 当交易所已经接单但本地订单保存失败时，`PersistFailed`/`SpotPersistFailed`
/// 会留下可恢复记录。恢复时必须重新查询 Binance 真实订单。
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
    /// 返回持久化到 SQLite 的稳定状态字符串。
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

/// 本地恢复记录。
///
/// 这个结构不走前端反序列化，先在 Rust 中保留强类型状态，再转换成前端视图结构。
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

/// 前端可展示的恢复记录。
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

/// 手动下单进度阶段。
///
/// 这些事件只用于前端展示当前流程，不作为交易事实来源。
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OrderProgressStage {
    Validate,
    Funding,
    Risk,
    Submit,
    Persist,
}

/// 手动下单进度状态。
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OrderProgressStatus {
    Pending,
    Running,
    Success,
    Error,
}

/// 手动下单进度事件。
///
/// `message` 只能是脱敏摘要，不应包含签名请求、Secret 或 Binance 原始响应。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderProgressEvent {
    pub flow_id: String,
    pub source: String,
    pub stage: OrderProgressStage,
    pub status: OrderProgressStatus,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub option_symbol: Option<String>,
    pub exchange_order_id: Option<String>,
}

/// Binance 期权账户快照。
///
/// 原始响应会在 `binance.rs` 被解析成该结构，其他模块不直接依赖交易所 JSON 字段。
#[derive(Clone, Debug)]
pub struct OptionAccountSnapshot {
    pub assets: Vec<OptionAssetBalance>,
    pub can_trade: bool,
}

impl OptionAccountSnapshot {
    /// 按资产代码查找保证金资产行。
    pub fn asset(&self, asset: &str) -> Option<&OptionAssetBalance> {
        self.assets.iter().find(|row| row.asset == asset)
    }
}

/// Binance 现货账户快照，用于到期 ITM 后的现货补齐检查。
#[derive(Clone, Debug)]
pub struct SpotAccountSnapshot {
    pub balances: Vec<SpotAssetBalance>,
    pub can_trade: bool,
}

impl SpotAccountSnapshot {
    /// 按资产代码查找现货资产行。
    pub fn asset(&self, asset: &str) -> Option<&SpotAssetBalance> {
        self.balances.iter().find(|row| row.asset == asset)
    }
}

/// 现货资产余额。
#[derive(Clone, Debug)]
pub struct SpotAssetBalance {
    pub asset: String,
    pub free: Decimal,
    pub locked: Decimal,
}

/// Binance Simple Earn 持仓快照。
#[derive(Clone, Debug)]
pub struct SimpleEarnSnapshot {
    pub balances: Vec<SimpleEarnAssetBalance>,
}

/// Binance Simple Earn 单项持仓。
#[derive(Clone, Debug)]
pub struct SimpleEarnAssetBalance {
    pub asset: String,
    pub source: String,
    pub product_id: String,
    pub amount: Decimal,
    pub flexible: bool,
    pub can_redeem: bool,
}

/// Binance Simple Earn 活期产品元数据。
#[derive(Clone, Debug)]
pub struct SimpleEarnFlexibleProduct {
    pub asset: String,
    pub product_id: String,
    pub can_purchase: bool,
    pub is_sold_out: bool,
    pub status: String,
    pub min_purchase_amount: Decimal,
}

/// 期权保证金资产余额。
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

/// Binance 期权合约元数据。
///
/// 精度、最小数量和保证金参数会参与候选过滤和下单前风控。
#[derive(Clone, Debug)]
pub struct OptionContract {
    pub symbol: String,
    pub side: String,
    pub strike_price: Decimal,
    pub underlying: String,
    pub unit: Decimal,
    pub min_qty: Decimal,
    pub max_qty: Decimal,
    pub quantity_step_size: Decimal,
    pub initial_margin: Decimal,
    pub maintenance_margin: Decimal,
    pub min_initial_margin: Decimal,
    pub min_maintenance_margin: Decimal,
    pub min_price: Decimal,
    pub max_price: Decimal,
    pub price_tick_size: Decimal,
    pub price_scale: u32,
    pub quantity_scale: u32,
    pub quote_asset: String,
    pub status: String,
    pub expiry: NaiveDate,
    pub expiry_at: DateTime<Utc>,
}

/// Binance mark price 行情。
///
/// mark price、Delta 和 IV 只作为候选估算输入，真实成交价格以订单回报为准。
#[derive(Clone, Debug)]
pub struct OptionMarkPrice {
    pub symbol: String,
    pub mark_price: Decimal,
    pub mark_iv: Decimal,
    pub delta: Decimal,
    pub high_price_limit: Decimal,
    pub low_price_limit: Decimal,
}

/// 交易所订单查询或下单响应的归一化结构。
///
/// 期权和现货接口字段不完全一致，`binance.rs` 会在解析时补齐均价、手续费等公共字段。
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

/// 期权行权记录中的结算价格。
#[derive(Clone, Debug)]
pub struct ExerciseRecord {
    pub symbol: String,
    pub settlement_price: Decimal,
}

/// 策略预览结果。
///
/// 预览只用于展示和本地订单字段计算，不能替代下单前的合约校验和风控校验。
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

/// 前端一次刷新所需的完整应用快照。
///
/// 通过一个快照减少细粒度 IPC 往返；前端再从快照派生指标、日历和列表状态。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub settings: AppSettings,
    pub balances: Vec<BalanceRow>,
    pub candidates: Vec<StrategyCandidate>,
    pub scan_diagnostics: CandidateScanDiagnostics,
    pub orders: Vec<StrategyOrder>,
    pub pnl: PnlReport,
    pub risk: RiskSnapshot,
    pub auto_rule: AutoInvestmentRule,
    pub order_attempts: Vec<OrderAttemptView>,
}

/// 连接 Binance 的请求。
///
/// Secret 只会在 Rust 端写入系统 keyring，不会进入 SQLite、审计日志或前端状态。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectBinanceRequest {
    pub api_key: String,
    pub api_secret: String,
}

/// 手动或自动下单请求。
///
/// `confirm_live` 是真实下单门禁；手动下单必须由前端显式确认后才传 true。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyRequest {
    pub candidate_id: Option<String>,
    pub client_flow_id: Option<String>,
    pub symbol: String,
    pub option_symbol: Option<String>,
    pub direction: StrategyDirection,
    pub expiry: NaiveDate,
    pub expiry_at: DateTime<Utc>,
    #[serde(with = "rust_decimal::serde::str")]
    pub strike: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub limit_price: Decimal,
    pub confirm_live: bool,
}

/// 开关自动复投的请求。
///
/// 自动策略开启后，符合规则的自动订单不会逐笔确认，因此开启动作必须二次确认。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleAutoRequest {
    pub enabled: bool,
    pub confirm_live: bool,
}
