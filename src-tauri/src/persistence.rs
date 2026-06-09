//! 本地 `SQLite` 持久化层。
//!
//! 这里只保存业务事件、订单快照和恢复记录；API Key/Secret 永远不进入
//! SQLite。订单和审计以追加/替换业务事实的方式保存，便于应用重启后从真实交易所状态恢复。

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, Row, params};
use rust_decimal::Decimal;
use std::str::FromStr;

use crate::domain::{
    AppError, AppResult, FundingAttempt, FundingAttemptKind, FundingAttemptStatus, OrderAttempt,
    OrderAttemptStatus, SettlementBatch, StrategyOrder,
};

pub struct Persistence {
    connection: Connection,
}

impl Persistence {
    /// 打开应用数据目录中的 `SQLite` 数据库。
    pub fn open_app_data(data_dir: impl AsRef<Path>) -> AppResult<Self> {
        std::fs::create_dir_all(data_dir.as_ref())
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let connection = Connection::open(data_dir.as_ref().join("dual-investment.db"))
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let store = Self { connection };
        store.init()?;
        Ok(store)
    }

    /// 初始化本地表结构。
    ///
    /// 只创建当前代码需要的表结构；开发期旧数据迁移或清理应使用一次性本地命令处理，
    /// 不把旧结构兼容或自动删库逻辑留在应用代码里。`order_attempts` 和
    /// `funding_attempts` 单独拆列，是因为恢复流程需要稳定按状态、订单号、资金动作引用
    /// 和查询时间推进。
    fn init(&self) -> AppResult<()> {
        self.connection
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS strategy_orders (
                    id TEXT PRIMARY KEY,
                    payload TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS order_attempts (
                    id TEXT PRIMARY KEY,
                    client_order_id TEXT NOT NULL,
                    symbol TEXT NOT NULL,
                    option_symbol TEXT NOT NULL,
                    status TEXT NOT NULL,
                    exchange_order_id TEXT,
                    message TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS funding_attempts (
                    id TEXT PRIMARY KEY,
                    chain_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    purpose TEXT NOT NULL,
                    asset TEXT NOT NULL,
                    amount TEXT NOT NULL,
                    transfer_type TEXT,
                    product_id TEXT,
                    exchange_ref TEXT,
                    status TEXT NOT NULL,
                    message TEXT NOT NULL,
                    query_after_at TEXT,
                    query_count INTEGER NOT NULL,
                    effective_query_count INTEGER NOT NULL,
                    transport_error_count INTEGER NOT NULL,
                    submit_error_kind TEXT,
                    history_error_kind TEXT,
                    history_window_start_at TEXT,
                    history_window_end_at TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS settlement_batches (
                    id TEXT PRIMARY KEY,
                    payload TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_strategy_orders_updated_at
                    ON strategy_orders (updated_at DESC);
                CREATE INDEX IF NOT EXISTS idx_order_attempts_updated_at
                    ON order_attempts (updated_at DESC);
                CREATE INDEX IF NOT EXISTS idx_funding_attempts_updated_at
                    ON funding_attempts (updated_at DESC);
                CREATE INDEX IF NOT EXISTS idx_funding_attempts_status
                    ON funding_attempts (status);
                CREATE INDEX IF NOT EXISTS idx_funding_attempts_chain_created
                    ON funding_attempts (chain_id, created_at);

                ",
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        Ok(())
    }

    /// 保存本地策略订单快照。
    ///
    /// 订单必须来自本机下单、恢复查询，或 Binance 当前持仓接管；不能由前端凭空创建。
    pub fn save_order(&self, order: &StrategyOrder) -> AppResult<()> {
        let payload =
            serde_json::to_string(order).map_err(|error| AppError::Storage(error.to_string()))?;
        self.connection
            .execute(
                "INSERT OR REPLACE INTO strategy_orders (id, payload, updated_at) VALUES (?1, ?2, ?3)",
                params![order.id, payload, order.updated_at.to_rfc3339()],
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        Ok(())
    }

    /// 保存下单恢复记录。
    ///
    /// 这条记录覆盖“交易所已接单但本地订单保存失败”的高风险窗口。
    pub fn save_order_attempt(&self, attempt: &OrderAttempt) -> AppResult<()> {
        self.connection
            .execute(
                "INSERT OR REPLACE INTO order_attempts (
                    id,
                    client_order_id,
                    symbol,
                    option_symbol,
                    status,
                    exchange_order_id,
                    message,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    attempt.id,
                    attempt.client_order_id,
                    attempt.symbol,
                    attempt.option_symbol,
                    attempt.status.as_str(),
                    attempt.exchange_order_id.as_deref(),
                    attempt.message,
                    attempt.updated_at.to_rfc3339(),
                ],
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        Ok(())
    }

    /// 保存真实资金动作恢复记录。
    pub fn save_funding_attempt(&self, attempt: &FundingAttempt) -> AppResult<()> {
        self.connection
            .execute(
                "INSERT OR REPLACE INTO funding_attempts (
                    id,
                    chain_id,
                    kind,
                    purpose,
                    asset,
                    amount,
                    transfer_type,
                    product_id,
                    exchange_ref,
                    status,
                    message,
                    query_after_at,
                    query_count,
                    effective_query_count,
                    transport_error_count,
                    submit_error_kind,
                    history_error_kind,
                    history_window_start_at,
                    history_window_end_at,
                    created_at,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
                params![
                    attempt.id,
                    attempt.chain_id,
                    attempt.kind.as_str(),
                    attempt.purpose,
                    attempt.asset,
                    attempt.amount.normalize().to_string(),
                    attempt.transfer_type.as_deref(),
                    attempt.product_id.as_deref(),
                    attempt.exchange_ref.as_deref(),
                    attempt.status.as_str(),
                    attempt.message,
                    attempt.query_after_at.as_ref().map(DateTime::to_rfc3339),
                    i64::from(attempt.query_count),
                    i64::from(attempt.effective_query_count),
                    i64::from(attempt.transport_error_count),
                    attempt.submit_error_kind.as_deref(),
                    attempt.history_error_kind.as_deref(),
                    attempt.history_window_start_at.as_ref().map(DateTime::to_rfc3339),
                    attempt.history_window_end_at.as_ref().map(DateTime::to_rfc3339),
                    attempt.created_at.to_rfc3339(),
                    attempt.updated_at.to_rfc3339(),
                ],
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        Ok(())
    }

    pub fn save_settlement_batch(&self, batch: &SettlementBatch) -> AppResult<()> {
        let payload =
            serde_json::to_string(batch).map_err(|error| AppError::Storage(error.to_string()))?;
        self.connection.execute(
            "INSERT INTO settlement_batches (id, payload, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET payload=excluded.payload, updated_at=excluded.updated_at",
            params![batch.id, payload, batch.updated_at.to_rfc3339()],
        ).map_err(|error| AppError::Storage(error.to_string()))?;
        Ok(())
    }

    /// 下单意图和策略关联必须一起落库，覆盖请求发送后进程退出的窗口。
    pub fn save_order_and_attempt(
        &self,
        order: &StrategyOrder,
        attempt: &OrderAttempt,
    ) -> AppResult<()> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| AppError::Storage(error.to_string()))?;
        self.save_order(order)?;
        self.save_order_attempt(attempt)?;
        transaction
            .commit()
            .map_err(|error| AppError::Storage(error.to_string()))
    }

    /// 最后一笔状态和收尾计划一起保存；已有批次记录也可从成交事实重建该步骤。
    pub fn save_batch_and_orders(
        &self,
        batch: &SettlementBatch,
        orders: &[StrategyOrder],
    ) -> AppResult<()> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| AppError::Storage(error.to_string()))?;
        for order in orders {
            self.save_order(order)?;
        }
        self.save_settlement_batch(batch)?;
        transaction
            .commit()
            .map_err(|error| AppError::Storage(error.to_string()))
    }

    /// 删除本地策略订单快照。
    pub fn delete_order(&self, order_id: &str) -> AppResult<()> {
        self.connection
            .execute(
                "DELETE FROM strategy_orders WHERE id = ?1",
                params![order_id],
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        Ok(())
    }

    /// 按更新时间倒序读取订单。
    pub fn list_orders(&self) -> AppResult<Vec<StrategyOrder>> {
        let mut statement = self
            .connection
            .prepare("SELECT payload FROM strategy_orders ORDER BY updated_at DESC")
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let mapped = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let mut orders = Vec::new();
        for row in mapped {
            let payload = row.map_err(|error| AppError::Storage(error.to_string()))?;
            let order = serde_json::from_str::<StrategyOrder>(&payload)
                .map_err(|error| AppError::Storage(error.to_string()))?;
            orders.push(order);
        }
        Ok(orders)
    }

    /// 按更新时间倒序读取恢复记录。
    pub fn list_order_attempts(&self, limit: usize) -> AppResult<Vec<OrderAttempt>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, client_order_id, symbol, option_symbol, status, exchange_order_id, message, updated_at
                 FROM order_attempts
                 ORDER BY updated_at DESC
                 LIMIT ?1",
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let mapped = statement
            .query_map([limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(|error| AppError::Storage(error.to_string()))?;

        let mut attempts = Vec::new();
        for row in mapped {
            let (
                id,
                client_order_id,
                symbol,
                option_symbol,
                status,
                exchange_order_id,
                message,
                updated_at,
            ) = row.map_err(|error| AppError::Storage(error.to_string()))?;
            attempts.push(OrderAttempt {
                id,
                client_order_id,
                symbol,
                option_symbol,
                status: parse_attempt_status(&status)?,
                exchange_order_id,
                message,
                updated_at: parse_time(&updated_at)?,
            });
        }
        Ok(attempts)
    }

    /// 按更新时间倒序读取资金动作恢复记录。
    pub fn list_funding_attempts(&self, limit: usize) -> AppResult<Vec<FundingAttempt>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, chain_id, kind, purpose, asset, amount, transfer_type, product_id,
                        exchange_ref, status, message, query_after_at, query_count,
                        effective_query_count, transport_error_count, submit_error_kind,
                        history_error_kind, history_window_start_at, history_window_end_at,
                        created_at, updated_at
                 FROM funding_attempts
                 ORDER BY updated_at DESC
                 LIMIT ?1",
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let mapped = statement
            .query_map([limit], FundingAttemptRow::read)
            .map_err(|error| AppError::Storage(error.to_string()))?;

        let mut attempts = Vec::new();
        for row in mapped {
            let row = row.map_err(|error| AppError::Storage(error.to_string()))?;
            attempts.push(row.into_attempt()?);
        }
        Ok(attempts)
    }

    /// 按资金链路读取恢复记录。
    pub fn list_funding_attempts_by_chain(&self, chain_id: &str) -> AppResult<Vec<FundingAttempt>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, chain_id, kind, purpose, asset, amount, transfer_type, product_id,
                        exchange_ref, status, message, query_after_at, query_count,
                        effective_query_count, transport_error_count, submit_error_kind,
                        history_error_kind, history_window_start_at, history_window_end_at,
                        created_at, updated_at
                 FROM funding_attempts
                 WHERE chain_id = ?1
                 ORDER BY created_at ASC",
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let mapped = statement
            .query_map([chain_id], FundingAttemptRow::read)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let mut attempts = Vec::new();
        for row in mapped {
            let row = row.map_err(|error| AppError::Storage(error.to_string()))?;
            attempts.push(row.into_attempt()?);
        }
        Ok(attempts)
    }

    pub fn list_settlement_batches(&self) -> AppResult<Vec<SettlementBatch>> {
        let mut statement = self
            .connection
            .prepare("SELECT payload FROM settlement_batches ORDER BY updated_at DESC")
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| AppError::Storage(error.to_string()))?;
        rows.map(|row| {
            let payload = row.map_err(|error| AppError::Storage(error.to_string()))?;
            serde_json::from_str(&payload).map_err(|error| AppError::Storage(error.to_string()))
        })
        .collect()
    }

    /// 统计仍需要人工确认或恢复的下单记录。
    pub fn count_pending_order_attempts(&self) -> AppResult<usize> {
        let count = self
            .connection
            .query_row(
                "SELECT COUNT(*)
                 FROM order_attempts
                 WHERE status NOT IN ('persisted', 'failed', 'spot_persisted')",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        usize::try_from(count).map_err(|error| AppError::Storage(error.to_string()))
    }

    /// 统计仍需要自动恢复或人工处理的资金动作记录。
    pub fn count_pending_funding_attempts(&self) -> AppResult<usize> {
        let count = self
            .connection
            .query_row(
                "SELECT COUNT(*)
                 FROM funding_attempts
                 WHERE status NOT IN ('confirmed', 'failed')",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        usize::try_from(count).map_err(|error| AppError::Storage(error.to_string()))
    }
}

struct FundingAttemptRow {
    id: String,
    chain_id: String,
    kind: String,
    purpose: String,
    asset: String,
    amount: String,
    transfer_type: Option<String>,
    product_id: Option<String>,
    exchange_ref: Option<String>,
    status: String,
    message: String,
    query_after_at: Option<String>,
    query_count: i64,
    effective_query_count: i64,
    transport_error_count: i64,
    submit_error_kind: Option<String>,
    history_error_kind: Option<String>,
    history_window_start_at: Option<String>,
    history_window_end_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl FundingAttemptRow {
    fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            chain_id: row.get(1)?,
            kind: row.get(2)?,
            purpose: row.get(3)?,
            asset: row.get(4)?,
            amount: row.get(5)?,
            transfer_type: row.get(6)?,
            product_id: row.get(7)?,
            exchange_ref: row.get(8)?,
            status: row.get(9)?,
            message: row.get(10)?,
            query_after_at: row.get(11)?,
            query_count: row.get(12)?,
            effective_query_count: row.get(13)?,
            transport_error_count: row.get(14)?,
            submit_error_kind: row.get(15)?,
            history_error_kind: row.get(16)?,
            history_window_start_at: row.get(17)?,
            history_window_end_at: row.get(18)?,
            created_at: row.get(19)?,
            updated_at: row.get(20)?,
        })
    }

    fn into_attempt(self) -> AppResult<FundingAttempt> {
        Ok(FundingAttempt {
            id: self.id,
            chain_id: self.chain_id,
            kind: parse_funding_kind(&self.kind)?,
            purpose: self.purpose,
            asset: self.asset,
            amount: parse_decimal(&self.amount)?,
            transfer_type: self.transfer_type,
            product_id: self.product_id,
            exchange_ref: self.exchange_ref,
            status: parse_funding_status(&self.status)?,
            message: self.message,
            query_after_at: self.query_after_at.as_deref().map(parse_time).transpose()?,
            query_count: parse_nonnegative_u32(self.query_count, "query_count")?,
            effective_query_count: parse_nonnegative_u32(
                self.effective_query_count,
                "effective_query_count",
            )?,
            transport_error_count: parse_nonnegative_u32(
                self.transport_error_count,
                "transport_error_count",
            )?,
            submit_error_kind: self.submit_error_kind,
            history_error_kind: self.history_error_kind,
            history_window_start_at: self
                .history_window_start_at
                .as_deref()
                .map(parse_time)
                .transpose()?,
            history_window_end_at: self
                .history_window_end_at
                .as_deref()
                .map(parse_time)
                .transpose()?,
            created_at: parse_time(&self.created_at)?,
            updated_at: parse_time(&self.updated_at)?,
        })
    }
}

fn parse_time(value: &str) -> AppResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map_err(|error| AppError::Storage(error.to_string()))
        .map(|time| time.with_timezone(&Utc))
}

