//! 到期批次编排。批次先备齐资金，逐笔确认成交，最后执行持久化收尾计划。
//! 等待期间释放运行锁；下一轮必须重新读取记录，已发送的步骤只查询原请求。

use crate::messages::Message;

use std::collections::BTreeMap;

use super::funding::{settlement_required_assets, spot_has_settlement_assets};
use super::{
    AppError, AppResult, AppState, BinanceClient, Decimal, DeliveryStatus, DeliveryTask, Duration,
    OPTION_IDLE_USDT_SWEEP_MIN_AMOUNT, OrderAttempt, OrderAttemptStatus,
    SETTLEMENT_SWEEP_DELAY_MINUTES, SettlementBatch, SettlementStage, SpotAccountSnapshot, Utc,
    Uuid, apply_spot_ack, is_in_the_money, make_spot_client_order_id,
    option_idle_usdt_sweep_amount, round_down_funding_amount, spot_asset_free,
    validate_spot_rebalance,
};
use crate::domain::SettlementAssets;

fn needs_settlement(task: &DeliveryTask) -> bool {
    task.status != DeliveryStatus::Completed
}

fn spot_assets(spot: &SpotAccountSnapshot) -> SettlementAssets {
    SettlementAssets {
        usdt: spot_asset_free(spot, "USDT"),
        btc: spot_asset_free(spot, "BTC"),
    }
}

fn sweep_started(batch: &SettlementBatch) -> bool {
    batch.option_sweep_usdt.is_some() || batch.spot_sweep.is_some()
}

struct ExerciseFacts {
    quantity: Decimal,
    price: Decimal,
    cash: Decimal,
    ids: Vec<String>,
}

fn exercise_facts(
    task: &DeliveryTask,
    records: &[crate::domain::ExerciseRecord],
) -> AppResult<Option<ExerciseFacts>> {
    let mut unique = BTreeMap::<String, &crate::domain::ExerciseRecord>::new();
    for record in records.iter().filter(|record| {
        record.symbol == task.option_symbol
            && record.position_side == "SHORT"
            && record.option_side == task.direction.option_type()
            && record.created_at >= task.expiry_at
            && record.created_at <= task.expiry_at + Duration::days(2)
    }) {
        if record.quantity == Decimal::ZERO
            || record.settlement_price <= Decimal::ZERO
            || record.id.is_empty()
        {
            return Err(AppError::response(crate::msg!(
                "backend.exerciseFactsInconsistent"
            )));
        }
        if let Some(old) = unique.insert(record.id.clone(), record)
            && (old.quantity != record.quantity
                || old.settlement_price != record.settlement_price
                || old.amount != record.amount
                || old.created_at != record.created_at)
        {
            return Err(AppError::response(crate::msg!(
                "backend.exerciseFactsInconsistent"
            )));
        }
    }
    let Some(first) = unique.values().next() else {
        return Ok(None);
    };
    let price = first.settlement_price;
    if unique
        .values()
        .any(|record| record.settlement_price != price)
    {
        return Err(AppError::response(crate::msg!(
            "backend.exerciseFactsInconsistent"
        )));
    }
    Ok(Some(ExerciseFacts {
        quantity: unique.values().map(|record| record.quantity.abs()).sum(),
        cash: unique.values().map(|record| record.amount).sum(),
        price,
        ids: unique.into_keys().collect(),
    }))
}

impl AppState {
    pub(super) fn settlement_batches(&self) -> AppResult<Vec<SettlementBatch>> {
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?
            .list_settlement_batches()
    }

    pub(super) fn save_batch(&self, batch: &mut SettlementBatch) -> AppResult<()> {
        batch.updated_at = Utc::now();
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?
            .save_settlement_batch(batch)
    }

    pub(super) fn update_batch(
        &self,
        batch: &mut SettlementBatch,
        stage: SettlementStage,
        message: &Message,
    ) -> AppResult<()> {
        let changed = batch.stage != stage || batch.message != *message;
        batch.stage = stage;
        batch.message = message.clone();
        self.save_batch(batch)?;
        if changed {
            self.record_log(
                "settlement.batch",
                &crate::msg!("ui.join", p0 = batch.id, p1 = message),
            );
        }
        Ok(())
    }

