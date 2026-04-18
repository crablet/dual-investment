use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};

use crate::domain::{AppError, AppResult, AuditEvent, StrategyOrder};

pub struct Persistence {
    connection: Connection,
}

impl Persistence {
    pub fn open_default() -> AppResult<Self> {
        let data_dir = Path::new("data");
        std::fs::create_dir_all(data_dir).map_err(|error| AppError::Storage(error.to_string()))?;
        Self::open(data_dir.join("dual-investment.db"))
    }

    pub fn open(path: impl AsRef<Path>) -> AppResult<Self> {
        let connection =
            Connection::open(path).map_err(|error| AppError::Storage(error.to_string()))?;
        let store = Self { connection };
        store.init()?;
        Ok(store)
    }

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
                ",
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        Ok(())
    }

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

    pub fn recent_audit(&self, limit: usize) -> AppResult<Vec<AuditEvent>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, kind, message, created_at FROM audit_events ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let mapped = statement
            .query_map([limit as i64], |row| {
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
            let created_at = DateTime::parse_from_rfc3339(&created_at)
                .map_err(|error| AppError::Storage(error.to_string()))?
                .with_timezone(&Utc);
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
