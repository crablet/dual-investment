//! 到期批次编排。批次先备齐资金，逐笔确认成交，最后执行持久化收尾计划。
//! 等待期间释放运行锁；下一轮必须重新读取记录，已发送的步骤只查询原请求。

use std::collections::BTreeMap;

use super::funding::{settlement_required_assets, spot_has_settlement_assets};
use super::{
    AppError, AppResult, AppState, BinanceClient, Decimal, Duration,
    OPTION_IDLE_USDT_SWEEP_MIN_AMOUNT, OrderAttempt, OrderAttemptStatus,
    SETTLEMENT_SWEEP_DELAY_MINUTES, SettlementBatch, SettlementStage, SpotAccountSnapshot,
    StrategyOrder, StrategyStatus, Utc, Uuid, apply_spot_ack, is_in_the_money,
    is_transient_binance_error, is_uncertain_submit_error, make_spot_client_order_id,
    option_idle_usdt_sweep_amount, round_down_funding_amount, spot_asset_free,
    validate_spot_rebalance,
};
use crate::domain::SettlementAssets;

fn needs_settlement(order: &StrategyOrder) -> bool {
    order.filled_quantity > Decimal::ZERO
        && matches!(
            order.status,
            StrategyStatus::Running
                | StrategyStatus::PartiallyFilled
                | StrategyStatus::Pending
                | StrategyStatus::RebalanceFailed
        )
}

fn spot_assets(spot: &SpotAccountSnapshot) -> SettlementAssets {
    SettlementAssets {
        usdt: spot_asset_free(spot, "USDT"),
        btc: spot_asset_free(spot, "BTC"),
    }
}

fn sweep_started(batch: &SettlementBatch) -> bool {
    batch.option_sweep.is_some() || batch.spot_sweep.is_some()
}

impl AppState {
    pub(super) fn settlement_batches(&self) -> AppResult<Vec<SettlementBatch>> {
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .list_settlement_batches()
    }

    pub(super) fn save_batch(&self, batch: &mut SettlementBatch) -> AppResult<()> {
        batch.updated_at = Utc::now();
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .save_settlement_batch(batch)
    }

    pub(super) fn update_batch(
        &self,
        batch: &mut SettlementBatch,
        stage: SettlementStage,
        message: &str,
    ) -> AppResult<()> {
        let changed = batch.stage != stage || batch.message != message;
        batch.stage = stage;
        batch.message = message.to_string();
        self.save_batch(batch)?;
        if changed {
            self.record_log("settlement.batch", &format!("{} {}", batch.id, message))?;
        }
        Ok(())
    }

