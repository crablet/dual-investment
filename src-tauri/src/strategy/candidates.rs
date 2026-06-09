//! 双币赢候选扫描与排序。
//!
//! 输入数据来自 Binance 合约、买一价、mark greeks 和现货价格。
//! 这里只构造可展示候选，不发送订单。

use std::collections::HashMap;

use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use super::math::align_decimal_down_to_step;
use crate::domain::{
    OptionContract, OptionMarkPrice, OptionOrderBook, OptionTicker, StrategyCandidate,
    StrategyDirection, StrategyStatus,
};

const BROWSE_MIN_DAYS_TO_EXPIRY: i64 = 1;
const BROWSE_MAX_DAYS_TO_EXPIRY: i64 = 90;
const DEFAULT_CANDIDATE_QUANTITY: Decimal = dec!(0.01);

/// 候选扫描所需的 Binance 实盘输入。
#[derive(Clone, Copy)]
pub struct CandidateBuildInputs<'a> {
    pub contracts: &'a [OptionContract],
    pub marks: &'a [OptionMarkPrice],
    pub tickers: &'a [OptionTicker],
    pub spot_price: Decimal,
}

/// 根据 Binance 合约、买一价、mark greeks 和现货价格构造候选列表。
///
/// 候选可见范围固定为到期 1-90 天，并使用 0.01 张作为手动下单初始数量。
/// 方向过滤必须使用 BTCUSDT 现货价：低买只保留现货价下方 Put，高卖只保留现货价上方 Call。
#[allow(clippy::too_many_lines)]
pub fn build_candidates(inputs: CandidateBuildInputs<'_>) -> Vec<StrategyCandidate> {
    let CandidateBuildInputs {
        contracts,
        marks,
        tickers,
        spot_price,
    } = inputs;

    if spot_price <= Decimal::ZERO {
        return Vec::new();
    }

    let mark_by_symbol = marks
        .iter()
        .map(|mark| (mark.symbol.as_str(), mark))
        .collect::<HashMap<_, _>>();
    let ticker_by_symbol = tickers
        .iter()
        .map(|ticker| (ticker.symbol.as_str(), ticker))
        .collect::<HashMap<_, _>>();
    let mut candidates = Vec::new();

    for contract in contracts {
        if contract.underlying != "BTCUSDT" || contract.quote_asset != "USDT" {
            continue;
        }

        if contract.status != "TRADING" {
            continue;
        }

        if DEFAULT_CANDIDATE_QUANTITY < contract.min_qty
            || DEFAULT_CANDIDATE_QUANTITY > contract.max_qty
        {
            continue;
        }

        if let Some(candidate) = (|| {
            let direction = direction_from_option_side(&contract.side)?;
            // 候选方向必须先经过现货价过滤，避免把价外方向错误展示成双币赢机会。
            if !strike_matches_direction(contract.strike_price, direction, spot_price) {
                return None;
            }

            let days_to_expiry = days_until_expiry(contract.expiry_at);
            if !(BROWSE_MIN_DAYS_TO_EXPIRY..=BROWSE_MAX_DAYS_TO_EXPIRY).contains(&days_to_expiry) {
                return None;
            }

            let mark = mark_by_symbol.get(contract.symbol.as_str()).copied()?;
            if mark.mark_price <= Decimal::ZERO {
                return None;
            }
            let ticker = ticker_by_symbol.get(contract.symbol.as_str()).copied()?;
            if ticker.bid_price <= Decimal::ZERO {
                return None;
            }

            let delta_abs = mark.delta.abs();
            let quantity = DEFAULT_CANDIDATE_QUANTITY;
            // 候选展示使用 Binance 买一价估算卖出 IOC 的可主动成交限价；
            // 真实下单前会重新读取订单簿并按当时买盘深度重算。
            let limit_price = option_sell_taker_limit_price(ticker.bid_price, contract);
            if !candidate_price_satisfies_exchange_limits(limit_price, contract, mark) {
                return None;
            }
            let premium = (limit_price * quantity).round_dp(contract.price_scale);
            let annualized = estimate_annualized(
                premium,
                contract.strike_price,
                quantity * contract.unit,
                days_to_expiry,
            );
            let exercise_probability = (delta_abs * dec!(100)).round_dp(2);
            let strike_distance_percent =
                calculate_strike_distance_percent(contract.strike_price, spot_price);
            Some(StrategyCandidate {
                symbol: contract.underlying.clone(),
                option_symbol: contract.symbol.clone(),
                direction,
                expiry_at: contract.expiry_at,
                strike: contract.strike_price,
                quantity,
                limit_price,
                annualized,
                exercise_probability,
                spot_price,
                strike_distance_percent,
            })
        })() {
            candidates.push(candidate);
        }
    }

    sort_candidates(&mut candidates);

    candidates
}

