//! Decimal 数值的 tick/step 对齐工具。

use rust_decimal::{Decimal, RoundingStrategy};

/// 向下对齐到交易所 step/tick 网格。
pub(super) fn align_decimal_down_to_step(value: Decimal, step: Decimal) -> Decimal {
    if step <= Decimal::ZERO {
        return value;
    }
    (value / step).round_dp_with_strategy(0, RoundingStrategy::ToNegativeInfinity) * step
}