    /// 已到期的义务和待完成批次都会预留资产，包括等待人工处理的批次。
    pub(super) async fn settlement_reserves_assets(&self) -> AppResult<bool> {
        let now = Utc::now();
        Ok(self.positions.read().await.iter().any(|position| {
            position.status == super::PositionStatus::Open && position.expiry_at <= now
        }) || self
            .deliveries
            .read()
            .await
            .iter()
            .any(|order| needs_settlement(order) && order.expiry_at <= now)
            || self
                .settlement_batches()?
                .iter()
                .any(|batch| batch.expiry_at <= now && !batch.stage.is_terminal()))
    }

    pub(super) async fn discover_settlement_batches(&self) -> AppResult<()> {
        let orders = self.deliveries.read().await.clone();
        let mut groups = BTreeMap::<_, Vec<String>>::new();
        for order in orders
            .iter()
            .filter(|order| needs_settlement(order) && order.expiry_at <= Utc::now())
        {
            groups
                .entry((order.symbol.clone(), order.expiry_at))
                .or_default()
                .push(order.id.clone());
        }
        let existing = self.settlement_batches()?;
        for ((symbol, expiry_at), mut ids) in groups {
            ids.sort();
            let id = format!("settlement-{symbol}-{}", expiry_at.timestamp_millis());
            if let Some(batch) = existing.iter().find(|batch| batch.id == id) {
                if ids.iter().any(|id| !batch.task_ids.contains(id)) {
                    let mut batch = batch.clone();
                    if batch.stage == SettlementStage::WaitingExercise {
                        for id in ids {
                            if !batch.task_ids.contains(&id) {
                                batch.task_ids.push(id);
                            }
                        }
                        batch.task_ids.sort();
                        self.save_batch(&mut batch)?;
                    } else {
                        self.update_batch(
                            &mut batch,
                            SettlementStage::Blocked,
                            &crate::msg!("backend.unregisteredOrdersAppearedAfterThisBatchStarted"),
                        )?;
                    }
                }
                continue;
            }
            let now = Utc::now();
            self.save_batch(&mut SettlementBatch {
                id,
                symbol,
                expiry_at,
                task_ids: ids,
                stage: SettlementStage::WaitingExercise,
                message: crate::msg!("backend.waitingForExpiryAndBinanceExerciseRecords"),
                required: SettlementAssets::default(),
                shortfall: SettlementAssets::default(),
                redemption: None,
                option_funding: None,
                sweep_after_at: None,
                option_sweep_usdt: None,
                spot_usdt_before_transfer: Decimal::ZERO,
                spot_sweep: None,
                updated_at: now,
            })?;
        }
        Ok(())
    }

    async fn batch_tasks(&self, batch: &SettlementBatch) -> AppResult<Vec<DeliveryTask>> {
        let orders = self.deliveries.read().await;
        batch
            .task_ids
            .iter()
            .map(|id| {
                orders
                    .iter()
                    .find(|order| &order.id == id)
                    .cloned()
                    .ok_or_else(|| {
                        AppError::Storage(crate::msg!(
                            "backend.aSettlementBatchMemberOrderIsMissing"
                        ))
                    })
            })
            .collect()
    }

    pub(super) async fn recover_pending_settlement_orders(&self) -> AppResult<()> {
        let _trade_guard = self.trade_lock.lock().await;
        let attempts = self
            .persistence
            .lock()
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?
            .list_order_attempts(usize::MAX)?;
        for attempt in attempts
            .into_iter()
            .filter(|attempt| attempt.status == OrderAttemptStatus::SpotAttempted)
        {
            if let Err(error) = self.recover_spot_attempt(attempt).await {
                self.record_warning(
                    "settlement.order.confirming",
                    &crate::msg!("backend.spotRebalancingAwaitsTheOriginalOrder", p0 = error),
                );
            }
        }
        Ok(())
    }