    /// 已到期的义务和待完成批次都会预留资产，包括等待人工处理的批次。
    pub(super) async fn settlement_reserves_assets(&self) -> AppResult<bool> {
        let now = Utc::now();
        Ok(self
            .orders
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
        let orders = self.orders.read().await.clone();
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
                if ids.iter().any(|id| !batch.order_ids.contains(id)) {
                    let mut batch = batch.clone();
                    if batch.stage == SettlementStage::WaitingExercise {
                        for id in ids {
                            if !batch.order_ids.contains(&id) {
                                batch.order_ids.push(id);
                            }
                        }
                        batch.order_ids.sort();
                        self.save_batch(&mut batch)?;
                    } else {
                        self.update_batch(
                            &mut batch,
                            SettlementStage::Blocked,
                            "批次开始后出现未登记订单，需要人工核对成员",
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
                order_ids: ids,
                stage: SettlementStage::WaitingExercise,
                message: "等待到期及 Binance 行权记录".to_string(),
                required: SettlementAssets::default(),
                shortfall: SettlementAssets::default(),
                redemption: None,
                option_funding: None,
                sweep_after_at: None,
                option_sweep: None,
                spot_before_transfer: SettlementAssets::default(),
                spot_sweep: None,
                created_at: now,
                updated_at: now,
            })?;
        }
        Ok(())
    }

    async fn batch_orders(&self, batch: &SettlementBatch) -> AppResult<Vec<StrategyOrder>> {
        let orders = self.orders.read().await;
        batch
            .order_ids
            .iter()
            .map(|id| {
                orders
                    .iter()
                    .find(|order| &order.id == id)
                    .cloned()
                    .ok_or_else(|| AppError::Storage("结算批次成员订单缺失，停止执行".to_string()))
            })
            .collect()
    }

    pub(super) async fn recover_pending_settlement_orders(&self) -> AppResult<()> {
        let _trade_guard = self.trade_lock.lock().await;
        let attempts = self
            .persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .list_order_attempts(usize::MAX)?;
        for attempt in attempts.into_iter().filter(|attempt| {
            matches!(
                attempt.status,
                OrderAttemptStatus::SpotAttempted
                    | OrderAttemptStatus::SpotExchangeAck
                    | OrderAttemptStatus::SpotPersistFailed
            )
        }) {
            if let Err(error) = self.recover_spot_attempt(attempt).await {
                self.record_warning(
                    "settlement.order.confirming",
                    &format!("现货补齐等待原订单确认：{error}"),
                )?;
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
                    "存在待确认恢复记录，等待确认后再继续资金动作",
                )?;
                break;
            }
            let result = self.advance_batch(&client, &mut batch).await;
            if let Err(error) = result {
                let stage =
                    if is_transient_binance_error(&error) || self.pending_recovery_count()? > 0 {
                        batch.stage
                    } else {
                        SettlementStage::Blocked
                    };
                self.update_batch(&mut batch, stage, &format!("结算暂停：{error}"))?;
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
        let mut orders = self.batch_orders(batch).await?;
        if orders
            .iter()
            .any(|order| order.status == StrategyStatus::RebalanceFailed)
        {
            return self.update_batch(
                batch,
                SettlementStage::Blocked,
                "存在终态补齐失败或异常成交数量，请先核对原订单",
            );
        }
        // 即使上轮在保存最后一笔成交后退出，也能从持久化成员事实恢复收尾。
        if orders
            .iter()
            .all(|order| order.status == StrategyStatus::Settled)
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
                "等待同批 Binance 行权记录齐全",
            );
        }
        // 先查询已发送的原单。不能用尚未确认的成交数量重新计算赎回缺口。
        for order in orders.iter().filter(|order| {
            order.status != StrategyStatus::Settled && !order.spot_client_order_id.is_empty()
        }) {
            let updated = self
                .rebalance_settlement_order(client, order.clone())
                .await?;
            if updated.status != StrategyStatus::Settled {
                return self.update_batch(
                    batch,
                    SettlementStage::ConfirmingOrders,
                    "等待原现货订单足额成交，不发送下一笔",
                );
            }
        }
        orders = self.batch_orders(batch).await?;
        batch.required = settlement_required_assets(&orders);
        if !self.prepare_batch_funding(client, batch).await? {
            return Ok(());
        }
        self.update_batch(
            batch,
            SettlementStage::Executing,
            "整批资金已到账，逐笔确认现货补齐",
        )?;
        for order in orders
            .into_iter()
            .filter(|order| order.status != StrategyStatus::Settled)
        {
            if self.pending_recovery_count()? > 0 {
                break;
            }
            let account = client.spot_account().await?;
            let remaining = settlement_required_assets(&self.batch_orders(batch).await?);
            if !spot_has_settlement_assets(&account, &remaining) {
                return self.update_batch(
                    batch,
                    SettlementStage::WaitingFunds,
                    "实际成交或外部资金变化导致预留不足，等待补足剩余接货资金",
                );
            }
            let updated = self.rebalance_settlement_order(client, order).await?;
            if updated.status != StrategyStatus::Settled {
                return self.update_batch(
                    batch,
                    SettlementStage::ConfirmingOrders,
                    "现货补齐尚未完成，等待原单确认后再继续",
                );
            }
        }
        let orders = self.batch_orders(batch).await?;
        if orders
            .iter()
            .all(|order| order.status == StrategyStatus::Settled)
        {
            self.schedule_batch_sweep(batch, &orders)?;
        }
        Ok(())
    }

    async fn confirm_batch_exercise(
        &self,
        client: &BinanceClient,
        orders: &mut [StrategyOrder],
    ) -> AppResult<bool> {
        let mut ready = true;
        for order in orders
            .iter_mut()
            .filter(|order| order.status != StrategyStatus::Settled)
        {
            if order.settlement_price <= Decimal::ZERO {
                let Some(price) = self.settlement_price(client, order).await? else {
                    ready = false;
                    continue;
                };
                order.settlement_price = price;
                order.updated_at = Utc::now();
                if !is_in_the_money(order, price) {
                    order.status = StrategyStatus::Settled;
                    order.settled_at = Some(Utc::now());
                    order.rebalance_message = "Binance 行权记录确认价外，无需现货补齐".to_string();
                }
                self.save_order(order).await?;
            }
        }
        Ok(ready)
    }

