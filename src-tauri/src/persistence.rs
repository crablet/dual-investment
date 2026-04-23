//! 本地 SQLite 持久化层。
//!
//! 这里只保存业务事件、订单快照、自动规则和恢复记录；API Key/Secret 永远不进入
//! SQLite。订单和审计以追加/替换业务事实的方式保存，便于应用重启后从真实交易所状态恢复。

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};

use crate::domain::{
    AppError, AppResult, AuditEvent, AutoInvestmentRule, OrderAttempt, OrderAttemptStatus,
    StrategyDirection, StrategyOrder,
};

pub struct Persistence {
    connection: Connection,
}

impl Persistence {
    /// 打开应用数据目录中的 SQLite 数据库。
    pub fn open_app_data(data_dir: impl AsRef<Path>) -> AppResult<Self> {
        std::fs::create_dir_all(data_dir.as_ref())
            .map_err(|error| AppError::Storage(error.to_string()))?;
        Self::open(data_dir.as_ref().join("dual-investment.db"))
    }

    /// 打开指定 SQLite 文件并确保表结构存在。
    pub fn open(path: impl AsRef<Path>) -> AppResult<Self> {
        let connection =
            Connection::open(path).map_err(|error| AppError::Storage(error.to_string()))?;
        let store = Self { connection };
        store.init()?;
        Ok(store)
    }

    /// 初始化本地表结构。
    ///
    /// `strategy_orders` 和 `auto_rule` 使用 JSON payload，是为了让领域结构演进时减少
    /// 迁移成本；`order_attempts` 单独拆列，是因为恢复流程需要稳定按状态和订单号查询。
    fn init(&self) -> AppResult<()> {
        self.connection
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS audit_events (
                    id TEXT PRIMARY KEY,
                    kind TEXT NOT NULL,
                    message TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );

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
                    direction TEXT NOT NULL,
                    status TEXT NOT NULL,
                    exchange_order_id TEXT,
                    message TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS auto_rule (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    payload TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_audit_events_created_at
                    ON audit_events (created_at DESC);
                CREATE INDEX IF NOT EXISTS idx_strategy_orders_updated_at
                    ON strategy_orders (updated_at DESC);
                CREATE INDEX IF NOT EXISTS idx_order_attempts_updated_at
                    ON order_attempts (updated_at DESC);
                ",
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        Ok(())
    }

    /// 写入审计事件。
    ///
    /// 调用方必须传入脱敏后的业务摘要，不能把 Secret、签名 query 或原始响应写入 message。
    pub fn append_audit(&self, event: &AuditEvent) -> AppResult<()> {
        self.connection
            .execute(
                "INSERT OR REPLACE INTO audit_events (id, kind, message, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![
                    event.id,
                    event.kind,
                    event.message,
                    event.created_at.to_rfc3339(),
                ],
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        Ok(())
    }

    /// 保存本地策略订单快照。
    ///
    /// 订单必须已绑定真实交易所订单号或来自恢复查询结果。
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
                    direction,
                    status,
                    exchange_order_id,
                    message,
                    created_at,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    attempt.id,
                    attempt.client_order_id,
                    attempt.symbol,
                    attempt.option_symbol,
                    match attempt.direction {
                        crate::domain::StrategyDirection::SellPut => "sellPut",
                        crate::domain::StrategyDirection::SellCall => "sellCall",
                    },
                    attempt.status.as_str(),
                    attempt.exchange_order_id.as_deref(),
                    attempt.message,
                    attempt.created_at.to_rfc3339(),
                    attempt.updated_at.to_rfc3339(),
                ],
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        Ok(())
    }

    /// 保存自动复投规则。
    ///
    /// 规则可持久化，但应用启动时会强制关闭自动执行状态。
    pub fn save_auto_rule(&self, rule: &AutoInvestmentRule) -> AppResult<()> {
        let payload =
            serde_json::to_string(rule).map_err(|error| AppError::Storage(error.to_string()))?;
        self.connection
            .execute(
                "INSERT OR REPLACE INTO auto_rule (id, payload, updated_at) VALUES (1, ?1, ?2)",
                params![payload, Utc::now().to_rfc3339()],
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        Ok(())
    }

    /// 读取自动复投规则；没有保存过则由策略层提供默认值。
    pub fn load_auto_rule(&self) -> AppResult<Option<AutoInvestmentRule>> {
        let mut statement = self
            .connection
            .prepare("SELECT payload FROM auto_rule WHERE id = 1")
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let mut rows = statement
            .query([])
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let Some(row) = rows
            .next()
            .map_err(|error| AppError::Storage(error.to_string()))?
        else {
            return Ok(None);
        };
        let payload = row
            .get::<_, String>(0)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        serde_json::from_str::<AutoInvestmentRule>(&payload)
            .map(Some)
            .map_err(|error| AppError::Storage(error.to_string()))
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
                "SELECT id, client_order_id, symbol, option_symbol, direction, status, exchange_order_id, message, created_at, updated_at
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
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
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
                direction,
                status,
                exchange_order_id,
                message,
                created_at,
                updated_at,
            ) = row.map_err(|error| AppError::Storage(error.to_string()))?;
            attempts.push(OrderAttempt {
                id,
                client_order_id,
                symbol,
                option_symbol,
                direction: parse_direction(&direction)?,
                status: parse_attempt_status(&status)?,
                exchange_order_id,
                message,
                created_at: parse_time(&created_at)?,
                updated_at: parse_time(&updated_at)?,
            });
        }
        Ok(attempts)
    }

    /// 读取近期审计事件。
    pub fn recent_audit(&self, limit: usize) -> AppResult<Vec<AuditEvent>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, kind, message, created_at FROM audit_events ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let mapped = statement
            .query_map([limit], |row| {
                let created_at = row.get::<_, String>(3)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    created_at,
                ))
            })
            .map_err(|error| AppError::Storage(error.to_string()))?;

        let mut events = Vec::new();
        for row in mapped {
            let (id, kind, message, created_at) =
                row.map_err(|error| AppError::Storage(error.to_string()))?;
            let created_at = parse_time(&created_at)?;
            events.push(AuditEvent {
                id,
                kind,
                message,
                created_at,
            });
        }
        Ok(events)
    }
}