/// 按精确 UTC 到期时间计算剩余天数。
///
/// 候选筛选仍按“天”展示和配置，但不能只看日期。这里向上取整：不足一天仍按 1 天
/// 处理，既保留现有候选浏览语义，也避免低估临近到期风险。
pub(super) fn days_until_expiry(expiry_at: chrono::DateTime<Utc>) -> i64 {
    let seconds = (expiry_at - Utc::now()).num_seconds();
    if seconds <= 0 {
        return 0;
    }
    (seconds + 86_399) / 86_400
}

/// 按“权利金 ÷ (执行价 × 数量基数) × 365 ÷ 到期天数”估算单利年化百分比。
///
/// 调用方负责按候选或真实订单语义传入数量基数和到期天数；这里不扣交易手续费。
pub(super) fn estimate_annualized(
    premium: Decimal,
    strike: Decimal,
    quantity: Decimal,
    expiry_days: i64,
) -> Decimal {
    let reserved = strike * quantity;
    if reserved <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    let days = Decimal::from(expiry_days);
    ((premium / reserved) * (dec!(365) / days) * dec!(100)).round_dp(2)
}

/// 估算下单所需保证金。
///
/// 卖 Put 需要覆盖名义本金；卖 Call 按 Binance 合约保证金参数估算。
pub fn required_margin(
    contract: &OptionContract,
    direction: StrategyDirection,
    quantity: Decimal,
) -> Decimal {
    let contract_units = quantity * contract.unit;
    let notional = contract.strike_price * contract_units;
    let ratio_margin = notional * contract.initial_margin;
    let min_margin = contract.min_initial_margin * contract_units;
    let maintenance_margin = (notional * contract.maintenance_margin)
        .max(contract.min_maintenance_margin * contract_units);
    match direction {
        StrategyDirection::SellPut => notional
            .max(maintenance_margin)
            .max(ratio_margin)
            .max(min_margin),
        StrategyDirection::SellCall => maintenance_margin.max(ratio_margin).max(min_margin),
    }
}

fn candidate_price_satisfies_exchange_limits(
    price: Decimal,
    contract: &OptionContract,
    mark: &OptionMarkPrice,
) -> bool {
    if price <= Decimal::ZERO {
        return false;
    }
    if contract.min_price > Decimal::ZERO && price < contract.min_price {
        return false;
    }
    if contract.max_price > Decimal::ZERO && price > contract.max_price {
        return false;
    }
    if mark.low_price_limit > Decimal::ZERO && price < mark.low_price_limit {
        return false;
    }
    if mark.high_price_limit > Decimal::ZERO && price > mark.high_price_limit {
        return false;
    }
    true
}

/// 根据当前买盘深度计算期权卖出 IOC 的可主动成交限价。
///
/// 真实下单只使用买一价作为卖出限价；买一量不足时返回 `None`，
/// 调用方必须阻止真实订单，避免在用户确认后悄悄下探到更低买盘价。
pub fn option_sell_taker_limit_price_for_quantity(
    order_book: &OptionOrderBook,
    quantity: Decimal,
    contract: &OptionContract,
) -> Option<Decimal> {
    if quantity <= Decimal::ZERO {
        return None;
    }
    let best_bid = order_book
        .bids
        .iter()
        .find(|level| level.price > Decimal::ZERO && level.quantity > Decimal::ZERO)?;
    if best_bid.quantity < quantity {
        return None;
    }
    let price = option_sell_taker_limit_price(best_bid.price, contract);
    (price > Decimal::ZERO).then_some(price)
}

