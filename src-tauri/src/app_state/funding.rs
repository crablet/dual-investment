use rust_decimal::{Decimal, RoundingStrategy};
use rust_decimal_macros::dec;

use crate::decimal_utils::{max_decimal, min_decimal, positive_decimal};
use crate::domain::{
    OptionAccountSnapshot, OptionContract, SimpleEarnSnapshot, SpotAccountSnapshot,
    StrategyDirection, StrategyOrder, StrategyRequest,
};
use crate::strategy;

const SELL_PUT_REBALANCE_USDT_BUFFER: Decimal = dec!(1.10);

pub(super) fn option_funding_shortfall(
    account: &OptionAccountSnapshot,
    contract: &OptionContract,
    request: &StrategyRequest,
) -> Decimal {
    let required = strategy::required_margin(contract, request.direction, request.quantity);
    let available = account
        .asset("USDT")
        .map_or(Decimal::ZERO, |asset| asset.available);
    let margin_shortfall = positive_decimal(required - available);

    let rules = strategy::risk_snapshot_from_account(account).rules;
    let Some(usdt) = account.asset("USDT") else {
        return max_decimal(
            margin_shortfall,
            risk_equity_target(required, rules.max_margin_usage),
        );
    };
    let equity = option_usdt_equity(usdt);
    let target_equity = risk_equity_target(usdt.initial_margin + required, rules.max_margin_usage);
    max_decimal(margin_shortfall, positive_decimal(target_equity - equity))
}

pub(super) fn option_idle_usdt_sweep_amount(account: &OptionAccountSnapshot) -> Decimal {
    let Some(usdt) = account.asset("USDT") else {
        return Decimal::ZERO;
    };
    let rules = strategy::risk_snapshot_from_account(account).rules;
    let equity = option_usdt_equity(usdt);
    let min_equity = risk_equity_target(usdt.initial_margin, rules.max_margin_usage);
    let excess_equity = positive_decimal(equity - min_equity);
    min_decimal(usdt.available, excess_equity)
}

pub(super) fn option_usdt_equity(usdt: &crate::domain::OptionAssetBalance) -> Decimal {
    if usdt.adjusted_equity > Decimal::ZERO {
        usdt.adjusted_equity
    } else {
        usdt.equity
    }
}

pub(super) fn risk_equity_target(margin: Decimal, max_usage_percent: Decimal) -> Decimal {
    if margin <= Decimal::ZERO || max_usage_percent <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    margin * dec!(100) / max_usage_percent
}

pub(super) fn spot_rebalance_required_asset(order: &StrategyOrder) -> (&'static str, Decimal) {
    match order.direction {
        StrategyDirection::SellPut => (
            "USDT",
            order.settlement_price * order.filled_quantity * SELL_PUT_REBALANCE_USDT_BUFFER,
        ),
        StrategyDirection::SellCall => ("BTC", order.filled_quantity),
    }
}

pub(super) fn spot_asset_free(account: &SpotAccountSnapshot, asset: &str) -> Decimal {
    account.asset(asset).map_or(Decimal::ZERO, |row| row.free)
}

pub(super) fn flexible_earn_asset_amount(earn: &SimpleEarnSnapshot, asset: &str) -> Decimal {
    earn.balances
        .iter()
        .filter(|balance| balance.flexible && balance.can_redeem && balance.asset == asset)
        .map(|balance| balance.amount)
        .sum()
}

pub(super) fn round_up_funding_amount(value: Decimal) -> Decimal {
    if value <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    value.round_dp_with_strategy(2, RoundingStrategy::ToPositiveInfinity)
}
