//! 当前卖方净仓决定跟踪范围；历史成交和行权记录都不能新增交割义务。

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

use super::AppState;
use crate::domain::{
    AppError, AppResult, DeliveryStatus, DeliveryTask, OptionContract, OptionPosition,
    PositionStatus, StrategyDirection, TrackedPosition,
};
use crate::messages::Message;

/// 到期前已登记且未平仓的净仓冻结为待确认任务，保留最后数量供核对，不能据此下单。
fn expire_positions(positions: &mut [TrackedPosition], now: DateTime<Utc>) -> Vec<DeliveryTask> {
    let mut tasks = Vec::new();
    for position in positions
        .iter_mut()
        .filter(|position| position.status == PositionStatus::Open && position.expiry_at <= now)
    {
        position.status = PositionStatus::Expired;
        tasks.push(DeliveryTask {
            id: format!("delivery-{}", position.option_symbol),
            symbol: position.symbol.clone(),
            option_symbol: position.option_symbol.clone(),
            direction: position.direction,
            status: DeliveryStatus::WaitingExercise,
            expiry_at: position.expiry_at,
            tracked_at: position.tracked_at,
            strike: position.strike,
            unit: position.unit,
            last_position_quantity: position.quantity,
            quantity: None,
            settlement_price: None,
            cash_settlement: None,
            exercise_record_ids: Vec::new(),
            spot_order_id: None,
            spot_client_order_id: String::new(),
            spot_order_status: String::new(),
            spot_filled_quantity: Decimal::ZERO,
            spot_avg_price: Decimal::ZERO,
            message: crate::msg!("backend.waitingForExpiryAndBinanceExerciseRecords"),
            created_at: now,
            updated_at: now,
            completed_at: None,
        });
    }
    tasks
}

fn reconcile_positions(
    previous: Vec<TrackedPosition>,
    raw: &[OptionPosition],
    contracts: &[OptionContract],
    observed_at: DateTime<Utc>,
) -> AppResult<(Vec<TrackedPosition>, Vec<DeliveryTask>)> {
    let mut positions = previous;
    // 响应跨过到期时刻时，先冻结旧义务，不能将到期后消失解释为提前平仓。
    let tasks = expire_positions(&mut positions, observed_at);
    let mut indexed: BTreeMap<_, _> = positions
        .into_iter()
        .map(|position| (position.option_symbol.clone(), position))
        .collect();
    let mut seen = HashSet::new();
    for position in raw.iter().filter(|position| {
        position.quantity < Decimal::ZERO
            && position.expiry_at > observed_at
            && position.quote_asset == "USDT"
            && position.symbol.starts_with("BTC-")
    }) {
        let contract = contracts
            .iter()
            .find(|contract| contract.symbol == position.symbol)
            .ok_or_else(|| {
                AppError::response(crate::msg!("backend.positionContractUnavailable"))
            })?;
        if contract.underlying != "BTCUSDT" || contract.quote_asset != "USDT" {
            continue;
        }
        if contract.unit <= Decimal::ZERO
            || contract.expiry_at != position.expiry_at
            || contract.strike_price != position.strike
            || contract.side != position.option_side
            || !seen.insert(position.symbol.clone())
        {
            return Err(AppError::response(crate::msg!(
                "backend.positionSnapshotInconsistent"
            )));
        }
        let direction = match contract.side.as_str() {
            "PUT" => StrategyDirection::SellPut,
            "CALL" => StrategyDirection::SellCall,
            _ => {
                return Err(AppError::response(crate::msg!(
                    "backend.positionSnapshotInconsistent"
                )));
            }
        };
        let tracked_at = indexed
            .get(&position.symbol)
            .filter(|stored| stored.status == PositionStatus::Open)
            .map_or(observed_at, |stored| stored.tracked_at);
        indexed.insert(
            position.symbol.clone(),
            TrackedPosition {
                option_symbol: position.symbol.clone(),
                symbol: contract.underlying.clone(),
                direction,
                status: PositionStatus::Open,
                expiry_at: contract.expiry_at,
                strike: contract.strike_price,
                unit: contract.unit,
                quantity: position.quantity.abs(),
                entry_price: position.entry_price,
                mark_price: position.mark_price,
                unrealized_pnl: position.unrealized_pnl,
                tracked_at,
                last_synced_at: observed_at,
            },
        );
    }
    for position in indexed.values_mut().filter(|position| {
        position.status == PositionStatus::Open && !seen.contains(&position.option_symbol)
    }) {
        position.status = PositionStatus::Closed;
        position.quantity = Decimal::ZERO;
        position.unrealized_pnl = Decimal::ZERO;
        position.last_synced_at = observed_at;
    }
    Ok((indexed.into_values().collect(), tasks))
}

impl AppState {
    async fn persist_position_sync(
        &self,
        positions: Vec<TrackedPosition>,
        tasks: &[DeliveryTask],
    ) -> AppResult<()> {
        let deliveries = {
            let persistence = self
                .persistence
                .lock()
                .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
            persistence.save_position_sync(&positions, tasks)?;
            persistence.list_delivery_tasks()?
        };
        *self.positions.write().await = positions;
        *self.deliveries.write().await = deliveries;
        Ok(())
    }

    /// 调用者持交易锁。失败保留上次成功的仓位，不能把失败当作空仓。
    pub(super) async fn sync_option_positions(&self) -> AppResult<()> {
        let result = self.sync_option_positions_inner().await;
        let mut runtime = self.runtime.write().await;
        match &result {
            Ok(()) => {
                runtime.position_sync.updated_at = Some(Utc::now());
                runtime.position_sync.error = None;
            }
            Err(error) => runtime.position_sync.error = Some(error.message()),
        }
        result
    }

    async fn sync_option_positions_inner(&self) -> AppResult<()> {
        let client = self.live_client().await?;
        let mut previous = self.positions.read().await.clone();
        let tasks = expire_positions(&mut previous, Utc::now());
        if !tasks.is_empty() {
            self.persist_position_sync(previous.clone(), &tasks).await?;
        }
        let raw = client.option_positions().await?;
        let contracts = client.exchange_info().await?;
        // 必须使用全部响应完成后的时间，迟到的历史仓位不获得首次登记资格。
        let (positions, tasks) = reconcile_positions(previous, &raw, &contracts, Utc::now())?;
        self.persist_position_sync(positions, &tasks).await
    }

    pub(super) async fn sync_positions_best_effort(&self) {
        if let Err(error) = self.sync_option_positions().await {
            self.record_warning("position.sync_failed", &error.message());
        }
    }

    pub(super) async fn save_delivery(&self, task: &DeliveryTask) -> AppResult<()> {
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?
            .save_delivery_task(task)?;
        let mut deliveries = self.deliveries.write().await;
        if let Some(stored) = deliveries.iter_mut().find(|stored| stored.id == task.id) {
            *stored = task.clone();
        } else {
            deliveries.push(task.clone());
        }
        Ok(())
    }
}
