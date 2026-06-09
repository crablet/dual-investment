//! 资金准备与补齐金额计算。
//!
//! 本模块只计算缺口、可赎回金额和 Binance 资金动作所需精度。真正的 Simple Earn
//! 赎回、申购和 Universal Transfer 必须先写恢复记录，再由 `AppState` 串行发送。

use rust_decimal::{Decimal, RoundingStrategy};
use rust_decimal_macros::dec;

use crate::domain::{
    OptionAccountSnapshot, OptionContract, SettlementAssets, SpotAccountSnapshot,
    StrategyDirection, StrategyOrder, StrategyRequest,
};
use crate::strategy;

const SELL_PUT_REBALANCE_USDT_BUFFER: Decimal = dec!(1.02);
const BINANCE_FUNDING_AMOUNT_SCALE: u32 = 8;

/// 计算下单前需要补入期权保证金账户的 USDT 缺口。
///
/// 缺口同时覆盖 Binance 可用保证金需求和本地保证金占用率风控所需的权益缓冲。
pub(super) fn option_funding_shortfall(
    account: &OptionAccountSnapshot,
    contract: &OptionContract,
    request: &StrategyRequest,
) -> Decimal {
    let required = strategy::required_margin(contract, request.direction, request.quantity);
    let available = account
        .asset("USDT")
        .map_or(Decimal::ZERO, |asset| asset.available);
    let margin_shortfall = (required - available).max(Decimal::ZERO);

    let max_margin_usage = strategy::DEFAULT_MAX_MARGIN_USAGE;
    let Some(usdt) = account.asset("USDT") else {
        return margin_shortfall.max(risk_equity_target(required, max_margin_usage));
    };
    let equity = option_usdt_equity(usdt);
    let target_equity = risk_equity_target(usdt.initial_margin + required, max_margin_usage);
    margin_shortfall.max((target_equity - equity).max(Decimal::ZERO))
}

/// 计算期权保证金账户可归集回 Simple Earn 的闲置 USDT。
pub(super) fn option_idle_usdt_sweep_amount(account: &OptionAccountSnapshot) -> Decimal {
    let Some(usdt) = account.asset("USDT") else {
        return Decimal::ZERO;
    };
    let max_margin_usage = strategy::DEFAULT_MAX_MARGIN_USAGE;
    let equity = option_usdt_equity(usdt);
    let min_equity = risk_equity_target(usdt.initial_margin, max_margin_usage);
    let excess_equity = (equity - min_equity).max(Decimal::ZERO);
    usdt.available.min(excess_equity)
}

/// 只划回当前接货缺口，且不超过保留其余期权保证金后可用的 USDT。
pub(super) fn settlement_option_funding_amount(
    account: &OptionAccountSnapshot,
    spot: &SpotAccountSnapshot,
    required: &SettlementAssets,
) -> Decimal {
    let deficit = (required.usdt - spot_asset_free(spot, "USDT")).max(Decimal::ZERO);
    round_down_funding_amount(deficit.min(option_idle_usdt_sweep_amount(account)))
}

pub(super) fn spot_has_settlement_assets(
    spot: &SpotAccountSnapshot,
    required: &SettlementAssets,
) -> bool {
    spot_asset_free(spot, "USDT") >= required.usdt && spot_asset_free(spot, "BTC") >= required.btc
}

/// 选取 Binance 期权账户 USDT 权益口径，优先使用调整后权益。
pub(super) fn option_usdt_equity(usdt: &crate::domain::OptionAssetBalance) -> Decimal {
    if usdt.adjusted_equity > Decimal::ZERO {
        usdt.adjusted_equity
    } else {
        usdt.equity
    }
}

/// 根据保证金占用率上限反推需要保留的账户权益。
pub(super) fn risk_equity_target(margin: Decimal, max_usage_percent: Decimal) -> Decimal {
    if margin <= Decimal::ZERO || max_usage_percent <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    margin * dec!(100) / max_usage_percent
}

/// 到期 ITM 后现货补齐所需资产；卖 Put 需要 USDT 缓冲，卖 Call 需要 BTC。
pub(super) fn spot_rebalance_required_asset(order: &StrategyOrder) -> (&'static str, Decimal) {
    match order.direction {
        StrategyDirection::SellPut => (
            "USDT",
            order.settlement_price * order.filled_quantity * SELL_PUT_REBALANCE_USDT_BUFFER,
        ),
        StrategyDirection::SellCall => ("BTC", order.filled_quantity),
    }
}

/// 查询现货账户某资产可用余额，缺失资产按 0 处理。
pub(super) fn spot_asset_free(account: &SpotAccountSnapshot, asset: &str) -> Decimal {
    account.asset(asset).map_or(Decimal::ZERO, |row| row.free)
}

/// 资金准备时向上取整，避免小数截断导致真实可用余额仍不足。
pub(super) fn round_up_funding_amount(value: Decimal) -> Decimal {
    if value <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    value.round_dp_with_strategy(2, RoundingStrategy::ToPositiveInfinity)
}

/// 资金归集/申购前向下对齐到 Binance 接受的小数位，避免超额划转或请求被拒。
pub(super) fn round_down_funding_amount(value: Decimal) -> Decimal {
    if value <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    value.round_dp_with_strategy(
        BINANCE_FUNDING_AMOUNT_SCALE,
        RoundingStrategy::ToNegativeInfinity,
    )
}

/// 汇总尚未完成的补齐资金。预留在本应用内生效，发送前仍须查询实盘余额。
pub(super) fn settlement_required_assets(
    orders: &[StrategyOrder],
) -> crate::domain::SettlementAssets {
    let mut assets = crate::domain::SettlementAssets::default();
    for order in orders
        .iter()
        .filter(|order| order.status != crate::domain::StrategyStatus::Settled)
    {
        let (asset, amount) = spot_rebalance_required_asset(order);
        match asset {
            "USDT" => assets.usdt += amount,
            "BTC" => assets.btc += amount,
            _ => {}
        }
    }
    assets
}

pub(super) fn settlement_funding_amount(asset: &str, value: Decimal) -> Decimal {
    value.max(Decimal::ZERO).round_dp_with_strategy(
        if asset == "USDT" {
            2
        } else {
            BINANCE_FUNDING_AMOUNT_SCALE
        },
        RoundingStrategy::ToPositiveInfinity,
    )
}
