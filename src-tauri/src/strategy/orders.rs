//! 策略订单快照构造。
//!
//! 本模块把用户请求、Binance 订单回报或当前期权持仓转换成本地 `StrategyOrder`。
//! 订单一旦持久化，其中的年化、权利金、成交数量等字段就是展示事实，前端不得重算。

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
    let option_symbol = if ack.symbol.is_empty() {
        request.option_symbol.clone().unwrap_or_else(|| {
            format!(
                "BTC-{}-{}-{}",
                request.expiry.format("%y%m%d"),
                request.strike,
                match request.direction {
                    StrategyDirection::SellPut => "P",
                    StrategyDirection::SellCall => "C",
                }
            )
        })
    } else {
        ack.symbol.clone()
    };
    let avg_price = if ack.avg_price > Decimal::ZERO {
        ack.avg_price
    } else {
        ack.price
    };
    let premium = (avg_price * ack.executed_qty).round_dp(2);
    let status = status_from_exchange(&ack.status, ack.executed_qty, ack.quantity);
    let annualized_quantity = if ack.executed_qty > Decimal::ZERO {
        ack.executed_qty
    } else {
        request.quantity
    };
    let annualized = annualized_from_order_terms(
        premium,
        request.strike,
        annualized_quantity,
        opened_at,
        request.expiry_at,
    );
    StrategyOrder {
        id: format!("LIVE-{}", Uuid::new_v4().simple()),
        source: StrategyOrderSource::LiveOrder,
        exchange_order_id: Some(ack.order_id.clone()),
        client_order_id: ack.client_order_id.clone(),
        symbol: request.symbol.clone(),
        option_symbol,
        direction: request.direction,
        status,
        expiry: request.expiry,
        expiry_at: request.expiry_at,
        strike: request.strike,
        quantity: request.quantity,
        filled_quantity: ack.executed_qty,
        avg_price,
        premium,
        annualized,
        breakeven: breakeven_from_entry(request.strike, request.limit_price, request.direction),
        exchange_status: ack.status.clone(),
        created_at: now,
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
    }
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
    let premium = (position.entry_price * position.quantity).round_dp(2);
    let annualized = annualized_from_order_terms(
        premium,
        contract.strike_price,
        position.quantity,
        now,
        contract.expiry_at,
    );
    StrategyOrder {
        id: format!("ADOPT-{}", Uuid::new_v4().simple()),
        source: StrategyOrderSource::AdoptedPosition,
        exchange_order_id: None,
        client_order_id: String::new(),
        symbol: contract.underlying.clone(),
        option_symbol: position.symbol.clone(),
        direction,
        status: StrategyStatus::Running,
        expiry: contract.expiry,
        expiry_at: contract.expiry_at,
        strike: contract.strike_price,
        quantity: position.quantity,
        filled_quantity: position.quantity,
        avg_price: position.entry_price,
        premium,
        annualized,
        breakeven: breakeven_from_entry(contract.strike_price, position.entry_price, direction),
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
    }
}

/// 按真实成交/持仓字段估算订单年化收益率。
///
/// 订单持久化后年化成为订单事实字段，前端只负责展示，不重新做金融计算。
pub fn annualized_from_order_terms(
    premium: Decimal,
    strike: Decimal,
    quantity: Decimal,
    opened_at: DateTime<Utc>,
    expiry_at: DateTime<Utc>,
) -> Decimal {
    let expiry_days = days_between(opened_at, expiry_at).max(1);
    estimate_annualized(premium, strike, quantity, expiry_days)
}

fn days_between(start: DateTime<Utc>, end: DateTime<Utc>) -> i64 {
    let seconds = (end - start).num_seconds();
    if seconds <= 0 {
        return 1;
    }
    (seconds + 86_399) / 86_400
}