    /// 调用者持有维护锁和交易锁。每轮先恢复，再推进；有不确定结果就停止发送。
    pub(super) async fn advance_settlement_batches(&self) -> AppResult<()> {
        let client = self.live_client().await?;
        let mut batches = self.settlement_batches()?;
        batches.sort_by_key(|batch| batch.expiry_at);
        for mut batch in batches
            .into_iter()
            .filter(|batch| batch.expiry_at <= Utc::now() && !batch.stage.is_terminal())
        {
            // PendingSweep 必须保留，资金恢复不能使收尾任务退回接货阶段。
            if self.pending_recovery_count()? > 0 {
                let stage = if matches!(
                    batch.stage,
                    SettlementStage::PendingSweep | SettlementStage::Blocked
                ) {
                    batch.stage
                } else {
                    SettlementStage::ConfirmingFunds
                };
                self.update_batch(
                    &mut batch,
                    stage,
                    &crate::msg!("backend.recoveryRecordsAwaitConfirmationFundingActionsWill"),
                )?;
                break;
            }
            let result = self.advance_batch(&client, &mut batch).await;
            if let Err(error) = result {
                let stage = if error.is_retryable_query() || self.pending_recovery_count()? > 0 {
                    batch.stage
                } else {
                    SettlementStage::Blocked
                };
                self.update_batch(
                    &mut batch,
                    stage,
                    &crate::msg!("backend.settlementPaused", p0 = error),
                )?;
            }
            if self.pending_recovery_count()? > 0
                || (batch.stage == SettlementStage::PendingSweep && sweep_started(&batch))
                || (batch.stage != SettlementStage::PendingSweep && !batch.stage.is_terminal())
            {
                break;
            }
        }
        Ok(())
    }

    async fn advance_batch(
        &self,
        client: &BinanceClient,
        batch: &mut SettlementBatch,
    ) -> AppResult<()> {
        let mut orders = self.batch_tasks(batch).await?;
        if orders
            .iter()
            .any(|order| order.status == DeliveryStatus::Failed)
        {
            return self.update_batch(
                batch,
                SettlementStage::Blocked,
                &crate::msg!("backend.terminalRebalancingFailureOrAbnormalFilledQuantity"),
            );
        }
        // 上轮即使在保存最后一笔成交后退出，也能从已保存的任务恢复收尾。
        if orders
            .iter()
            .all(|order| order.status == DeliveryStatus::Completed)
        {
            self.schedule_batch_sweep(batch, &orders)?;
            return self.sweep_settlement_batch(client, batch).await;
        }
        if batch.stage == SettlementStage::Blocked {
            return Ok(());
        }
        if !self.confirm_batch_exercise(client, &mut orders).await? {
            return self.update_batch(
                batch,
                SettlementStage::WaitingExercise,
                &crate::msg!("backend.waitingForAllBinanceExerciseRecordsIn"),
            );
        }
        // 先查询已发送的原单。不能用尚未确认的成交数量重新计算赎回缺口。
        for order in orders.iter().filter(|order| {
            order.status != DeliveryStatus::Completed && !order.spot_client_order_id.is_empty()
        }) {
            let updated = self.rebalance_delivery_task(client, order.clone()).await?;
            if updated.status != DeliveryStatus::Completed {
                return self.update_batch(
                    batch,
                    SettlementStage::ConfirmingOrders,
                    &crate::msg!("backend.waitingForTheOriginalSpotOrderTo"),
                );
            }
        }
        orders = self.batch_tasks(batch).await?;
        batch.required = settlement_required_assets(&orders)?;
        if !self.prepare_batch_funding(client, batch).await? {
            return Ok(());
        }
        self.update_batch(
            batch,
            SettlementStage::Executing,
            &crate::msg!("backend.batchFundingReceivedConfirmingEachSpotRebalance"),
        )?;
        for order in orders
            .into_iter()
            .filter(|order| order.status != DeliveryStatus::Completed)
        {
            if self.pending_recovery_count()? > 0 {
                break;
            }
            let account = client.spot_account().await?;
            let remaining = settlement_required_assets(&self.batch_tasks(batch).await?)?;
            if !spot_has_settlement_assets(&account, &remaining) {
                return self.update_batch(
                    batch,
                    SettlementStage::WaitingFunds,
                    &crate::msg!("backend.actualFillsOrExternalBalanceChangesLeft"),
                );
            }
            let updated = self.rebalance_delivery_task(client, order).await?;
            if updated.status != DeliveryStatus::Completed {
                return self.update_batch(
                    batch,
                    SettlementStage::ConfirmingOrders,
                    &crate::msg!("backend.spotRebalancingIsIncompleteWaitingForThe"),
                );
            }
        }
        let orders = self.batch_tasks(batch).await?;
        if orders
            .iter()
            .all(|order| order.status == DeliveryStatus::Completed)
        {
            self.schedule_batch_sweep(batch, &orders)?;
        }
        Ok(())
    }

