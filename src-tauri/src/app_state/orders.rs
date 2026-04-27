//! 订单恢复、同步和展示快照辅助逻辑。
//!
//! 这些函数把交易所回报或当前持仓应用到本地订单字段。真实下单/补齐失败后的恢复必须
//! 先查询 Binance 事实，不能靠这里重放请求。

use std::collections::HashMap;

use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;

use crate::domain::{
    AppError, AppResult, BalanceRow, ExchangeOrderAck, OptionAccountSnapshot, OptionContract,
    OptionPosition, OrderAttempt, OrderAttemptStatus, SimpleEarnSnapshot, SpotAccountSnapshot,
    StrategyDirection, StrategyOrder, StrategyRequest, StrategyStatus,
};
use crate::strategy;

/// 从期权恢复记录和交易所回报重建下单请求。
pub(super) fn request_from_option_attempt(
    attempt: &OrderAttempt,
    ack: &ExchangeOrderAck,
    contract: &OptionContract,
) -> AppResult<StrategyRequest> {
    let (_expiry, strike, direction) = parse_option_symbol(&attempt.option_symbol)?;
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
        candidate_id: None,
        client_flow_id: None,
        symbol: attempt.symbol.clone(),
        option_symbol: Some(attempt.option_symbol.clone()),
        direction,
        expiry: contract.expiry,
        expiry_at: contract.expiry_at,
        strike,
        quantity,
        limit_price,
        confirm_live: true,
    })
}

/// 解析 Binance 期权合约代码中的到期日、执行价和方向。
pub(super) fn parse_option_symbol(
    value: &str,
) -> AppResult<(NaiveDate, Decimal, StrategyDirection)> {
    let parts = value.split('-').collect::<Vec<_>>();
    if parts.len() != 4 {
        return Err(AppError::Storage(format!("无法解析期权合约代码：{value}")));
    }
    let expiry = NaiveDate::parse_from_str(parts[1], "%y%m%d")
        .map_err(|error| AppError::Storage(format!("期权合约到期日无法解析：{error}")))?;
    let strike = parts[2]
        .parse::<Decimal>()
        .map_err(|error| AppError::Storage(format!("期权合约执行价无法解析：{error}")))?;
    let direction = match parts[3] {
        "P" | "PUT" => StrategyDirection::SellPut,
        "C" | "CALL" => StrategyDirection::SellCall,
        side => {
            return Err(AppError::Storage(format!("期权合约方向无法解析：{side}")));
        }
    };
    Ok((expiry, strike, direction))
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
    order.fee = ack.fee;
    order.premium = (order.avg_price * order.filled_quantity).round_dp(2);
    order.estimated_pnl = order.premium - order.fee;
    let annualized_quantity = if order.filled_quantity > Decimal::ZERO {
        order.filled_quantity
    } else {
        order.quantity
    };
    order.annualized = strategy::annualized_from_order_terms(
        order.premium,
        order.strike,
        annualized_quantity,
        order.created_at,
        order.expiry_at,
    );
    order.updated_at = ack.update_time.or(ack.create_time).unwrap_or_else(Utc::now);
}

pub(super) fn is_unfilled_terminal_option_ack(ack: &ExchangeOrderAck) -> bool {
    ack.executed_qty <= Decimal::ZERO
        && matches!(
            ack.status.as_str(),
            "EXPIRED" | "REJECTED" | "CANCELED" | "CANCELLED"
        )
}

/// 把 Binance 当前期权持仓应用到接管订单。
pub(super) fn apply_option_position(order: &mut StrategyOrder, position: &OptionPosition) {
    order.status = StrategyStatus::Running;
    order.exchange_status = "POSITION".to_string();
    order.quantity = position.quantity;
    order.filled_quantity = position.quantity;
    if position.entry_price > Decimal::ZERO {
        order.avg_price = position.entry_price;
    }
    order.premium = (order.avg_price * order.filled_quantity).round_dp(2);
    order.estimated_pnl = order.premium - order.fee;
    let annualized_quantity = if order.filled_quantity > Decimal::ZERO {
        order.filled_quantity
    } else {
        order.quantity
    };
    order.annualized = strategy::annualized_from_order_terms(
        order.premium,
        order.strike,
        annualized_quantity,
        order.created_at,
        order.expiry_at,
    );
    order.breakeven = match order.direction {
        StrategyDirection::SellPut => order.strike - order.avg_price,
        StrategyDirection::SellCall => order.strike + order.avg_price,
    };
    order.updated_at = position.update_time.unwrap_or_else(Utc::now);
}

/// 把现货补齐订单回报应用到本地订单。
///
/// 现货没有足额成交时，订单进入 `RebalanceFailed`，避免系统继续自动执行相关动作。
pub(super) fn apply_spot_ack(order: &mut StrategyOrder, ack: &ExchangeOrderAck) {
    order.spot_order_status.clone_from(&ack.status);
    order.spot_filled_quantity = ack.executed_qty;
    order.spot_avg_price = ack.avg_price;
    order.spot_fee = ack.fee;
    order.updated_at = ack.update_time.or(ack.create_time).unwrap_or_else(Utc::now);
    let expected = order.filled_quantity;
    if ack.status == "FILLED" && ack.executed_qty >= expected {
        order.status = StrategyStatus::Settled;
        order.settled_at = Some(Utc::now());
        order.rebalance_message = format!(
            "现货补齐已足额成交，数量 {}，均价 {}，状态 {}",
            ack.executed_qty, ack.avg_price, ack.status
        );
    } else if matches!(
        ack.status.as_str(),
        "REJECTED" | "EXPIRED" | "CANCELED" | "CANCELLED"
    ) {
        order.status = StrategyStatus::RebalanceFailed;
        order.rebalance_message = format!(
            "现货补齐未完成，成交 {} / {}，状态 {}",
            ack.executed_qty, expected, ack.status
        );
    } else {
        order.status = StrategyStatus::RebalanceFailed;
        order.rebalance_message = format!(
            "现货补齐尚未足额成交，成交 {} / {}，状态 {}",
            ack.executed_qty, expected, ack.status
        );
    }
}

pub(super) fn is_spot_attempt(status: OrderAttemptStatus) -> bool {
    matches!(
        status,
        OrderAttemptStatus::SpotAttempted
            | OrderAttemptStatus::SpotExchangeAck
            | OrderAttemptStatus::SpotPersisted
            | OrderAttemptStatus::SpotPersistFailed
    )
}

/// 合并期权保证金和现货资产行。
pub(super) fn combined_balances(
    option: &OptionAccountSnapshot,
    spot: &SpotAccountSnapshot,
    earn: &SimpleEarnSnapshot,
    spot_prices: &HashMap<String, Decimal>,
) -> Vec<BalanceRow> {
    let mut balances = strategy::build_balances(option, spot_prices);
    balances.extend(strategy::build_spot_balances(spot, spot_prices));
    balances.extend(strategy::build_simple_earn_balances(earn, spot_prices));
    balances
}