    fn schedule_batch_sweep(
        &self,
        batch: &mut SettlementBatch,
        orders: &[StrategyOrder],
    ) -> AppResult<()> {
        if batch.sweep_after_at.is_some() {
            return Ok(());
        }
        batch.stage = SettlementStage::PendingSweep;
        batch.required = SettlementAssets::default();
        batch.shortfall = SettlementAssets::default();
        batch.sweep_after_at = Some(Utc::now() + Duration::minutes(SETTLEMENT_SWEEP_DELAY_MINUTES));
        batch.message = "整批结算完成，等待五分钟后统一归集".to_string();
        batch.updated_at = Utc::now();
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .save_batch_and_orders(batch, orders)
    }

    async fn settlement_price(
        &self,
        client: &BinanceClient,
        order: &StrategyOrder,
    ) -> AppResult<Option<Decimal>> {
        let records = client
            .exercise_records(
                &order.option_symbol,
                (order.expiry_at - Duration::hours(1)).timestamp_millis(),
                (order.expiry_at + Duration::days(2)).timestamp_millis(),
            )
            .await?;
        let mut prices = records
            .into_iter()
            .filter(|record| {
                record.symbol == order.option_symbol && record.settlement_price > Decimal::ZERO
            })
            .map(|record| record.settlement_price);
        let first = prices.next();
        if prices.any(|price| Some(price) != first) {
            return Err(AppError::Binance(
                "行权记录结算价不一致，需要人工核对".to_string(),
            ));
        }
        Ok(first)
    }

    pub(super) async fn save_settlement_order(
        &self,
        order: &StrategyOrder,
        attempt: &OrderAttempt,
    ) -> AppResult<()> {
        self.persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?
            .save_order_and_attempt(order, attempt)?;
        let mut orders = self.orders.write().await;
        let stored = orders
            .iter_mut()
            .find(|stored| stored.id == order.id)
            .ok_or_else(|| AppError::Storage("现货补齐对应策略订单不存在".to_string()))?;
        *stored = order.clone();
        Ok(())
    }

