//! 双币赢客户端的领域模型与 IPC 数据契约。
//!
//! 这个模块集中定义 Rust 内部策略模型、Binance 响应归一化后的结构，以及
//! 通过 Tauri IPC 暴露给前端的 DTO。所有金额、价格、数量、收益率都使用
//! [`Decimal`]，序列化给前端时统一走字符串，避免 JavaScript `number`
//! 在金融数据上产生隐式精度损失。

use crate::messages::Message;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub use crate::errors::{AppError, AppResult};

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

    /// 返回面向 UI 和审计日志的策略名称消息。
    pub fn display(self) -> Message {
        match self {
            Self::SellPut => crate::msg!("backend.sellPut"),
            Self::SellCall => crate::msg!("backend.sellCall"),
        }
    }
}

/// 本机期权订单的交易所状态，不代表当前持仓或交割进度。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StrategyStatus {
    Pending,
    Filled,
    Cancelled,
    Rejected,
    Expired,
    PartiallyFilled,
}

/// 前端设置页和导航状态需要展示的运行配置，不包含 API Key 或 Secret。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub connected: bool,
    pub saved_credentials: bool,
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub event: String,
    pub message: Message,
}

/// 设置页日志面板需要的日志快照。
///
/// 快照包含最近脱敏日志，以及是否存在可清理日志文件的最小状态。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSnapshot {
    pub has_files: bool,
    pub recent_entries: Vec<LogEntry>,
    pub write_error: Option<Message>,
}

/// 账户资产行。
///
/// 数值以字符串序列化给前端；跨账户金额与数量在 Rust 聚合，前端只负责展示。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceRow {
    pub asset: String,
    pub source: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub free: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub locked: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub usdt_value: Decimal,
    pub usdt_value_known: bool,
}

/// 同名资产的 Decimal 汇总，明细保留原账户来源和估值口径。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetBalance {
    pub asset: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub free: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub locked: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub usdt_value: Decimal,
    pub usdt_value_known: bool,
    pub details: Vec<BalanceRow>,
}

/// 账户展示缓存。刷新失败时保留最后一次成功结果，并独立返回错误。
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSnapshot {
    pub balances: Vec<AssetBalance>,
    #[serde(with = "rust_decimal::serde::str")]
    pub total_usdt_value: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub available_margin: Decimal,
    pub unpriced_asset_count: usize,
    pub updated_at: Option<DateTime<Utc>>,
    pub error: Option<Message>,
}

/// 已成交且仍在运行的期权义务。名义本金不代表 Binance 实际保证金占用。
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioSummary {
    pub put_count: usize,
    pub call_count: usize,
    #[serde(with = "rust_decimal::serde::str")]
    pub put_quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub put_notional: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub call_quantity: Decimal,
}

/// 一条可浏览、可下单的策略候选。
///
/// 候选来自 Binance 实盘合约、买一价与 mark greeks，再结合当前浏览规则过滤。
/// 浏览范围和可见性在策略层决定，前端筛选只影响展示。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyCandidate {
    pub symbol: String,
    pub option_symbol: String,
    pub direction: StrategyDirection,
    pub expiry_at: DateTime<Utc>,
    #[serde(with = "rust_decimal::serde::str")]
    pub strike: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub limit_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub annualized: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub exercise_probability: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub spot_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub strike_distance_percent: Decimal,
}

/// 本机提交的期权订单记录；成交历史不作为持仓或交割数量。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StrategyOrder {
    pub id: String,
    pub exchange_order_id: Option<String>,
    pub client_order_id: String,
    pub symbol: String,
    pub option_symbol: String,
    pub direction: StrategyDirection,
    pub status: StrategyStatus,
    pub expiry_at: DateTime<Utc>,
    #[serde(with = "rust_decimal::serde::str")]
    pub strike: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub unit: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub filled_quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub avg_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub premium: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub annualized: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub breakeven: Decimal,
    pub exchange_status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PositionStatus {
    Open,
    Closed,
    Expired,
}