/// 解析 `SQLite` 中持久化的恢复状态。
fn parse_attempt_status(value: &str) -> AppResult<OrderAttemptStatus> {
    match value {
        "attempted" => Ok(OrderAttemptStatus::Attempted),
        "exchange_ack" => Ok(OrderAttemptStatus::ExchangeAck),
        "persisted" => Ok(OrderAttemptStatus::Persisted),
        "failed" => Ok(OrderAttemptStatus::Failed),
        "persist_failed" => Ok(OrderAttemptStatus::PersistFailed),
        "spot_attempted" => Ok(OrderAttemptStatus::SpotAttempted),
        "spot_persisted" => Ok(OrderAttemptStatus::SpotPersisted),
        _ => Err(AppError::Storage(format!("未知订单尝试状态：{value}"))),
    }
}

fn parse_decimal(value: &str) -> AppResult<Decimal> {
    // 恢复记录里的金额必须保持 Decimal 可解析；解析失败说明本地事实表已损坏。
    Decimal::from_str(value)
        .map_err(|error| AppError::Storage(format!("恢复记录金额无法解析：{error}")))
}

fn parse_nonnegative_u32(value: i64, field: &str) -> AppResult<u32> {
    // 查询计数只允许非负值，避免损坏数据绕过恢复重试上限。
    u32::try_from(value)
        .map_err(|error| AppError::Storage(format!("资金恢复记录 {field} 无法解析：{error}")))
}

