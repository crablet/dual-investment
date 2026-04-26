use rust_decimal::Decimal;

pub(crate) fn positive_decimal(value: Decimal) -> Decimal {
    max_decimal(value, Decimal::ZERO)
}

pub(crate) fn max_decimal(left: Decimal, right: Decimal) -> Decimal {
    if left >= right { left } else { right }
}

pub(crate) fn min_decimal(left: Decimal, right: Decimal) -> Decimal {
    if left <= right { left } else { right }
}
