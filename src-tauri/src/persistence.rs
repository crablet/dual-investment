//! 保存账户绑定、订单、净仓、交割任务和恢复记录。API 凭据只存系统 keyring。

use crate::messages::Message;

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, Row, params};
use rust_decimal::Decimal;
use std::str::FromStr;

use crate::domain::{
    AppError, AppResult, DeliveryTask, FundingAttempt, FundingAttemptKind, FundingAttemptStatus,
    OrderAttempt, OrderAttemptStatus, SettlementBatch, StrategyOrder, TrackedPosition,
};

const ORDER_ATTEMPT_COLUMNS: &str =
    "id, client_order_id, symbol, option_symbol, status, exchange_order_id, message, updated_at";
const FUNDING_ATTEMPT_COLUMNS: &str = "id, chain_id, kind, purpose, asset, amount, transfer_type, product_id,
    exchange_ref, status, message, query_after_at, query_count, effective_query_count, transport_error_count,
    submit_error_kind, history_error_kind, history_window_start_at, history_window_end_at, created_at, updated_at";

pub struct Persistence {
    connection: Connection,
}

impl Persistence {
    /// 打开应用数据目录中的 `SQLite` 数据库。
    pub fn open_app_data(data_dir: impl AsRef<Path>) -> AppResult<Self> {
        std::fs::create_dir_all(data_dir.as_ref())
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        let connection = Connection::open(data_dir.as_ref().join("dual-investment.db"))
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        let store = Self { connection };
        store.init()?;
        Ok(store)
    }