/// 解析 `SQLite` 中持久化的资金动作类型。
fn parse_funding_kind(value: &str) -> AppResult<FundingAttemptKind> {
    match value {
        "universal_transfer" => Ok(FundingAttemptKind::UniversalTransfer),
        "simple_earn_redeem" => Ok(FundingAttemptKind::SimpleEarnRedeem),
        "simple_earn_subscribe" => Ok(FundingAttemptKind::SimpleEarnSubscribe),
        _ => Err(AppError::Storage(format!("未知资金动作类型：{value}"))),
    }
}

/// 解析 `SQLite` 中持久化的资金恢复状态。
fn parse_funding_status(value: &str) -> AppResult<FundingAttemptStatus> {
    match value {
        "attempted" => Ok(FundingAttemptStatus::Attempted),
        "exchange_ack" => Ok(FundingAttemptStatus::ExchangeAck),
        "uncertain" => Ok(FundingAttemptStatus::Uncertain),
        "pending" => Ok(FundingAttemptStatus::Pending),
        "confirmed" => Ok(FundingAttemptStatus::Confirmed),
        "failed" => Ok(FundingAttemptStatus::Failed),
        "blocked" => Ok(FundingAttemptStatus::Blocked),
        _ => Err(AppError::Storage(format!("未知资金恢复状态：{value}"))),
    }
}