    async fn rebalance_settlement_order(
        &self,
        client: &BinanceClient,
        mut order: StrategyOrder,
    ) -> AppResult<StrategyOrder> {
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
                .map_err(|error| AppError::Storage(error.to_string()))?
                .list_order_attempts(usize::MAX)?
                .into_iter()
                .find(|attempt| attempt.client_order_id == order.spot_client_order_id)
                .ok_or_else(|| AppError::Storage("原现货补齐恢复记录缺失，不能重发".to_string()))?;
            order.spot_order_id = Some(ack.order_id.clone());
            apply_spot_ack(&mut order, &ack);
            attempt.status = OrderAttemptStatus::SpotPersisted;
            attempt.exchange_order_id = Some(ack.order_id);
            attempt.updated_at = Utc::now();
            attempt.message.clone_from(&order.rebalance_message);
            self.save_settlement_order(&order, &attempt).await?;
            return Ok(order);
        }
        // 下单或接管时已确认后续到期处理；这里只能补齐已有的到期价内订单。
        if !needs_settlement(&order)
            || order.symbol != "BTCUSDT"
            || order.expiry_at > Utc::now()
            || order.settlement_price <= Decimal::ZERO
            || !is_in_the_money(&order, order.settlement_price)
        {
            return Err(AppError::RiskRejected(
                "只有已登记、到期且价内的 BTCUSDT 订单可以现货补齐".to_string(),
            ));
        }
        if self.pending_recovery_count()? > 0 {
            return Err(AppError::RiskRejected(
                "存在待确认记录，禁止发送下一笔现货订单".to_string(),
            ));
        }
        validate_spot_rebalance(&order, &client.spot_account().await?)?;
        order.spot_client_order_id = make_spot_client_order_id(order.direction);
        order.updated_at = Utc::now();
        let mut attempt = OrderAttempt {
            id: Uuid::new_v4().to_string(),
            client_order_id: order.spot_client_order_id.clone(),
            symbol: "BTCUSDT".to_string(),
            option_symbol: order.option_symbol.clone(),
            direction: order.direction,
            status: OrderAttemptStatus::SpotAttempted,
            exchange_order_id: None,
            message: "现货补齐意图已登记，发送前关联策略订单".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.save_settlement_order(&order, &attempt).await?;
        let side = crate::binance::spot_rebalance_side(order.direction);
        match client
            .place_spot_market_order(
                "BTCUSDT",
                side,
                order.filled_quantity,
                &order.spot_client_order_id,
            )
            .await
        {
            Ok(ack) => {
                attempt.exchange_order_id = Some(ack.order_id.clone());
                order.spot_order_id = Some(ack.order_id.clone());
                apply_spot_ack(&mut order, &ack);
                attempt.status = OrderAttemptStatus::SpotPersisted;
                attempt.message.clone_from(&order.rebalance_message);
            }
            Err(error) => {
                if !is_uncertain_submit_error(&error) {
                    order.status = StrategyStatus::RebalanceFailed;
                    attempt.status = OrderAttemptStatus::Failed;
                }
                attempt.message = format!("现货补齐暂停，需确认原请求：{error}");
                order.rebalance_message.clone_from(&attempt.message);
            }
        }
        attempt.updated_at = Utc::now();
        self.save_settlement_order(&order, &attempt).await?;
        self.record_log(
            "settlement.spot",
            &format!("{} {}", order.option_symbol, order.rebalance_message),
        )?;
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
                .orders
                .read()
                .await
                .iter()
                .any(|order| needs_settlement(order) && order.expiry_at <= Utc::now())
        {
            return self.update_batch(
                batch,
                SettlementStage::PendingSweep,
                "其它到期订单尚未完成，继续保留资金，延后归集",
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
            .ok_or_else(|| AppError::Storage("归集计划缺失".to_string()))?;
        for (asset, amount) in [("USDT", amounts.usdt), ("BTC", amounts.btc)] {
            if self.pending_recovery_count()? > 0 {
                return Ok(());
            }
            self.subscribe_spot_asset_to_earn(
                client,
                asset,
                amount,
                "整批现货补齐后归集",
                &format!("{}-subscribe-{asset}", batch.id),
            )
            .await?;
        }
        self.update_batch(
            batch,
            SettlementStage::Completed,
            "整批成交与资金归集均已完成",
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
        if batch.option_sweep.is_none() {
            let account = client.option_account().await?;
            let amount = round_down_funding_amount(option_idle_usdt_sweep_amount(&account));
            batch.option_sweep = Some(SettlementAssets {
                usdt: if amount >= OPTION_IDLE_USDT_SWEEP_MIN_AMOUNT {
                    amount
                } else {
                    Decimal::ZERO
                },
                btc: Decimal::ZERO,
            });
            batch.spot_before_transfer = spot_assets(&client.spot_account().await?);
            self.save_batch(batch)?;
        }
        let amount = batch
            .option_sweep
            .as_ref()
            .map_or(Decimal::ZERO, |assets| assets.usdt);
        if amount <= Decimal::ZERO {
            return Ok(true);
        }
        let chain_id = format!("{}-option-return", batch.id);
        // 尚未发送时重新核对保证金，已发送时只确认原划转及到账。
        if self.funding_attempts_by_chain(&chain_id)?.is_empty() {
            let account = client.option_account().await?;
            if option_idle_usdt_sweep_amount(&account) < amount {
                batch.option_sweep = None;
                self.update_batch(
                    batch,
                    SettlementStage::PendingSweep,
                    "保证金需求变化，重新评估可归集金额",
                )?;
                return Ok(false);
            }
        }
        self.execute_universal_transfer(
            client,
            &chain_id,
            "整批结算后归集",
            "OPTION_MAIN",
            "USDT",
            amount,
        )
        .await?;
        if !self
            .wait_for_spot_asset_available(client, "USDT", batch.spot_before_transfer.usdt + amount)
            .await?
        {
            self.update_batch(
                batch,
                SettlementStage::PendingSweep,
                "期权划转已确认，等待现货到账后统一申购",
            )?;
            return Ok(false);
        }
        Ok(true)
    }
}