    async fn confirm_batch_exercise(
        &self,
        client: &BinanceClient,
        tasks: &mut [DeliveryTask],
    ) -> AppResult<bool> {
        let mut ready = true;
        for task in tasks
            .iter_mut()
            .filter(|task| task.status == DeliveryStatus::WaitingExercise)
        {
            let records = client
                .exercise_records(
                    &task.option_symbol,
                    task.expiry_at.timestamp_millis(),
                    (task.expiry_at + Duration::days(2))
                        .min(Utc::now())
                        .timestamp_millis(),
                )
                .await?;
            if let Some(facts) = exercise_facts(task, &records)? {
                task.quantity = Some(facts.quantity);
                task.settlement_price = Some(facts.price);
                task.cash_settlement = Some(facts.cash);
                task.exercise_record_ids = facts.ids;
                if is_in_the_money(task, facts.price) {
                    task.status = DeliveryStatus::Ready;
                    task.message = crate::msg!("backend.deliveryFactsConfirmed");
                } else {
                    task.status = DeliveryStatus::Completed;
                    task.completed_at = Some(Utc::now());
                    task.message = crate::msg!("backend.binanceExerciseRecordsConfirmOutOfThe");
                }
            } else if let Some(price) = client
                .exercise_history(&task.symbol, task.expiry_at)
                .await?
            {
                if price.underlying != task.symbol || price.expiry_at != task.expiry_at {
                    return Err(AppError::response(crate::msg!(
                        "backend.exerciseFactsInconsistent"
                    )));
                }
                if is_in_the_money(task, price.settlement_price) {
                    // 市场结算价不能证明本账户的最终数量，价内时必须等账户记录。
                    ready = false;
                    continue;
                }
                task.settlement_price = Some(price.settlement_price);
                task.status = DeliveryStatus::Completed;
                task.completed_at = Some(Utc::now());
                task.message = crate::msg!("backend.publicSettlementConfirmsNoDelivery");
            } else {
                ready = false;
                continue;
            }
            task.updated_at = Utc::now();
            self.save_delivery(task).await?;
        }
        Ok(ready)
    }

    fn schedule_batch_sweep(
        &self,
        batch: &mut SettlementBatch,
        orders: &[DeliveryTask],
    ) -> AppResult<()> {
        if batch.sweep_after_at.is_some() {
            return Ok(());
        }
        batch.stage = SettlementStage::PendingSweep;
        batch.required = SettlementAssets::default();
        batch.shortfall = SettlementAssets::default();
        batch.sweep_after_at = Some(Utc::now() + Duration::minutes(SETTLEMENT_SWEEP_DELAY_MINUTES));
        batch.message = crate::msg!("backend.batchSettlementCompletedWaitingFiveMinutesBefore");
        batch.updated_at = Utc::now();
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?
            .save_batch_and_deliveries(batch, orders)
    }

    pub(super) async fn save_delivery_and_attempt(
        &self,
        order: &DeliveryTask,
        attempt: &OrderAttempt,
    ) -> AppResult<()> {
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?
            .save_delivery_and_attempt(order, attempt)?;
        let mut orders = self.deliveries.write().await;
        let stored = orders
            .iter_mut()
            .find(|stored| stored.id == order.id)
            .ok_or_else(|| {
                AppError::Storage(crate::msg!("backend.theDeliveryTaskForThisSpotRebalance"))
            })?;
        *stored = order.clone();
        Ok(())
    }