    /// 初始化本地表结构。
    ///
    /// 只创建当前表结构，不迁移或自动清理旧库。恢复记录单独存列，便于按状态、编号和查询时间检索。
    fn init(&self) -> AppResult<()> {
        self.connection
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS account_binding (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    uid TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS strategy_orders (
                    id TEXT PRIMARY KEY,
                    payload TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS option_positions (
                    option_symbol TEXT PRIMARY KEY,
                    payload TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS delivery_tasks (
                    id TEXT PRIMARY KEY,
                    option_symbol TEXT NOT NULL UNIQUE,
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
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        Ok(())
    }

    /// 首次验证账户时绑定数据库；已有归属只允许同一 UID，删除凭据不解除绑定。
    pub fn bind_account(&self, uid: &str) -> AppResult<()> {
        self.connection
            .execute(
                "INSERT INTO account_binding (id, uid) VALUES (1, ?1)
                 ON CONFLICT (id) DO NOTHING",
                [uid],
            )
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        let bound_uid: String = self
            .connection
            .query_row("SELECT uid FROM account_binding WHERE id = 1", [], |row| {
                row.get(0)
            })
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        if bound_uid != uid {
            return Err(AppError::Config(crate::msg!(
                "backend.localDataBelongsToAnotherBinanceAccount"
            )));
        }
        Ok(())
    }

    /// 保存本地策略订单快照。
    ///
    /// 订单必须来自本机下单或恢复查询；净持仓与交割义务单独保存。
    pub fn save_order(&self, order: &StrategyOrder) -> AppResult<()> {
        let payload = serde_json::to_string(order)
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        self.connection
            .execute(
                "INSERT OR REPLACE INTO strategy_orders (id, payload, updated_at) VALUES (?1, ?2, ?3)",
                params![order.id, payload, order.updated_at.to_rfc3339()],
            )
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        Ok(())
    }

    /// 保存下单恢复记录。
    ///
    /// 交易所接单后即使订单保存失败，仍可据此查询原订单。
    pub fn save_order_attempt(&self, attempt: &OrderAttempt) -> AppResult<()> {
        self.connection
            .execute(
                &format!("INSERT OR REPLACE INTO order_attempts ({ORDER_ATTEMPT_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"),
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
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        Ok(())
    }

    /// 保存真实资金动作恢复记录。
    pub fn save_funding_attempt(&self, attempt: &FundingAttempt) -> AppResult<()> {
        self.connection
            .execute(
                &format!("INSERT OR REPLACE INTO funding_attempts ({FUNDING_ATTEMPT_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)"),
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
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        Ok(())
    }

    pub fn save_settlement_batch(&self, batch: &SettlementBatch) -> AppResult<()> {
        let payload = serde_json::to_string(batch)
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        self.connection.execute(
            "INSERT INTO settlement_batches (id, payload, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET payload=excluded.payload, updated_at=excluded.updated_at",
            params![batch.id, payload, batch.updated_at.to_rfc3339()],
        ).map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
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
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        self.save_order(order)?;
        self.save_order_attempt(attempt)?;
        transaction
            .commit()
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))
    }

    /// 最后一笔状态和收尾计划一起保存；已有批次记录也可从成交事实重建该步骤。
    pub fn save_batch_and_deliveries(
        &self,
        batch: &SettlementBatch,
        tasks: &[DeliveryTask],
    ) -> AppResult<()> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        for task in tasks {
            self.save_delivery_task(task)?;
        }
        self.save_settlement_batch(batch)?;
        transaction
            .commit()
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))
    }

    pub fn list_positions(&self) -> AppResult<Vec<TrackedPosition>> {
        self.read_payloads("SELECT payload FROM option_positions ORDER BY option_symbol")
    }

    pub fn list_delivery_tasks(&self) -> AppResult<Vec<DeliveryTask>> {
        self.read_payloads("SELECT payload FROM delivery_tasks ORDER BY updated_at DESC")
    }

    fn read_payloads<T: serde::de::DeserializeOwned>(&self, sql: &str) -> AppResult<Vec<T>> {
        let mut statement = self
            .connection
            .prepare(sql)
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        rows.map(|row| {
            let payload =
                row.map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
            serde_json::from_str(&payload)
                .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))
        })
        .collect()
    }

    /// 净仓快照与首次到期义务一起提交；重复扫描不会重置已有交割任务。
    pub fn save_position_sync(
        &self,
        positions: &[TrackedPosition],
        tasks: &[DeliveryTask],
    ) -> AppResult<()> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        for position in positions {
            let payload = serde_json::to_string(position)
                .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
            self.connection
                .execute(
                    "INSERT INTO option_positions (option_symbol, payload) VALUES (?1, ?2)
                 ON CONFLICT(option_symbol) DO UPDATE SET payload=excluded.payload",
                    params![position.option_symbol, payload],
                )
                .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        }
        for task in tasks {
            let payload = serde_json::to_string(task)
                .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
            self.connection.execute(
                "INSERT INTO delivery_tasks (id, option_symbol, payload, updated_at) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO NOTHING",
                params![task.id, task.option_symbol, payload, task.updated_at.to_rfc3339()],
            ).map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        }
        transaction
            .commit()
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))
    }

    pub fn save_delivery_task(&self, task: &DeliveryTask) -> AppResult<()> {
        let payload = serde_json::to_string(task)
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        self.connection.execute(
            "INSERT INTO delivery_tasks (id, option_symbol, payload, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET payload=excluded.payload, updated_at=excluded.updated_at",
            params![task.id, task.option_symbol, payload, task.updated_at.to_rfc3339()],
        ).map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        Ok(())
    }

    /// 固定现货客户端订单号与恢复记录必须在发送请求前原子落库。
    pub fn save_delivery_and_attempt(
        &self,
        task: &DeliveryTask,
        attempt: &OrderAttempt,
    ) -> AppResult<()> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        self.save_delivery_task(task)?;
        self.save_order_attempt(attempt)?;
        transaction
            .commit()
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))
    }

    /// 按更新时间倒序读取订单。
    pub fn list_orders(&self) -> AppResult<Vec<StrategyOrder>> {
        let mut statement = self
            .connection
            .prepare("SELECT payload FROM strategy_orders ORDER BY updated_at DESC")
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        let mapped = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        let mut orders = Vec::new();
        for row in mapped {
            let payload =
                row.map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
            let mut order = serde_json::from_str::<StrategyOrder>(&payload)
                .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
            crate::strategy::refresh_order_metrics(&mut order);
            orders.push(order);
        }
        Ok(orders)
    }

    /// 按更新时间倒序读取恢复记录。
    pub fn list_order_attempts(&self, limit: usize) -> AppResult<Vec<OrderAttempt>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = self.connection.prepare(&format!(
            "SELECT {ORDER_ATTEMPT_COLUMNS} FROM order_attempts ORDER BY updated_at DESC LIMIT ?1"
        )).map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        let rows = statement
            .query_map([limit], read_order_attempt)
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        rows.map(|row| {
            row.map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))
        })
        .collect()
    }

    pub fn get_order_attempt(&self, id: &str) -> AppResult<Option<OrderAttempt>> {
        self.connection
            .query_row(
                &format!("SELECT {ORDER_ATTEMPT_COLUMNS} FROM order_attempts WHERE id = ?1"),
                [id],
                read_order_attempt,
            )
            .optional()
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))
    }

    pub fn get_funding_attempt(&self, id: &str) -> AppResult<Option<FundingAttempt>> {
        self.connection
            .query_row(
                &format!("SELECT {FUNDING_ATTEMPT_COLUMNS} FROM funding_attempts WHERE id = ?1"),
                [id],
                FundingAttemptRow::read,
            )
            .optional()
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?
            .map(FundingAttemptRow::into_attempt)
            .transpose()
    }

    /// 按更新时间倒序读取资金动作恢复记录。
    pub fn list_funding_attempts(&self, limit: usize) -> AppResult<Vec<FundingAttempt>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = self
            .connection
            .prepare(
                &format!("SELECT {FUNDING_ATTEMPT_COLUMNS} FROM funding_attempts ORDER BY updated_at DESC LIMIT ?1"),
            )
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        let mapped = statement
            .query_map([limit], FundingAttemptRow::read)
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;

        let mut attempts = Vec::new();
        for row in mapped {
            let row =
                row.map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
            attempts.push(row.into_attempt()?);
        }
        Ok(attempts)
    }

    /// 按资金链路读取恢复记录。
    pub fn list_funding_attempts_by_chain(&self, chain_id: &str) -> AppResult<Vec<FundingAttempt>> {
        let mut statement = self
            .connection
            .prepare(
                &format!("SELECT {FUNDING_ATTEMPT_COLUMNS} FROM funding_attempts WHERE chain_id = ?1 ORDER BY created_at ASC"),
            )
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        let mapped = statement
            .query_map([chain_id], FundingAttemptRow::read)
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        let mut attempts = Vec::new();
        for row in mapped {
            let row =
                row.map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
            attempts.push(row.into_attempt()?);
        }
        Ok(attempts)
    }

    pub fn list_settlement_batches(&self) -> AppResult<Vec<SettlementBatch>> {
        let mut statement = self
            .connection
            .prepare("SELECT payload FROM settlement_batches ORDER BY updated_at DESC")
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        rows.map(|row| {
            let payload =
                row.map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
            serde_json::from_str(&payload)
                .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))
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
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        usize::try_from(count)
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))
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
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        usize::try_from(count)
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))
    }
}

