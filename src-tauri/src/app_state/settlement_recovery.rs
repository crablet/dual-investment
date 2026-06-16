//! 人工结束失败的理财收尾。只关闭批次，不改写成交或资金结果，也不发送交易所请求。

use crate::messages::Message;

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
) -> Option<Message> {
    if batch.stage != SettlementStage::Blocked {
        return Some(crate::msg!("backend.onlyBlockedEarnFinalizationCanBeEnded"));
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
        return Some(crate::msg!("backend.notAllBatchOrdersAreSettledCheck"));
    }
    if has_pending_recovery {
        return Some(crate::msg!(
            "backend.orderOrFundingRequestsAwaitConfirmationComplete"
        ));
    }
    if batch.sweep_after_at.is_none()
        || batch.spot_sweep.is_none()
        || batch.redemption.is_some()
        || batch.option_funding.is_some()
        || batch.required.usdt != Decimal::ZERO
        || batch.required.btc != Decimal::ZERO
        || batch.shortfall.usdt != Decimal::ZERO
        || batch.shortfall.btc != Decimal::ZERO
    {
        return Some(crate::msg!(
            "backend.fundingPreparationTransfersAndSpotReceiptAre"
        ));
    }
    let Some(option_sweep_usdt) = batch.option_sweep_usdt else {
        return Some(crate::msg!(
            "backend.optionsTransferPlanIsMissingCheckFunding"
        ));
    };
    if option_sweep_usdt > Decimal::ZERO
        && !funding.iter().any(|attempt| {
            attempt.chain_id == format!("{}-option-return", batch.id)
                && attempt.kind == FundingAttemptKind::UniversalTransfer
                && attempt.asset == "USDT"
                && attempt.amount == option_sweep_usdt
                && attempt.transfer_type.as_deref() == Some("OPTION_MAIN")
                && attempt.status == FundingAttemptStatus::Confirmed
        })
    {
        return Some(crate::msg!(
            "backend.optionsTransferSuccessIsUnconfirmedCheckFunding"
        ));
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
        return Some(crate::msg!(
            "backend.thisBatchHasUnconfirmedOrFailedRedemptions"
        ));
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
            crate::msg!("backend.endedManually")
        } else {
            crate::msg!("backend.batchBlocked")
        },
        can_finish: blocker.is_none(),
        fact_source: crate::msg!("backend.localSettlementOrderAndFundingRecoveryRecords"),
        next_action: if skipped {
            crate::msg!("backend.earnFinalizationEndedForThisBatchUnsubscribed")
        } else {
            blocker.unwrap_or(crate::msg!(
                "backend.youMayEndEarnFinalizationUnsubscribedFunds"
            ))
        },
        message: batch.message.clone(),
        updated_at: batch.updated_at,
    }
}

impl AppState {
    pub(super) fn settlement_recoveries(&self) -> AppResult<Vec<SettlementRecoveryView>> {
        let persistence = self
            .persistence
            .lock()
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
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
            return Err(AppError::RiskRejected(crate::msg!(
                "backend.endingEarnFinalizationRequiresExplicitUserConfirmation"
            )));
        }
        // 在交易锁内重读数据库，按钮显示时的快照不能作为放行依据。
        let persistence = self
            .persistence
            .lock()
            .map_err(|error| AppError::Storage(Message::diagnostic(error.to_string())))?;
        let mut batch = persistence
            .list_settlement_batches()?
            .into_iter()
            .find(|batch| batch.id == request.batch_id)
            .ok_or_else(|| AppError::Storage(crate::msg!("backend.settlementBatchNotFound")))?;
        let orders = persistence.list_orders()?;
        let funding = persistence.list_funding_attempts(usize::MAX)?;
        let has_pending = persistence.count_pending_order_attempts()?
            + persistence.count_pending_funding_attempts()?
            > 0;
        if batch.stage != SettlementStage::SweepSkipped {
            if let Some(reason) = finish_blocker(&batch, &orders, &funding, has_pending) {
                return Err(AppError::RiskRejected(reason));
            }
            batch.stage = SettlementStage::SweepSkipped;
            batch.message = crate::msg!(
                "backend.userConfirmedEndingEarnFinalizationUnsubscribedFunds",
                p0 = batch.message
            );
            batch.updated_at = Utc::now();
            persistence.save_settlement_batch(&batch)?;
            self.record_log(
                "settlement.sweep.skipped",
                &crate::msg!(
                    "backend.userConfirmedEndingEarnFinalizationOriginalFunding",
                    p0 = batch.id
                ),
            );
        }
        Ok(recovery_view(&batch, &orders, &funding, has_pending))
    }
}
