//! 策略层 Decimal 数学工具。
//!
//! 这里提供绝对值和 tick/step 对齐等纯函数，供候选过滤、收益估算和订单计算复用。

use rust_decimal::{Decimal, RoundingStrategy};

pub(super) use crate::decimal_utils::{max_decimal, min_decimal};

/// Decimal 绝对值，避免为了简单计算把金融字段转成浮点数。
pub(super) fn abs_decimal(value: Decimal) -> Decimal {
    if value < Decimal::ZERO { -value } else { value }
}

/// 向下对齐到交易所 step/tick 网格。
pub(super) fn align_decimal_down_to_step(value: Decimal, step: Decimal) -> Decimal {
    if step <= Decimal::ZERO {
        return value;
    }
    (value / step).round_dp_with_strategy(0, RoundingStrategy::ToNegativeInfinity) * step
}
