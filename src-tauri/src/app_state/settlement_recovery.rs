//! 人工结束失败的理财收尾。只关闭批次，不改写成交或资金结果，也不发送交易所请求。

use super::{
    AppError, AppResult, AppState, Decimal, FundingAttempt, FundingAttemptKind,
    FundingAttemptStatus, SettlementBatch, SettlementStage, StrategyOrder, StrategyStatus, Utc,
};
use crate::domain::{FinishSettlementSweepRequest, SettlementRecoveryView};

/// `spot_sweep` 只在期权划转确认且现货到账后落库，是进入理财收尾的事实边界。
fn finish_blocker(
    batch: &SettlementBatch,
    orders: &[StrategyOrder],
    funding: &[FundingAttempt],
    has_pending_recovery: bool,
) -> Option<&'static str> {
    if batch.stage != SettlementStage::Blocked {
        return Some("只有已阻断的理财收尾可以人工结束");
    }
    if batch.order_ids.is_empty()
        || batch.order_ids.iter().any(|id| {
            !orders.iter().any(|order| {
                order.id == *id
                    && order.symbol == batch.symbol
                    && order.expiry_at == batch.expiry_at
                    && order.status == StrategyStatus::Settled
            })
        })
        || orders.iter().any(|order| {
            order.symbol == batch.symbol
                && order.expiry_at == batch.expiry_at
                && order.filled_quantity > Decimal::ZERO
                && order.status != StrategyStatus::Settled
        })
    {
        return Some("同批订单尚未全部结算完成，请先核对订单");
    }
    if has_pending_recovery {
        return Some("存在待确认的订单或资金请求，请先完成恢复核对");
    }
    if batch.sweep_after_at.is_none()
        || batch.spot_sweep.is_none()
        || batch.redemption.is_some()
        || batch.required.usdt != Decimal::ZERO
        || batch.required.btc != Decimal::ZERO
        || batch.shortfall.usdt != Decimal::ZERO
        || batch.shortfall.btc != Decimal::ZERO
    {
        return Some("尚未确认资金准备、划转及现货到账，不能跳过收尾");
    }
    let Some(option_sweep) = &batch.option_sweep else {
        return Some("缺少期权划转计划，请先核对资金记录");
    };
    if option_sweep.usdt > Decimal::ZERO
        && !funding.iter().any(|attempt| {
            attempt.chain_id == format!("{}-option-return", batch.id)
                && attempt.kind == FundingAttemptKind::UniversalTransfer
                && attempt.asset == "USDT"
                && attempt.amount == option_sweep.usdt
                && attempt.transfer_type.as_deref() == Some("OPTION_MAIN")
                && attempt.status == FundingAttemptStatus::Confirmed
        })
    {
        return Some("期权划转尚未确认成功，请先核对资金记录");
    }
    let prefix = format!("{}-", batch.id);
    if funding
        .iter()
        .filter(|attempt| attempt.chain_id.starts_with(&prefix))
        .any(|attempt| {
            !attempt.status.is_terminal()
                || (attempt.kind != FundingAttemptKind::SimpleEarnSubscribe
                    && attempt.status != FundingAttemptStatus::Confirmed)
        })
    {
        return Some("该批次仍有未确认或失败的赎回、划转，不能结束理财收尾");
    }
    None
}

fn recovery_view(
    batch: &SettlementBatch,
    orders: &[StrategyOrder],
    funding: &[FundingAttempt],
    has_pending_recovery: bool,
) -> SettlementRecoveryView {
    let skipped = batch.stage == SettlementStage::SweepSkipped;
    let blocker = finish_blocker(batch, orders, funding, has_pending_recovery);
    SettlementRecoveryView {
        id: batch.id.clone(),
        symbol: batch.symbol.clone(),
        expiry_at: batch.expiry_at,
        status: batch.stage,
        status_label: if skipped {
            "已人工结束"
        } else {
            "批次阻断"
        }
        .to_string(),
        can_finish: blocker.is_none(),
        fact_source: "本地结算、订单与资金恢复记录".to_string(),
        next_action: if skipped {
            "本批理财收尾已结束，未申购资金留在现货"
        } else {
            blocker.unwrap_or("可选择结束理财收尾，未申购资金留在现货")
        }
        .to_string(),
        message: batch.message.clone(),
        updated_at: batch.updated_at,
    }
}

impl AppState {
    pub(super) fn settlement_recoveries(&self) -> AppResult<Vec<SettlementRecoveryView>> {
        let persistence = self
            .persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let orders = persistence.list_orders()?;
        let funding = persistence.list_funding_attempts(usize::MAX)?;
        let has_pending = persistence.count_pending_order_attempts()?
            + persistence.count_pending_funding_attempts()?
            > 0;
        Ok(persistence
            .list_settlement_batches()?
            .iter()
            .filter(|batch| {
                matches!(
                    batch.stage,
                    SettlementStage::Blocked | SettlementStage::SweepSkipped
                )
            })
            .map(|batch| recovery_view(batch, &orders, &funding, has_pending))
            .collect())
    }

    pub async fn finish_settlement_sweep(
        &self,
        request: &FinishSettlementSweepRequest,
    ) -> AppResult<SettlementRecoveryView> {
        let _trade_guard = self.trade_lock.lock().await;
        if !request.confirm_manual {
            return Err(AppError::RiskRejected(
                "结束理财收尾需要用户明确确认".to_string(),
            ));
        }
        // 在交易锁内重读数据库，按钮显示时的快照不能作为放行依据。
        let persistence = self
            .persistence
            .lock()
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let mut batch = persistence
            .list_settlement_batches()?
            .into_iter()
            .find(|batch| batch.id == request.batch_id)
            .ok_or_else(|| AppError::Storage("未找到指定结算批次".to_string()))?;
        let orders = persistence.list_orders()?;
        let funding = persistence.list_funding_attempts(usize::MAX)?;
        let has_pending = persistence.count_pending_order_attempts()?
            + persistence.count_pending_funding_attempts()?
            > 0;
        if batch.stage != SettlementStage::SweepSkipped {
            if let Some(reason) = finish_blocker(&batch, &orders, &funding, has_pending) {
                return Err(AppError::RiskRejected(reason.to_string()));
            }
            batch.stage = SettlementStage::SweepSkipped;
            batch.message = format!(
                "用户确认结束理财收尾，未申购资金留在现货。原暂停原因：{}",
                batch.message
            );
            batch.updated_at = Utc::now();
            persistence.save_settlement_batch(&batch)?;
            self.record_log(
                "settlement.sweep.skipped",
                &format!("{} 用户确认结束理财收尾，保留原资金结果", batch.id),
            )?;
        }
        Ok(recovery_view(&batch, &orders, &funding, has_pending))
    }
}
