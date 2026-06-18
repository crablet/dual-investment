//! 策略订单快照构造。
//!
//! 本模块把用户请求和 Binance 订单回报转换成本机成交历史，不创建净仓或交割任务。
//! 权利金、年化和保本价统一从已保存的成交字段派生，构造、同步和读库展示使用同一口径。

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

use super::candidates::{breakeven_from_entry, estimate_annualized, status_from_exchange};
use crate::domain::{ExchangeOrderAck, StrategyOrder, StrategyRequest};

/// 从下单请求和 Binance 订单回报构造本地订单。
///
/// 成交数量、均价和交易所状态以 `ack` 为准；请求只提供策略方向和用户确认的参数。
pub fn order_from_request(
    request: &StrategyRequest,
    ack: &ExchangeOrderAck,
    unit: Decimal,
) -> StrategyOrder {
    let now = ack.update_time.or(ack.create_time).unwrap_or_else(Utc::now);
    let opened_at = ack.create_time.or(ack.update_time).unwrap_or(now);
    let quantity = if ack.quantity > Decimal::ZERO {
        ack.quantity
    } else {
        request.quantity
    };
    let status = status_from_exchange(&ack.status, ack.executed_qty, quantity);
    let mut order = StrategyOrder {
        // 一份数据库只属于一个账户；同合约、同交易所订单的恢复必须复用原身份。
        id: format!("OPTION-{}-{}", ack.symbol, ack.order_id),
        exchange_order_id: Some(ack.order_id.clone()),
        client_order_id: ack.client_order_id.clone(),
        symbol: request.symbol.clone(),
        option_symbol: ack.symbol.clone(),
        direction: request.direction,
        status,
        expiry_at: request.expiry_at,
        strike: request.strike,
        unit,
        quantity,
        filled_quantity: ack.executed_qty,
        avg_price: ack.avg_price,
        premium: Decimal::ZERO,
        annualized: Decimal::ZERO,
        breakeven: Decimal::ZERO,
        exchange_status: ack.status.clone(),
        created_at: opened_at,
        updated_at: now,
    };
    refresh_order_metrics(&mut order);
    order
}

/// 只计算实际成交权利金；委托量或委托价不能替代成交事实。
pub(super) fn filled_premium(order: &StrategyOrder) -> Decimal {
    if order.filled_quantity <= Decimal::ZERO || order.avg_price <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    (order.avg_price * order.filled_quantity).round_dp(2)
}

/// 刷新金融展示指标，不改写订单身份、成交数量或结算事实。
///
/// 没有真实成交或有效成交均价时，不用委托价格生成已成交收益和保本价。
pub fn refresh_order_metrics(order: &mut StrategyOrder) {
    if order.filled_quantity <= Decimal::ZERO || order.avg_price <= Decimal::ZERO {
        order.premium = Decimal::ZERO;
        order.annualized = Decimal::ZERO;
        order.breakeven = Decimal::ZERO;
        return;
    }
    order.premium = filled_premium(order);
    order.breakeven = if order.unit > Decimal::ZERO {
        // 成交均价按合约张报价，执行价按 BTC 报价，先统一为每 BTC 权利金。
        breakeven_from_entry(order.strike, order.avg_price / order.unit, order.direction)
    } else {
        Decimal::ZERO
    };
    order.annualized = annualized_from_order_terms(
        order.premium,
        order.strike,
        order.filled_quantity * order.unit,
        order.created_at,
        order.expiry_at,
    );
}

fn annualized_from_order_terms(
    premium: Decimal,
    strike: Decimal,
    quantity: Decimal,
    opened_at: DateTime<Utc>,
    expiry_at: DateTime<Utc>,
) -> Decimal {
    let expiry_days = days_between(opened_at, expiry_at);
    estimate_annualized(premium, strike, quantity, expiry_days)
}

fn days_between(start: DateTime<Utc>, end: DateTime<Utc>) -> i64 {
    let seconds = (end - start).num_seconds();
    if seconds <= 0 {
        return 1;
    }
    (seconds + 86_399) / 86_400
}