    async fn rebalance_delivery_task(
        &self,
        client: &BinanceClient,
        mut order: DeliveryTask,
    ) -> AppResult<DeliveryTask> {
        if !order.spot_client_order_id.is_empty() {
            let ack = client
                .query_spot_order(
                    "BTCUSDT",
                    order.spot_order_id.as_deref(),
                    Some(&order.spot_client_order_id),
                )
                .await?;
            let mut attempt = self
                .persistence
                .lock()
                .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?
                .list_order_attempts(usize::MAX)?
                .into_iter()
                .find(|attempt| attempt.client_order_id == order.spot_client_order_id)
                .ok_or_else(|| {
                    AppError::Storage(crate::msg!(
                        "backend.theOriginalSpotRecoveryRecordIsMissing"
                    ))
                })?;
            order.spot_order_id = Some(ack.order_id.clone());
            apply_spot_ack(&mut order, &ack)?;
            attempt.status = OrderAttemptStatus::SpotPersisted;
            attempt.exchange_order_id = Some(ack.order_id);
            attempt.updated_at = Utc::now();
            attempt.message.clone_from(&order.message);
            self.save_delivery_and_attempt(&order, &attempt).await?;
            return Ok(order);
        }
        // 任务只来自到期前已登记的卖方净仓，执行数量必须已经由账户行权记录确认。
        if order.status != DeliveryStatus::Ready
            || order.symbol != "BTCUSDT"
            || order.expiry_at > Utc::now()
            || order.tracked_at >= order.expiry_at
            || order.base_quantity()? <= Decimal::ZERO
            || !is_in_the_money(&order, order.exercise_price()?)
        {
            return Err(AppError::RiskRejected(crate::msg!(
                "backend.onlyRegisteredBtcusdtOrdersThatExpiredIn"
            )));
        }
        if self.pending_recovery_count()? > 0 {
            return Err(AppError::RiskRejected(crate::msg!(
                "backend.unconfirmedRecordsExistSendingTheNextSpot"
            )));
        }
        validate_spot_rebalance(&order, &client.spot_account().await?)?;
        order.spot_client_order_id = make_spot_client_order_id();
        order.status = DeliveryStatus::Executing;
        order.updated_at = Utc::now();
        let mut attempt = OrderAttempt {
            id: Uuid::new_v4().to_string(),
            client_order_id: order.spot_client_order_id.clone(),
            symbol: "BTCUSDT".to_string(),
            option_symbol: order.option_symbol.clone(),
            status: OrderAttemptStatus::SpotAttempted,
            exchange_order_id: None,
            message: crate::msg!("backend.spotRebalancingIntentRecordedLinkingTheStrategy"),
            updated_at: Utc::now(),
        };
        self.save_delivery_and_attempt(&order, &attempt).await?;
        let side = crate::binance::spot_rebalance_side(order.direction);
        match client
            .place_spot_market_order(
                "BTCUSDT",
                side,
                order.base_quantity()?,
                &order.spot_client_order_id,
            )
            .await
        {
            Ok(ack) => {
                attempt.exchange_order_id = Some(ack.order_id.clone());
                order.spot_order_id = Some(ack.order_id.clone());
                apply_spot_ack(&mut order, &ack)?;
                attempt.status = OrderAttemptStatus::SpotPersisted;
                attempt.message.clone_from(&order.message);
            }
            Err(error) => {
                if !error.is_uncertain_submit() {
                    order.status = DeliveryStatus::Failed;
                    attempt.status = OrderAttemptStatus::Failed;
                }
                attempt.message = crate::msg!(
                    "backend.spotRebalancingPausedConfirmTheOriginalRequest",
                    p0 = error
                );
                order.message.clone_from(&attempt.message);
            }
        }
        attempt.updated_at = Utc::now();
        self.save_delivery_and_attempt(&order, &attempt).await?;
        self.record_log(
            "settlement.spot",
            &crate::msg!("ui.join", p0 = order.option_symbol, p1 = order.message),
        );
        Ok(order)
    }