fn read_order_attempt(row: &Row<'_>) -> rusqlite::Result<OrderAttempt> {
    let status = parse_attempt_status(&row.get::<_, String>(4)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let updated_at = parse_time(&row.get::<_, String>(7)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(OrderAttempt {
        id: row.get(0)?,
        client_order_id: row.get(1)?,
        symbol: row.get(2)?,
        option_symbol: row.get(3)?,
        status,
        exchange_order_id: row.get(5)?,
        message: row.get(6)?,
        updated_at,
    })
}

struct FundingAttemptRow {
    id: String,
    chain_id: String,
    kind: String,
    purpose: Message,
    asset: String,
    amount: String,
    transfer_type: Option<String>,
    product_id: Option<String>,
    exchange_ref: Option<String>,
    status: String,
    message: Message,
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
        .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))
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
        _ => Err(AppError::Storage(crate::msg!(
            "backend.unknownOrderAttemptStatus",
            p0 = value
        ))),
    }
}

fn parse_decimal(value: &str) -> AppResult<Decimal> {
    // 金额无法解析为 Decimal 时视为记录损坏。
    Decimal::from_str(value).map_err(|error| {
        AppError::Storage(crate::msg!("backend.cannotParseRecoveryAmount", p0 = error))
    })
}

fn parse_nonnegative_u32(value: i64, field: &str) -> AppResult<u32> {
    // 查询计数只允许非负值，避免损坏数据绕过恢复重试上限。
    u32::try_from(value).map_err(|error| {
        AppError::Storage(crate::msg!(
            "backend.cannotParseFundingRecoveryField",
            p0 = field,
            p1 = error
        ))
    })
}

/// 解析 `SQLite` 中持久化的资金动作类型。
fn parse_funding_kind(value: &str) -> AppResult<FundingAttemptKind> {
    match value {
        "universal_transfer" => Ok(FundingAttemptKind::UniversalTransfer),
        "simple_earn_redeem" => Ok(FundingAttemptKind::SimpleEarnRedeem),
        "simple_earn_subscribe" => Ok(FundingAttemptKind::SimpleEarnSubscribe),
        _ => Err(AppError::Storage(crate::msg!(
            "backend.unknownFundingActionType",
            p0 = value
        ))),
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
        _ => Err(AppError::Storage(crate::msg!(
            "backend.unknownFundingRecoveryStatus",
            p0 = value
        ))),
    }
}
