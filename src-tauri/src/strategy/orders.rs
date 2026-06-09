//! 策略订单快照构造。
//!
//! 本模块把用户请求、Binance 订单回报或当前期权持仓转换成本地 `StrategyOrder`。
//! 权利金、年化和保本价统一从已保存的成交字段派生，构造、同步和读库展示使用同一口径。

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use super::candidates::{
    breakeven_from_entry, direction_from_option_side, estimate_annualized, status_from_exchange,
};
use crate::domain::{
    ExchangeOrderAck, OptionContract, OptionPosition, StrategyDirection, StrategyOrder,
    StrategyOrderSource, StrategyRequest, StrategyStatus,
};

/// 从下单请求和 Binance 订单回报构造本地订单。
///
/// 成交数量、均价和交易所状态以 `ack` 为准；请求只提供策略方向和用户确认的参数。
pub fn order_from_request(request: &StrategyRequest, ack: &ExchangeOrderAck) -> StrategyOrder {
    let now = ack.update_time.or(ack.create_time).unwrap_or_else(Utc::now);
    let opened_at = ack.create_time.or(ack.update_time).unwrap_or(now);
    let status = status_from_exchange(&ack.status, ack.executed_qty, ack.quantity);
    let mut order = StrategyOrder {
        id: format!("LIVE-{}", Uuid::new_v4().simple()),
        source: StrategyOrderSource::LiveOrder,
        exchange_order_id: Some(ack.order_id.clone()),
        client_order_id: ack.client_order_id.clone(),
        symbol: request.symbol.clone(),
        option_symbol: ack.symbol.clone(),
        direction: request.direction,
        status,
        expiry_at: request.expiry_at,
        strike: request.strike,
        quantity: request.quantity,
        filled_quantity: ack.executed_qty,
        avg_price: ack.avg_price,
        premium: Decimal::ZERO,
        annualized: Decimal::ZERO,
        breakeven: Decimal::ZERO,
        exchange_status: ack.status.clone(),
        created_at: opened_at,
        updated_at: now,
        settlement_action: match request.direction {
            StrategyDirection::SellPut => "若到期低于执行价，自动买入 BTC 补齐换币".to_string(),
            StrategyDirection::SellCall => "若到期高于执行价，自动卖出 BTC 补齐换币".to_string(),
        },
        settlement_price: Decimal::ZERO,
        settled_at: None,
        spot_order_id: None,
        spot_client_order_id: String::new(),
        spot_order_status: String::new(),
        rebalance_message: String::new(),
        spot_filled_quantity: Decimal::ZERO,
        spot_avg_price: Decimal::ZERO,
    };
    refresh_order_metrics(&mut order);
    order
}

/// 从 Binance 当前期权持仓构造本地接管订单。
///
/// 接管订单可能没有原始订单号；后续同步会继续以当前持仓和到期行权记录为事实来源。
pub fn order_from_adopted_position(
    position: &OptionPosition,
    contract: &OptionContract,
) -> StrategyOrder {
    let direction = direction_from_option_side(&contract.side)
        .expect("validated option contract side before adoption");
    let now = position.update_time.unwrap_or_else(Utc::now);
    let mut order = StrategyOrder {
        id: format!("ADOPT-{}", Uuid::new_v4().simple()),
        source: StrategyOrderSource::AdoptedPosition,
        exchange_order_id: None,
        client_order_id: String::new(),
        symbol: contract.underlying.clone(),
        option_symbol: position.symbol.clone(),
        direction,
        status: StrategyStatus::Running,
        expiry_at: contract.expiry_at,
        strike: contract.strike_price,
        quantity: position.quantity,
        filled_quantity: position.quantity,
        avg_price: position.entry_price,
        premium: Decimal::ZERO,
        annualized: Decimal::ZERO,
        breakeven: Decimal::ZERO,
        exchange_status: "POSITION".to_string(),
        created_at: now,
        updated_at: now,
        settlement_action: match direction {
            StrategyDirection::SellPut => {
                "接管持仓：若到期低于执行价，自动买入 BTC 补齐换币".to_string()
            }
            StrategyDirection::SellCall => {
                "接管持仓：若到期高于执行价，自动卖出 BTC 补齐换币".to_string()
            }
        },
        settlement_price: Decimal::ZERO,
        settled_at: None,
        spot_order_id: None,
        spot_client_order_id: String::new(),
        spot_order_status: String::new(),
        rebalance_message: "已从 Binance 当前期权持仓接管，本机继续监控到期结算".to_string(),
        spot_filled_quantity: Decimal::ZERO,
        spot_avg_price: Decimal::ZERO,
    };
    refresh_order_metrics(&mut order);
    order
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
    order.premium = (order.avg_price * order.filled_quantity).round_dp(2);
    order.breakeven = breakeven_from_entry(order.strike, order.avg_price, order.direction);
    order.annualized = annualized_from_order_terms(
        order.premium,
        order.strike,
        order.filled_quantity,
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
