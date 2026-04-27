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

/// 后台运行健康状态。
///
/// 这个状态只用于前端观测长期运行质量，不作为订单、结算或资金事实来源。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeHealthStatus {
    Idle,
    Healthy,
    Degraded,
    Attention,
}

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

/// 本地策略订单来源。
///
/// 接管持仓没有原始订单号，必须显式标记来源，避免启动加载和同步逻辑继续假设
/// 所有可跟踪订单都来自本机下单响应。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StrategyOrderSource {
    LiveOrder,
    AdoptedPosition,
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
    pub source: StrategyOrderSource,
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
    pub annualized: Decimal,
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
    pub max_margin_usage: Decimal,
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
    pub rules: RiskRuleSet,
    pub alerts: Vec<RiskAlert>,
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
    pub floating_spot: Decimal,
}

/// 后台调度器和恢复记录的运行健康快照。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeHealth {
    pub status: RuntimeHealthStatus,
    pub last_tick_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub consecutive_failures: u32,
    pub skipped_ticks: u32,
    pub last_error: Option<String>,
    pub pending_recovery_count: usize,
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
    Failed,
    PersistFailed,
    SpotAttempted,
    SpotExchangeAck,
    SpotPersisted,
    SpotPersistFailed,
}

impl OrderAttemptStatus {
    /// 返回持久化到 `SQLite` 的稳定状态字符串。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Attempted => "attempted",
            Self::ExchangeAck => "exchange_ack",
            Self::Persisted => "persisted",
            Self::Failed => "failed",
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

/// 真实资金动作恢复记录类型。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FundingAttemptKind {
    UniversalTransfer,
    SimpleEarnRedeem,
    SimpleEarnSubscribe,
}

impl FundingAttemptKind {
    /// 返回持久化到 `SQLite` 的稳定类型字符串。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UniversalTransfer => "universal_transfer",
            Self::SimpleEarnRedeem => "simple_earn_redeem",
            Self::SimpleEarnSubscribe => "simple_earn_subscribe",
        }
    }

    /// 返回前端展示用的中文类型名称。
    pub const fn display(self) -> &'static str {
        match self {
            Self::UniversalTransfer => "账户划转",
            Self::SimpleEarnRedeem => "活期赎回",
            Self::SimpleEarnSubscribe => "活期申购",
        }
    }
}

/// 真实资金动作恢复状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FundingAttemptStatus {
    Attempted,
    ExchangeAck,
    Uncertain,
    Pending,
    Confirmed,
    Failed,
    Blocked,
}

impl FundingAttemptStatus {
    /// 返回持久化到 `SQLite` 的稳定状态字符串。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Attempted => "attempted",
            Self::ExchangeAck => "exchange_ack",
            Self::Uncertain => "uncertain",
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
        }
    }

    /// 已确认不会再自动推进的终态。
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Confirmed | Self::Failed)
    }
}

/// 人工核对资金恢复记录后的处置结果。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum FundingAttemptManualResolution {
    Confirmed,
    Failed,
}

impl FundingAttemptManualResolution {
    /// 返回用户可读的处置结果。
    pub const fn display(self) -> &'static str {
        match self {
            Self::Confirmed => "已确认",
            Self::Failed => "已失败",
        }
    }
}