fn parse_time(value: &str) -> AppResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map_err(|error| AppError::Storage(error.to_string()))
        .map(|time| time.with_timezone(&Utc))
}

/// 解析 SQLite 中持久化的策略方向。
fn parse_direction(value: &str) -> AppResult<StrategyDirection> {
    match value {
        "sellPut" => Ok(StrategyDirection::SellPut),
        "sellCall" => Ok(StrategyDirection::SellCall),
        _ => Err(AppError::Storage(format!("未知订单尝试方向：{value}"))),
    }
}

/// 解析 SQLite 中持久化的恢复状态。
fn parse_attempt_status(value: &str) -> AppResult<OrderAttemptStatus> {
    match value {
        "attempted" => Ok(OrderAttemptStatus::Attempted),
        "exchange_ack" => Ok(OrderAttemptStatus::ExchangeAck),
        "persisted" => Ok(OrderAttemptStatus::Persisted),
        "persist_failed" => Ok(OrderAttemptStatus::PersistFailed),
        "spot_attempted" => Ok(OrderAttemptStatus::SpotAttempted),
        "spot_exchange_ack" => Ok(OrderAttemptStatus::SpotExchangeAck),
        "spot_persisted" => Ok(OrderAttemptStatus::SpotPersisted),
        "spot_persist_failed" => Ok(OrderAttemptStatus::SpotPersistFailed),
        _ => Err(AppError::Storage(format!("未知订单尝试状态：{value}"))),
    }
}
