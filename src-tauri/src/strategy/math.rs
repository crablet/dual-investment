//! 策略层 Decimal 数学工具。
//!
//! 这里提供收益率、绝对值和 tick/step 对齐等纯函数，调用方负责传入业务字段和处理
//! 分母为零等语义默认值。

use rust_decimal::{Decimal, RoundingStrategy};
use rust_decimal_macros::dec;

pub(super) use crate::decimal_utils::{max_decimal, min_decimal};

/// 计算百分比，分母为 0 时交给调用方决定默认值。
pub(super) fn percent_of(part: Decimal, total: Decimal) -> Option<Decimal> {
    if total <= Decimal::ZERO {
        None
    } else {
        Some(((part / total) * dec!(100)).round_dp(2))
    }
}

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