/// 本地资金动作恢复记录。
///
/// 该记录只保存脱敏、可恢复的业务事实，不保存签名 query 或 Binance 原始响应。
#[derive(Clone, Debug)]
pub struct FundingAttempt {
    pub id: String,
    pub chain_id: String,
    pub kind: FundingAttemptKind,
    pub purpose: String,
    pub asset: String,
    pub amount: Decimal,
    pub transfer_type: Option<String>,
    pub product_id: Option<String>,
    pub exchange_ref: Option<String>,
    pub status: FundingAttemptStatus,
    pub message: String,
    pub query_after_at: Option<DateTime<Utc>>,
    pub send_count: u32,
    pub query_count: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 人工解除资金恢复阻断的请求。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveFundingAttemptRequest {
    pub attempt_id: String,
    pub resolution: FundingAttemptManualResolution,
    pub confirm_manual: bool,
}

/// 前端可展示的资金动作恢复记录。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FundingAttemptView {
    pub id: String,
    pub chain_id: String,
    pub kind: String,
    pub kind_label: String,
    pub purpose: String,
    pub asset: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
    pub transfer_type: Option<String>,
    pub product_id: Option<String>,
    pub exchange_ref: Option<String>,
    pub status: String,
    pub message: String,
    pub fact_source: String,
    pub next_action: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<FundingAttempt> for FundingAttemptView {
    fn from(attempt: FundingAttempt) -> Self {
        let fact_source = match (&attempt.exchange_ref, attempt.status) {
            (Some(exchange_ref), FundingAttemptStatus::Confirmed) => {
                format!("Binance 历史记录 {exchange_ref}")
            }
            (
                Some(exchange_ref),
                FundingAttemptStatus::ExchangeAck | FundingAttemptStatus::Pending,
            ) => {
                format!("Binance 已返回引用 {exchange_ref}")
            }
            (Some(exchange_ref), _) => format!("Binance 引用 {exchange_ref}"),
            (None, FundingAttemptStatus::Attempted) if attempt.send_count == 0 => {
                "本地预写记录，尚未开始发送".to_string()
            }
            (None, FundingAttemptStatus::Confirmed) => "人工核对 Binance 事实已确认".to_string(),
            (None, FundingAttemptStatus::Failed) => "Binance 明确拒绝或人工核对失败".to_string(),
            (None, _) => "按 Binance 历史与账户事实待确认".to_string(),
        };
        let next_action = match attempt.status {
            FundingAttemptStatus::Attempted if attempt.send_count == 0 => {
                "自动发送首次请求".to_string()
            }
            FundingAttemptStatus::Attempted
            | FundingAttemptStatus::ExchangeAck
            | FundingAttemptStatus::Uncertain
            | FundingAttemptStatus::Pending => "自动查询 Binance 历史确认".to_string(),
            FundingAttemptStatus::Confirmed => "无需处理".to_string(),
            FundingAttemptStatus::Failed => "已失败，检查原因后重新触发业务流程".to_string(),
            FundingAttemptStatus::Blocked => "需要人工核对 Binance 事实".to_string(),
        };
        Self {
            id: attempt.id,
            chain_id: attempt.chain_id,
            kind: attempt.kind.as_str().to_string(),
            kind_label: attempt.kind.display().to_string(),
            purpose: attempt.purpose,
            asset: attempt.asset,
            amount: attempt.amount,
            transfer_type: attempt.transfer_type,
            product_id: attempt.product_id,
            exchange_ref: attempt.exchange_ref,
            status: attempt.status.as_str().to_string(),
            message: attempt.message,
            fact_source,
            next_action,
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

/// Binance Universal Transfer 历史记录。
#[derive(Clone, Debug)]
pub struct UniversalTransferRecord {
    pub transfer_type: String,
    pub asset: String,
    pub amount: Decimal,
    pub status: String,
    pub tran_id: String,
}

/// Binance Simple Earn 活期申购历史记录。
#[derive(Clone, Debug)]
pub struct SimpleEarnSubscriptionRecord {
    pub product_id: String,
    pub asset: String,
    pub amount: Decimal,
    pub status: String,
    pub purchase_id: String,
}

/// Binance Simple Earn 活期赎回历史记录。
#[derive(Clone, Debug)]
pub struct SimpleEarnRedemptionRecord {
    pub product_id: String,
    pub asset: String,
    pub amount: Decimal,
    pub status: String,
    pub redeem_id: String,
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

/// Binance 期权当前持仓。
///
/// 接管外部订单时以当前持仓为事实来源；它不一定能反查到原始下单订单号。
#[derive(Clone, Debug)]
pub struct OptionPosition {
    pub symbol: String,
    pub side: String,
    pub quantity: Decimal,
    pub entry_price: Decimal,
    pub mark_price: Decimal,
    pub update_time: Option<DateTime<Utc>>,
}

/// 可接管的 Binance 期权持仓视图。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptableOptionPositionView {
    pub id: String,
    pub symbol: String,
    pub direction: StrategyDirection,
    pub expiry: NaiveDate,
    pub expiry_at: DateTime<Utc>,
    #[serde(with = "rust_decimal::serde::str")]
    pub strike: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub entry_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub mark_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub premium: Decimal,
    pub already_tracked: bool,
    pub adoptable: bool,
    pub reason: String,
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
    pub health: RuntimeHealth,
    pub balances: Vec<BalanceRow>,
    pub candidates: Vec<StrategyCandidate>,
    pub scan_diagnostics: CandidateScanDiagnostics,
    pub orders: Vec<StrategyOrder>,
    pub pnl: PnlReport,
    pub risk: RiskSnapshot,
    pub auto_rule: AutoInvestmentRule,
    pub order_attempts: Vec<OrderAttemptView>,
    pub funding_attempts: Vec<FundingAttemptView>,
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

/// 接管 Binance 当前期权持仓的请求。
///
/// 接管不会立即下单，但后续到期 ITM 时可能触发现货补齐，因此仍要求显式确认。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdoptOptionPositionsRequest {
    pub symbols: Vec<String>,
    pub confirm_live: bool,
}
