//! Decimal 小工具。
//!
//! 金额、价格、数量和收益率都在 Rust 侧用 `Decimal` 处理。本模块只提供通用边界函数，
//! 避免在策略和状态机里为了取正数/极值退回到浮点数。

use rust_decimal::Decimal;

/// 返回 `value` 和零之间较大的值，用于把负数派生值截断为非负展示值。
pub(crate) fn positive_decimal(value: Decimal) -> Decimal {
    max_decimal(value, Decimal::ZERO)
}

/// 返回两个 Decimal 中较大的值。
pub(crate) fn max_decimal(left: Decimal, right: Decimal) -> Decimal {
    if left >= right { left } else { right }
}

/// 返回两个 Decimal 中较小的值。
pub(crate) fn min_decimal(left: Decimal, right: Decimal) -> Decimal {
    if left <= right { left } else { right }
}