/// 把期权卖出 IOC 限价对齐到 Binance `PRICE_FILTER` tick。
///
/// 价格来源必须是买盘报价。向下对齐能保持卖单继续主动穿过买盘；
/// 不向上抬到 `minPrice`，否则可能把主动成交单变成挂单或未成交单。
pub fn option_sell_taker_limit_price(price: Decimal, contract: &OptionContract) -> Decimal {
    let tick_aligned = align_decimal_down_to_step(price, contract.price_tick_size);
    if contract.max_price > Decimal::ZERO {
        tick_aligned.min(contract.max_price)
    } else {
        tick_aligned
    }
}

/// 判断数值是否落在交易所 step/tick 网格上。
pub fn decimal_matches_step(value: Decimal, step: Decimal) -> bool {
    if step <= Decimal::ZERO {
        return true;
    }
    (value / step).normalize().scale() == 0
}

/// 把 Binance 订单状态映射成本地策略状态。
///
/// IOC 订单可能返回 `CANCELED` 但已经部分成交，因此需要结合成交数量判断。
pub fn status_from_exchange(
    status: &str,
    executed_qty: Decimal,
    requested_qty: Decimal,
) -> StrategyStatus {
    match status {
        "FILLED" => StrategyStatus::Running,
        "PARTIALLY_FILLED" => StrategyStatus::PartiallyFilled,
        "CANCELED" | "CANCELLED" => {
            if executed_qty > Decimal::ZERO {
                StrategyStatus::PartiallyFilled
            } else {
                StrategyStatus::Cancelled
            }
        }
        "EXPIRED" => {
            if executed_qty > Decimal::ZERO {
                StrategyStatus::PartiallyFilled
            } else {
                StrategyStatus::Expired
            }
        }
        "REJECTED" => StrategyStatus::Rejected,
        "NEW" | "PENDING" => StrategyStatus::Pending,
        _ if executed_qty > Decimal::ZERO && executed_qty < requested_qty => {
            StrategyStatus::PartiallyFilled
        }
        _ if executed_qty > Decimal::ZERO => StrategyStatus::Running,
        _ => StrategyStatus::Pending,
    }
}

pub(super) fn direction_from_option_side(side: &str) -> Option<StrategyDirection> {
    match side {
        "PUT" => Some(StrategyDirection::SellPut),
        "CALL" => Some(StrategyDirection::SellCall),
        _ => None,
    }
}

pub(super) fn breakeven_from_entry(
    strike: Decimal,
    entry_price: Decimal,
    direction: StrategyDirection,
) -> Decimal {
    match direction {
        StrategyDirection::SellPut => strike - entry_price,
        StrategyDirection::SellCall => strike + entry_price,
    }
}

/// 校验执行价是否符合双币赢方向。
fn strike_matches_direction(
    strike: Decimal,
    direction: StrategyDirection,
    spot_price: Decimal,
) -> bool {
    match direction {
        StrategyDirection::SellPut => strike < spot_price,
        StrategyDirection::SellCall => strike > spot_price,
    }
}

/// 计算执行价相对现货价的偏离百分比。
fn calculate_strike_distance_percent(strike: Decimal, spot_price: Decimal) -> Decimal {
    if spot_price <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    (((strike - spot_price) / spot_price) * dec!(100)).round_dp(2)
}

/// 候选排序：年化高优先，再按精确到期时间近优先。
fn sort_candidates(candidates: &mut [StrategyCandidate]) {
    candidates.sort_by(|left, right| {
        right
            .annualized
            .cmp(&left.annualized)
            .then_with(|| left.expiry_at.cmp(&right.expiry_at))
    });
}