/// 一份数据库已绑定一个账户；完整合约代码唯一标识该账户的卖方净仓。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrackedPosition {
    pub option_symbol: String,
    pub symbol: String,
    pub direction: StrategyDirection,
    pub status: PositionStatus,
    pub expiry_at: DateTime<Utc>,
    #[serde(with = "rust_decimal::serde::str")]
    pub strike: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub unit: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub entry_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub mark_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub unrealized_pnl: Decimal,
    pub tracked_at: DateTime<Utc>,
    pub last_synced_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionSyncStatus {
    pub updated_at: Option<DateTime<Utc>>,
    pub error: Option<Message>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeliveryStatus {
    WaitingExercise,
    Ready,
    Executing,
    Completed,
    Failed,
}

/// 仅从到期前已登记且未平仓的净仓创建。成交任务不随当前持仓消失而删除。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeliveryTask {
    pub id: String,
    pub symbol: String,
    pub option_symbol: String,
    pub direction: StrategyDirection,
    pub status: DeliveryStatus,
    pub expiry_at: DateTime<Utc>,
    pub tracked_at: DateTime<Utc>,
    #[serde(with = "rust_decimal::serde::str")]
    pub strike: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub unit: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub last_position_quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub quantity: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub settlement_price: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub cash_settlement: Option<Decimal>,
    pub exercise_record_ids: Vec<String>,
    pub spot_order_id: Option<String>,
    pub spot_client_order_id: String,
    pub spot_order_status: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub spot_filled_quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub spot_avg_price: Decimal,
    pub message: Message,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl DeliveryTask {
    pub fn base_quantity(&self) -> AppResult<Decimal> {
        self.quantity
            .map(|quantity| quantity * self.unit)
            .ok_or_else(|| AppError::Storage(crate::msg!("backend.deliveryFactsNotConfirmed")))
    }

    pub fn exercise_price(&self) -> AppResult<Decimal> {
        self.settlement_price
            .ok_or_else(|| AppError::Storage(crate::msg!("backend.deliveryFactsNotConfirmed")))
    }
}

/// 本机卖出权利金汇总，不代表完整账户累计收益。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_field_names, reason = "IPC 字段明确标注权利金口径")]
pub struct PnlReport {
    #[serde(with = "rust_decimal::serde::str")]
    pub recorded_premium: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub today_recorded_premium: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub sell_put_premium: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub sell_call_premium: Decimal,
}

/// 下单恢复记录的状态机。
///
/// 当交易所已经接单但本地订单保存失败时，恢复记录会保留待确认状态。
/// 恢复时必须重新查询 Binance 真实订单。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderAttemptStatus {
    Attempted,
    ExchangeAck,
    Persisted,
    Failed,
    PersistFailed,
    SpotAttempted,
    SpotPersisted,
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
            Self::SpotPersisted => "spot_persisted",
        }
    }
}

/// 本地恢复记录。
///
/// 只包含可展示的恢复事实，不接收前端反序列化，也不复制到另一套同形 DTO。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderAttempt {
    pub id: String,
    pub client_order_id: String,
    pub symbol: String,
    pub option_symbol: String,
    pub status: OrderAttemptStatus,
    pub exchange_order_id: Option<String>,
    pub message: Message,
    pub updated_at: DateTime<Utc>,
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

    /// 返回前端展示用的类型名称消息。
    pub fn display(self) -> Message {
        match self {
            Self::UniversalTransfer => crate::msg!("backend.accountTransfer"),
            Self::SimpleEarnRedeem => crate::msg!("backend.flexibleRedemption"),
            Self::SimpleEarnSubscribe => crate::msg!("backend.flexibleSubscription"),
        }
    }
}

