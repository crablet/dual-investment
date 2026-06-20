//! 根据交易所回报更新本地订单与展示状态；恢复时查询原订单，不重新下单。

use std::collections::HashMap;

use chrono::Utc;
use rust_decimal::Decimal;

use crate::domain::{
    AppError, AppResult, BalanceRow, DeliveryStatus, DeliveryTask, ExchangeOrderAck,
    OptionAccountSnapshot, OptionContract, OrderAttempt, OrderAttemptStatus, SettlementBatch,
    SimpleEarnSnapshot, SpotAccountSnapshot, StrategyDirection, StrategyOrder, StrategyRequest,
};
use crate::strategy;

/// 批次保留在后端，等待和收尾说明复用订单详情中的补齐结果。
pub(super) fn apply_settlement_progress(orders: &mut [DeliveryTask], batches: &[SettlementBatch]) {
    for batch in batches.iter().filter(|batch| !batch.stage.is_terminal()) {
        for order in orders
            .iter_mut()
            .filter(|order| batch.task_ids.contains(&order.id))
        {
            if order.message.is_empty() {
                order.message.clone_from(&batch.message);
            } else {
                order.message =
                    crate::msg!("ui.joinSentence", p0 = order.message, p1 = batch.message);
            }
        }
    }
}

/// 从期权恢复记录和交易所回报重建下单请求。
pub(super) fn request_from_option_attempt(
    attempt: &OrderAttempt,
    ack: &ExchangeOrderAck,
    contract: &OptionContract,
) -> AppResult<StrategyRequest> {
    let direction = match contract.side.as_str() {
        "PUT" => StrategyDirection::SellPut,
        "CALL" => StrategyDirection::SellCall,
        side => {
            return Err(AppError::Storage(crate::msg!(
                "backend.invalidBinanceContractSideInRecoveryRecord",
                p0 = side
            )));
        }
    };
    let quantity = if ack.quantity > Decimal::ZERO {
        ack.quantity
    } else {
        ack.executed_qty
    };
    let limit_price = if ack.price > Decimal::ZERO {
        ack.price
    } else {
        ack.avg_price
    };
    Ok(StrategyRequest {
        client_flow_id: String::new(),
        symbol: attempt.symbol.clone(),
        option_symbol: attempt.option_symbol.clone(),
        direction,
        expiry_at: contract.expiry_at,
        strike: contract.strike_price,
        quantity,
        limit_price,
        confirm_live: true,
    })
}

/// 把期权订单回报应用到本地订单。
pub(super) fn apply_option_ack(order: &mut StrategyOrder, ack: &ExchangeOrderAck) {
    let requested_qty = if ack.quantity > Decimal::ZERO {
        ack.quantity
    } else {
        order.quantity
    };
    order.status = strategy::status_from_exchange(&ack.status, ack.executed_qty, requested_qty);
    order.exchange_status.clone_from(&ack.status);
    order.filled_quantity = ack.executed_qty;
    if ack.avg_price > Decimal::ZERO {
        order.avg_price = ack.avg_price;
    }
    strategy::refresh_order_metrics(order);
    order.updated_at = ack.update_time.or(ack.create_time).unwrap_or_else(Utc::now);
}

pub(super) fn is_unfilled_terminal_option_ack(ack: &ExchangeOrderAck) -> bool {
    ack.executed_qty <= Decimal::ZERO
        && matches!(
            ack.status.as_str(),
            "EXPIRED" | "REJECTED" | "CANCELED" | "CANCELLED"
        )
}

/// 把现货补齐订单回报应用到本地订单。
///
/// 活跃现货单继续查询；只有终态成交不足或异常数量才进入失败状态。
pub(super) fn apply_spot_ack(order: &mut DeliveryTask, ack: &ExchangeOrderAck) -> AppResult<()> {
    order.spot_order_status.clone_from(&ack.status);
    order.spot_filled_quantity = ack.executed_qty;
    order.spot_avg_price = ack.avg_price;
    order.updated_at = ack.update_time.or(ack.create_time).unwrap_or_else(Utc::now);
    let expected = order.base_quantity()?;
    if ack.status == "FILLED" && ack.executed_qty == expected {
        order.status = DeliveryStatus::Completed;
        order.completed_at = Some(Utc::now());
        order.message = crate::msg!(
            "backend.spotRebalancingFullyFilledQuantityAverageStatus",
            p0 = ack.executed_qty,
            p1 = ack.avg_price,
            p2 = ack.status
        );
    } else if matches!(
        ack.status.as_str(),
        "NEW" | "PARTIALLY_FILLED" | "PENDING_NEW"
    ) && ack.executed_qty < expected
    {
        order.status = DeliveryStatus::Executing;
        order.message = crate::msg!(
            "backend.spotOrderIsStillFillingAwaitingThe",
            p0 = ack.executed_qty,
            p1 = expected
        );
    } else if matches!(
        ack.status.as_str(),
        "REJECTED" | "EXPIRED" | "CANCELED" | "CANCELLED"
    ) {
        order.status = DeliveryStatus::Failed;
        order.message = crate::msg!(
            "backend.spotRebalancingIncompleteFilledStatus",
            p0 = ack.executed_qty,
            p1 = expected,
            p2 = ack.status
        );
    } else {
        order.status = DeliveryStatus::Failed;
        order.message = crate::msg!(
            "backend.spotRebalancingNotFullyFilledStatus",
            p0 = ack.executed_qty,
            p1 = expected,
            p2 = ack.status
        );
    }
    Ok(())
}

pub(super) fn is_spot_attempt(status: OrderAttemptStatus) -> bool {
    matches!(
        status,
        OrderAttemptStatus::SpotAttempted | OrderAttemptStatus::SpotPersisted
    )
}

/// 合并期权保证金、现货和 Simple Earn 资产展示行。
pub(super) fn combined_balances(
    option: &OptionAccountSnapshot,
    spot: &SpotAccountSnapshot,
    earn: &SimpleEarnSnapshot,
    spot_prices: &HashMap<String, Decimal>,
) -> Vec<BalanceRow> {
    let mut balances = strategy::build_balances(option, spot_prices);
    balances.extend(strategy::build_spot_balances(spot, earn, spot_prices));
    balances.extend(strategy::build_simple_earn_balances(earn, spot_prices));
    balances
}
