//! 本地 `SQLite` 持久化层。
//!
//! 这里只保存业务事件、订单快照、自动规则和恢复记录；API Key/Secret 永远不进入
//! SQLite。订单和审计以追加/替换业务事实的方式保存，便于应用重启后从真实交易所状态恢复。

use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use rust_decimal::Decimal;
use std::str::FromStr;

use crate::domain::{
    AppError, AppResult, AutoInvestmentRule, FundingAttempt, FundingAttemptKind,
    FundingAttemptStatus, OrderAttempt, OrderAttemptStatus, StrategyDirection, StrategyOrder,
};

pub struct Persistence {
    connection: Connection,
}

impl Persistence {
    /// 打开应用数据目录中的 `SQLite` 数据库。
    pub fn open_app_data(data_dir: impl AsRef<Path>) -> AppResult<Self> {
        std::fs::create_dir_all(data_dir.as_ref())
            .map_err(|error| AppError::Storage(error.to_string()))?;
        Self::open(data_dir.as_ref().join("dual-investment.db"))
    }

    /// 打开指定 `SQLite` 文件并确保表结构存在。
    pub fn open(path: impl AsRef<Path>) -> AppResult<Self> {
        let connection =
            Connection::open(path).map_err(|error| AppError::Storage(error.to_string()))?;
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
                    direction TEXT NOT NULL,
                    status TEXT NOT NULL,
                    exchange_order_id TEXT,
                    message TEXT NOT NULL,
                    created_at TEXT NOT NULL,
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
                    send_count INTEGER NOT NULL,
                    query_count INTEGER NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS auto_rule (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
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
                    ON funding_attempts (status, updated_at DESC);
                CREATE INDEX IF NOT EXISTS idx_funding_attempts_chain
                    ON funding_attempts (chain_id, kind);
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
                    send_count,
                    query_count,
                    created_at,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
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
                    i64::from(attempt.send_count),
                    i64::from(attempt.query_count),
                    attempt.created_at.to_rfc3339(),
                    attempt.updated_at.to_rfc3339(),
                ],
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        Ok(())
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

    /// 按更新时间倒序读取资金动作恢复记录。
    pub fn list_funding_attempts(&self, limit: usize) -> AppResult<Vec<FundingAttempt>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, chain_id, kind, purpose, asset, amount, transfer_type, product_id,
                        exchange_ref, status, message, query_after_at, send_count, query_count,
                        created_at, updated_at
                 FROM funding_attempts
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
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, i64>(12)?,
                    row.get::<_, i64>(13)?,
                    row.get::<_, String>(14)?,
                    row.get::<_, String>(15)?,
                ))
            })
            .map_err(|error| AppError::Storage(error.to_string()))?;

        let mut attempts = Vec::new();
        for row in mapped {
            let (
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
                send_count,
                query_count,
                created_at,
                updated_at,
            ) = row.map_err(|error| AppError::Storage(error.to_string()))?;
            attempts.push(FundingAttempt {
                id,
                chain_id,
                kind: parse_funding_kind(&kind)?,
                purpose,
                asset,
                amount: parse_decimal(&amount)?,
                transfer_type,
                product_id,
                exchange_ref,
                status: parse_funding_status(&status)?,
                message,
                query_after_at: query_after_at.as_deref().map(parse_time).transpose()?,
                send_count: parse_nonnegative_u32(send_count, "send_count")?,
                query_count: parse_nonnegative_u32(query_count, "query_count")?,
                created_at: parse_time(&created_at)?,
                updated_at: parse_time(&updated_at)?,
            });
        }
        Ok(attempts)
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

fn parse_time(value: &str) -> AppResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map_err(|error| AppError::Storage(error.to_string()))
        .map(|time| time.with_timezone(&Utc))
}

/// 解析 `SQLite` 中持久化的策略方向。
fn parse_direction(value: &str) -> AppResult<StrategyDirection> {
    match value {
        "sellPut" => Ok(StrategyDirection::SellPut),
        "sellCall" => Ok(StrategyDirection::SellCall),
        _ => Err(AppError::Storage(format!("未知订单尝试方向：{value}"))),
    }
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
        "spot_exchange_ack" => Ok(OrderAttemptStatus::SpotExchangeAck),
        "spot_persisted" => Ok(OrderAttemptStatus::SpotPersisted),
        "spot_persist_failed" => Ok(OrderAttemptStatus::SpotPersistFailed),
        _ => Err(AppError::Storage(format!("未知订单尝试状态：{value}"))),
    }
}

fn parse_decimal(value: &str) -> AppResult<Decimal> {
    // 恢复记录里的金额必须保持 Decimal 可解析；解析失败说明本地事实表已损坏。
    Decimal::from_str(value)
        .map_err(|error| AppError::Storage(format!("恢复记录金额无法解析：{error}")))
}

fn parse_nonnegative_u32(value: i64, field: &str) -> AppResult<u32> {
    // send/query 计数只允许非负值，避免损坏数据绕过恢复重试上限。
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