/// 真实资金动作恢复状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
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
    pub fn display(self) -> Message {
        match self {
            Self::Confirmed => crate::msg!("backend.confirmed"),
            Self::Failed => crate::msg!("backend.failed2"),
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
    pub purpose: Message,
    pub asset: String,
    pub amount: Decimal,
    pub transfer_type: Option<String>,
    pub product_id: Option<String>,
    pub exchange_ref: Option<String>,
    pub status: FundingAttemptStatus,
    pub message: Message,
    pub query_after_at: Option<DateTime<Utc>>,
    pub query_count: u32,
    pub effective_query_count: u32,
    pub transport_error_count: u32,
    pub submit_error_kind: Option<String>,
    pub history_error_kind: Option<String>,
    pub history_window_start_at: Option<DateTime<Utc>>,
    pub history_window_end_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 批次阶段只描述编排进度；成交与资金成功仍由 Binance 记录确认。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SettlementStage {
    WaitingExercise,
    WaitingFunds,
    ConfirmingFunds,
    Executing,
    ConfirmingOrders,
    PendingSweep,
    Blocked,
    Completed,
    SweepSkipped,
}

impl SettlementStage {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::SweepSkipped)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementAssets {
    #[serde(with = "rust_decimal::serde::str")]
    pub usdt: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub btc: Decimal,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementRedemption {
    pub chain_id: String,
    pub asset: String,
    pub product_id: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub expected_spot: Decimal,
}

/// 接货前划回期权余款的固定计划，与接货后的归集计划分别确认。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementOptionFunding {
    pub chain_id: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub expected_spot: Decimal,
}

/// 同一精确到期时间的持久化结算批次，成员是交割任务。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementBatch {
    pub id: String,
    pub symbol: String,
    pub expiry_at: DateTime<Utc>,
    pub task_ids: Vec<String>,
    pub stage: SettlementStage,
    pub message: Message,
    pub required: SettlementAssets,
    pub shortfall: SettlementAssets,
    pub redemption: Option<SettlementRedemption>,
    pub option_funding: Option<SettlementOptionFunding>,
    pub sweep_after_at: Option<DateTime<Utc>>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub option_sweep_usdt: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str")]
    pub spot_usdt_before_transfer: Decimal,
    pub spot_sweep: Option<SettlementAssets>,
    pub updated_at: DateTime<Utc>,
}

/// 恢复中心中的批次记录，操作资格由后端判断。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementRecoveryView {
    pub id: String,
    pub symbol: String,
    pub expiry_at: DateTime<Utc>,
    pub status: SettlementStage,
    pub status_label: Message,
    pub can_finish: bool,
    pub fact_source: Message,
    pub next_action: Message,
    pub message: Message,
    pub updated_at: DateTime<Utc>,
}

/// 用户选择结束理财收尾，保留原有资金结果。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FinishSettlementSweepRequest {
    pub batch_id: String,
    pub confirm_manual: bool,
}

/// 人工解除资金恢复阻断的请求。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveFundingAttemptRequest {
    pub attempt_id: String,
    pub resolution: FundingAttemptManualResolution,
    pub confirm_manual: bool,
}