    async fn sweep_settlement_batch(
        &self,
        client: &BinanceClient,
        batch: &mut SettlementBatch,
    ) -> AppResult<()> {
        if batch.stage == SettlementStage::Blocked
            || batch.stage.is_terminal()
            || batch.sweep_after_at.is_none_or(|at| at > Utc::now())
        {
            return Ok(());
        }
        // 已登记的收尾资金链必须先完成，后续批次不能消耗其固定申购金额。
        if !sweep_started(batch)
            && self
                .deliveries
                .read()
                .await
                .iter()
                .any(|order| needs_settlement(order) && order.expiry_at <= Utc::now())
        {
            return self.update_batch(
                batch,
                SettlementStage::PendingSweep,
                &crate::msg!("backend.otherExpiringOrdersAreUnfinishedRetainingFunds"),
            );
        }
        if self.pending_recovery_count()? > 0 {
            return Ok(());
        }
        if !self.confirm_batch_option_return(client, batch).await? {
            return Ok(());
        }
        if batch.spot_sweep.is_none() {
            batch.spot_sweep = Some(spot_assets(&client.spot_account().await?));
            self.save_batch(batch)?;
        }
        let amounts = batch
            .spot_sweep
            .clone()
            .ok_or_else(|| AppError::Storage(crate::msg!("backend.fundSweepPlanIsMissing")))?;
        for (asset, amount) in [("USDT", amounts.usdt), ("BTC", amounts.btc)] {
            if self.pending_recovery_count()? > 0 {
                return Ok(());
            }
            self.subscribe_spot_asset_to_earn(
                client,
                asset,
                amount,
                &crate::msg!("backend.fundSweepAfterBatchSpotRebalancing"),
                &format!("{}-subscribe-{asset}", batch.id),
            )
            .await?;
        }
        self.update_batch(
            batch,
            SettlementStage::Completed,
            &crate::msg!("backend.allBatchFillsAndFundSweepsAre"),
        )
    }

    async fn confirm_batch_option_return(
        &self,
        client: &BinanceClient,
        batch: &mut SettlementBatch,
    ) -> AppResult<bool> {
        if batch.spot_sweep.is_some() {
            return Ok(true);
        }
        if batch.option_sweep_usdt.is_none() {
            let account = client.option_account().await?;
            let amount = round_down_funding_amount(option_idle_usdt_sweep_amount(&account));
            batch.option_sweep_usdt = Some(if amount >= OPTION_IDLE_USDT_SWEEP_MIN_AMOUNT {
                amount
            } else {
                Decimal::ZERO
            });
            batch.spot_usdt_before_transfer =
                spot_asset_free(&client.spot_account().await?, "USDT");
            self.save_batch(batch)?;
        }
        let amount = batch.option_sweep_usdt.unwrap_or(Decimal::ZERO);
        if amount <= Decimal::ZERO {
            return Ok(true);
        }
        let chain_id = format!("{}-option-return", batch.id);
        // 尚未发送时重新核对保证金，已发送时只确认原划转及到账。
        if self.funding_attempts_by_chain(&chain_id)?.is_empty() {
            let account = client.option_account().await?;
            if option_idle_usdt_sweep_amount(&account) < amount {
                batch.option_sweep_usdt = None;
                self.update_batch(
                    batch,
                    SettlementStage::PendingSweep,
                    &crate::msg!("backend.marginRequirementsChangedReassessingTheAmountAvailable"),
                )?;
                return Ok(false);
            }
        }
        self.execute_universal_transfer(
            client,
            &chain_id,
            &crate::msg!("backend.fundSweepAfterBatchSettlement"),
            "OPTION_MAIN",
            "USDT",
            amount,
        )
        .await?;
        if !self
            .wait_for_spot_asset_available(client, "USDT", batch.spot_usdt_before_transfer + amount)
            .await?
        {
            self.update_batch(
                batch,
                SettlementStage::PendingSweep,
                &crate::msg!("backend.optionsTransferConfirmedWaitingForSpotFunds"),
            )?;
            return Ok(false);
        }
        Ok(true)
    }
}
