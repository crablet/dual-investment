//! 双币赢候选扫描与推荐排序。
//!
//! 输入数据来自 Binance 合约、mark price、账户快照和本地真实订单。这里只构造可展示候选
//! 和推荐等级，不发送订单，也不把规则不命中的候选直接隐藏成“没有机会”。

use std::collections::HashMap;

use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use super::math::{abs_decimal, align_decimal_down_to_step, max_decimal, min_decimal};
use crate::domain::{
    AutoInvestmentRule, CandidateScanResult, OptionAccountSnapshot, OptionContract,
    OptionMarkPrice, RecommendationLevel, StrategyCandidate, StrategyDirection, StrategyOrder,
    StrategyStatus,
};

const BROWSE_MIN_DAYS_TO_EXPIRY: i64 = 1;
const BROWSE_MAX_DAYS_TO_EXPIRY: i64 = 90;

/// 根据 Binance 合约、mark price、账户和规则构造候选列表。
///
/// 候选可见范围固定为到期 1-90 天；自动复投规则只影响推荐等级，不决定候选是否可见。
/// 方向过滤必须使用 BTCUSDT 现货价：低买只保留现货价下方 Put，高卖只保留现货价上方 Call。
#[allow(clippy::too_many_lines)]
pub fn build_candidates(
    contracts: &[OptionContract],
    marks: &[OptionMarkPrice],
    rule: &AutoInvestmentRule,
    account: &OptionAccountSnapshot,
    orders: &[StrategyOrder],
    spot_price: Decimal,
    funding_available_usdt: Decimal,
) -> CandidateScanResult {
    if spot_price <= Decimal::ZERO {
        return CandidateScanResult {
            candidates: Vec::new(),
        };
    }

    let mark_by_symbol = marks
        .iter()
        .map(|mark| (mark.symbol.as_str(), mark))
        .collect::<HashMap<_, _>>();
    let has_open_direction = |direction| {
        orders.iter().any(|order| {
            order.direction == direction
                && matches!(
                    order.status,
                    StrategyStatus::Pending
                        | StrategyStatus::Running
                        | StrategyStatus::PartiallyFilled
                )
        })
    };

    let mut candidates = Vec::new();

    for contract in contracts {
        if contract.underlying != "BTCUSDT" || contract.quote_asset != "USDT" {
            continue;
        }

        if contract.status != "TRADING" {
            continue;
        }

        if rule.quantity < contract.min_qty || rule.quantity > contract.max_qty {
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

            let delta_abs = abs_decimal(mark.delta);
            let iv_rank = (mark.mark_iv * dec!(100)).round_dp(2);
            let quantity = rule.quantity;
            // 候选展示使用 Binance mark price；真实下单会在编排层按 tick size 对齐。
            let limit_price = mark.mark_price.round_dp(contract.price_scale);
            let premium = (limit_price * quantity).round_dp(contract.price_scale);
            let annualized = estimate_annualized(
                premium,
                contract.strike_price,
                quantity * contract.unit,
                days_to_expiry.max(1),
            );
            let exercise_probability = (delta_abs * dec!(100)).round_dp(2);
            let strike_distance_percent =
                calculate_strike_distance_percent(contract.strike_price, spot_price);
            let breakeven = match direction {
                StrategyDirection::SellPut => contract.strike_price - limit_price,
                StrategyDirection::SellCall => contract.strike_price + limit_price,
            };
            let required_margin = required_margin(contract, direction, quantity);
            let available_margin = account
                .asset("USDT")
                .map_or(Decimal::ZERO, |asset| asset.available)
                + funding_available_usdt;
            // 规则命中只影响推荐等级；不命中规则但仍有保证金的候选会保留为 Watch，方便排查。
            let passes_rule = rule.include_btc
                && (!rule.exclude_same_direction || !has_open_direction(direction))
                && days_to_expiry >= rule.min_days_to_expiry
                && days_to_expiry <= rule.max_days_to_expiry
                && annualized >= rule.min_annualized
                && delta_abs <= rule.max_delta_abs
                && iv_rank <= rule.max_iv_rank
                && available_margin >= required_margin;
            let recommendation = if passes_rule {
                RecommendationLevel::Recommended
            } else if annualized >= Decimal::ZERO && available_margin >= required_margin {
                RecommendationLevel::Watch
            } else {
                RecommendationLevel::Reject
            };
            let reason = if passes_rule {
                format!(
                    "来自 Binance 合约和 mark price，满足当前自动规则，mark IV 代理值 {iv_rank}%"
                )
            } else if available_margin < required_margin {
                "可用保证金不足，暂不建议下单".to_string()
            } else {
                format!(
                    "来自 Binance 合约和 mark price，未完全满足当前自动规则，mark IV 代理值 {iv_rank}%"
                )
            };

            Some(StrategyCandidate {
                id: format!("{}-{}", contract.symbol, Utc::now().timestamp()),
                symbol: contract.underlying.clone(),
                option_symbol: contract.symbol.clone(),
                direction,
                expiry: contract.expiry,
                expiry_at: contract.expiry_at,
                strike: contract.strike_price,
                quantity,
                limit_price,
                premium,
                annualized,
                exercise_probability,
                breakeven,
                spot_price,
                strike_distance_percent,
                recommendation,
                reason,
            })
        })() {
            candidates.push(candidate);
        }
    }

    sort_candidates(&mut candidates);

    CandidateScanResult { candidates }
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

/// 估算年化收益率。
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
    let maintenance_margin = max_decimal(
        notional * contract.maintenance_margin,
        contract.min_maintenance_margin * contract_units,
    );
    match direction {
        StrategyDirection::SellPut => max_decimal(
            notional,
            max_decimal(maintenance_margin, max_decimal(ratio_margin, min_margin)),
        ),
        StrategyDirection::SellCall => {
            max_decimal(maintenance_margin, max_decimal(ratio_margin, min_margin))
        }
    }
}

/// 把期权卖单限价对齐到 Binance `PRICE_FILTER` tick。
///
/// 期权下单始终是卖出限价 IOC。向下对齐不会把最低可接受权利金抬高到 mark price 之上，
/// 同时避免把不在 tick size 网格上的价格发给 Binance。
pub fn option_sell_limit_price(price: Decimal, contract: &OptionContract) -> Decimal {
    let tick_aligned = align_decimal_down_to_step(price, contract.price_tick_size);
    let min_aligned = if contract.min_price > Decimal::ZERO {
        max_decimal(tick_aligned, contract.min_price)
    } else {
        tick_aligned
    };
    if contract.max_price > Decimal::ZERO {
        min_decimal(min_aligned, contract.max_price)
    } else {
        min_aligned
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

/// 推荐等级排序权重。
fn recommendation_rank(level: RecommendationLevel) -> u8 {
    match level {
        RecommendationLevel::Reject => 0,
        RecommendationLevel::Watch => 1,
        RecommendationLevel::Recommended => 2,
    }
}

/// 候选排序：推荐优先，其次年化高优先，再按精确到期时间近优先。
fn sort_candidates(candidates: &mut [StrategyCandidate]) {
    candidates.sort_by(|left, right| {
        recommendation_rank(right.recommendation)
            .cmp(&recommendation_rank(left.recommendation))
            .then_with(|| right.annualized.cmp(&left.annualized))
            .then_with(|| left.expiry_at.cmp(&right.expiry_at))
    });
}