/// 前端可展示的资金动作恢复记录。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FundingAttemptView {
    pub id: String,
    pub kind_label: Message,
    pub asset: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
    pub transfer_type: Option<String>,
    pub product_id: Option<String>,
    pub exchange_ref: Option<String>,
    pub status: FundingAttemptStatus,
    pub message: Message,
    pub fact_source: Message,
    pub next_action: Message,
    pub query_after_at: Option<DateTime<Utc>>,
    pub query_count: u32,
    pub effective_query_count: u32,
    pub transport_error_count: u32,
    pub submit_error_kind: Option<String>,
    pub history_error_kind: Option<String>,
    pub history_window_start_at: Option<DateTime<Utc>>,
    pub history_window_end_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl From<FundingAttempt> for FundingAttemptView {
    fn from(attempt: FundingAttempt) -> Self {
        let fact_source = match (&attempt.exchange_ref, attempt.status) {
            (Some(exchange_ref), FundingAttemptStatus::Confirmed) => {
                crate::msg!("backend.binanceHistoryRecord", p0 = exchange_ref)
            }
            (
                Some(exchange_ref),
                FundingAttemptStatus::ExchangeAck | FundingAttemptStatus::Pending,
            ) => {
                crate::msg!("backend.binanceReturnedReference", p0 = exchange_ref)
            }
            (Some(exchange_ref), _) => crate::msg!("backend.binanceReference", p0 = exchange_ref),
            (None, FundingAttemptStatus::Confirmed) => {
                crate::msg!("backend.confirmedByManualReviewOfBinanceFacts")
            }
            (None, FundingAttemptStatus::Failed) => {
                crate::msg!("backend.explicitBinanceRejectionOrFailedManualReview")
            }
            (None, _) => crate::msg!("backend.awaitingConfirmationFromBinanceHistoryAndAccount"),
        };
        let next_action = match attempt.status {
            FundingAttemptStatus::Attempted
            | FundingAttemptStatus::ExchangeAck
            | FundingAttemptStatus::Uncertain
            | FundingAttemptStatus::Pending => {
                crate::msg!("backend.automaticallyQueryBinanceHistoryForConfirmation")
            }
            FundingAttemptStatus::Confirmed => crate::msg!("backend.noActionNeeded"),
            FundingAttemptStatus::Failed => {
                crate::msg!("backend.failedCheckTheCauseBeforeStartingThe")
            }
            FundingAttemptStatus::Blocked => {
                crate::msg!("backend.manualReviewOfBinanceFactsRequired")
            }
        };
        Self {
            id: attempt.id,
            kind_label: attempt.kind.display(),
            asset: attempt.asset,
            amount: attempt.amount,
            transfer_type: attempt.transfer_type,
            product_id: attempt.product_id,
            exchange_ref: attempt.exchange_ref,
            status: attempt.status,
            message: attempt.message,
            fact_source,
            next_action,
            query_after_at: attempt.query_after_at,
            query_count: attempt.query_count,
            effective_query_count: attempt.effective_query_count,
            transport_error_count: attempt.transport_error_count,
            submit_error_kind: attempt.submit_error_kind,
            history_error_kind: attempt.history_error_kind,
            history_window_start_at: attempt.history_window_start_at,
            history_window_end_at: attempt.history_window_end_at,
            updated_at: attempt.updated_at,
        }
    }
}

/// 手动下单进度阶段。
///
/// 这些事件只用于前端展示当前流程，不作为交易事实来源。
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OrderProgressStage {
    Validate,
    Funding,
    Risk,
    Submit,
    Persist,
}

/// 手动下单进度状态。
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OrderProgressStatus {
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
    pub stage: OrderProgressStage,
    pub status: OrderProgressStatus,
    pub message: Message,
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
    pub uid: String,
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
    pub expiry_at: DateTime<Utc>,
}

/// Binance mark price 行情。
///
/// mark price 用于价格上下限校验，Delta 用于估算候选被行权概率。
/// 下单限价必须来自可主动成交的盘口买盘，真实成交价格以订单回报为准。
#[derive(Clone, Debug)]
pub struct OptionMarkPrice {
    pub symbol: String,
    pub mark_price: Decimal,
    pub delta: Decimal,
    pub high_price_limit: Decimal,
    pub low_price_limit: Decimal,
}

/// Binance 期权 24h ticker 中的当前盘口摘要。
///
/// 候选扫描用这里的买一价估算可主动卖出的权利金；发送真实订单前还会重新读取订单簿。
#[derive(Clone, Debug)]
pub struct OptionTicker {
    pub symbol: String,
    pub bid_price: Decimal,
}

/// Binance 期权订单簿单档价格和数量。
#[derive(Clone, Debug)]
pub struct OrderBookLevel {
    pub price: Decimal,
    pub quantity: Decimal,
}

/// Binance 期权订单簿。
///
/// 真实卖出 IOC 下单前使用当前买一价和买一量计算可主动成交的限价，不复用候选扫描快照。
#[derive(Clone, Debug)]
pub struct OptionOrderBook {
    pub bids: Vec<OrderBookLevel>,
}

/// 交易所订单查询或下单响应的归一化结构。
///
/// 期权和现货接口字段不完全一致，`binance.rs` 负责归一化成交字段。
/// 已成交回报必须提供有效均价或可推算均价的累计成交额，不用委托价格代替。
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
    pub create_time: Option<DateTime<Utc>>,
    pub update_time: Option<DateTime<Utc>>,
}

/// Binance 当前持仓数量保留符号：负数为卖方，正数为买方。
#[derive(Clone, Debug)]
pub struct OptionPosition {
    pub symbol: String,
    pub quantity: Decimal,
    pub entry_price: Decimal,
    pub mark_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub strike: Decimal,
    pub expiry_at: DateTime<Utc>,
    pub option_side: String,
    pub quote_asset: String,
}

/// 账户行权事实；只读取交割与收益所需字段，不解析手续费。
#[derive(Clone, Debug)]
pub struct ExerciseRecord {
    pub id: String,
    pub symbol: String,
    pub settlement_price: Decimal,
    pub quantity: Decimal,
    pub amount: Decimal,
    pub created_at: DateTime<Utc>,
    pub option_side: String,
    pub position_side: String,
}

/// 同标的、同到期时刻共享的公开结算价；不提供任何账户结算数量。
#[derive(Clone, Debug)]
pub struct ExercisePrice {
    pub underlying: String,
    pub settlement_price: Decimal,
    pub expiry_at: DateTime<Utc>,
}

/// 本地订单和恢复事实，加上最近一次成功刷新的账户展示缓存。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub settings: AppSettings,
    pub account: AccountSnapshot,
    pub candidates: Vec<StrategyCandidate>,
    pub orders: Vec<StrategyOrder>,
    pub positions: Vec<TrackedPosition>,
    pub deliveries: Vec<DeliveryTask>,
    pub position_sync: PositionSyncStatus,
    pub pnl: PnlReport,
    pub portfolio: PortfolioSummary,
    pub order_attempts: Vec<OrderAttempt>,
    pub funding_attempts: Vec<FundingAttemptView>,
    pub settlement_recoveries: Vec<SettlementRecoveryView>,
}

/// 连接 Binance 的请求。
///
/// 用户输入的 Secret 仅由 Rust 凭据模块写入系统 keyring，
/// 不进入 SQLite、日志或返回前端的快照。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectBinanceRequest {
    pub api_key: String,
    pub api_secret: String,
}

/// 手动下单或订单恢复使用的请求。
///
/// `confirm_live` 是真实下单门禁；手动下单必须由前端显式确认后才传 true。
/// 手动请求携带冻结的 option symbol、精确到期时间和数量等输入，不能缺失后再模糊匹配。
/// 请求不携带候选 ID 或预览年化；订单年化必须由 Binance 回报生成。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StrategyRequest {
    pub client_flow_id: String,
    pub symbol: String,
    pub option_symbol: String,
    pub direction: StrategyDirection,
    pub expiry_at: DateTime<Utc>,
    #[serde(with = "rust_decimal::serde::str")]
    pub strike: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub limit_price: Decimal,
    pub confirm_live: bool,
}

/// 纯计算预览输入，不包含账户、合约查询或真实下单授权。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StrategyPreviewRequest {
    pub direction: StrategyDirection,
    #[serde(with = "rust_decimal::serde::str")]
    pub strike: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub limit_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyPreview {
    #[serde(with = "rust_decimal::serde::str")]
    pub premium: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub breakeven: Decimal,
}
