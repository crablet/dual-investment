//! 应用状态层校验规则。
//!
//! 本模块在真实下单和到期补齐前校验业务边界、Binance 合约过滤器、
//! 精度、保证金和本地风控。前端传入的字段只能作为请求意图，最终以这里和 Binance
//! 返回的事实为准。

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use super::funding::{spot_asset_free, spot_rebalance_required_asset};
use crate::domain::{
    AppError, AppResult, OptionAccountSnapshot, OptionContract, OptionMarkPrice,
    SpotAccountSnapshot, StrategyDirection, StrategyOrder, StrategyOrderSource, StrategyRequest,
    StrategyStatus,
};
use crate::strategy;

/// 校验下单请求的基础业务边界。
pub(super) fn validate_request(request: &StrategyRequest) -> AppResult<()> {
    if request.client_flow_id.trim().is_empty() {
        return Err(AppError::Config(crate::msg!(
            "backend.manualOrderRequestIsMissingAFlow"
        )));
    }
    if request.option_symbol.trim().is_empty() {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.aManualOrderMustSpecifyABinance"
        )));
    }
    if request.symbol != "BTCUSDT" {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.onlyBtcUsdtStrategiesAreSupportedIn"
        )));
    }
    if request.quantity < dec!(0.01) {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.theBinanceOptionsMinimumIsTreatedAs"
        )));
    }
    if request.strike <= Decimal::ZERO {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.strikePriceMustBeGreaterThanZero"
        )));
    }
    if request.limit_price <= Decimal::ZERO {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.limitPriceMustBeGreaterThanZero"
        )));
    }
    Ok(())
}

/// 按用户复核时冻结的 option symbol 找到 Binance 合约。
pub(super) fn resolve_contract<'a>(
    request: &StrategyRequest,
    contracts: &'a [OptionContract],
) -> AppResult<&'a OptionContract> {
    let contract = contracts
        .iter()
        .find(|contract| contract.symbol == request.option_symbol.trim())
        .ok_or_else(|| {
            AppError::RiskRejected(crate::msg!(
                "backend.specifiedBinanceOptionContractNotFound"
            ))
        })?;
    ensure_contract_matches_request(request, contract)?;
    Ok(contract)
}

/// 确认前端传来的合约代码与请求参数一致。
pub(super) fn ensure_contract_matches_request(
    request: &StrategyRequest,
    contract: &OptionContract,
) -> AppResult<()> {
    if contract.underlying != request.symbol {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.optionUnderlyingDoesNotMatchTheOrder"
        )));
    }
    if contract.expiry_at != request.expiry_at {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.exactContractExpiryDiffersFromTheReview"
        )));
    }
    if contract.strike_price != request.strike {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.contractStrikePriceDoesNotMatchThe"
        )));
    }
    if contract.side != request.direction.option_type() {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.contractSideDoesNotMatchTheOrder"
        )));
    }
    Ok(())
}

/// 备资完成后，按最新账户可用余额校验本次订单的保证金需求。
pub(super) fn validate_option_margin(
    request: &StrategyRequest,
    contract: &OptionContract,
    account: &OptionAccountSnapshot,
) -> AppResult<()> {
    let available = account
        .asset("USDT")
        .map_or(Decimal::ZERO, |asset| asset.available);
    let required = strategy::required_margin(contract, request.direction, request.quantity);
    if available < required {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.insufficientUsdtMarginAtLeastRequiredAvailable",
            p0 = required.round_dp(2),
            p1 = available.round_dp(2)
        )));
    }
    Ok(())
}

/// 校验不需要移动资金的合约与订单条件。
pub(super) fn validate_contract_terms(
    request: &StrategyRequest,
    contract: &OptionContract,
    marks: &[OptionMarkPrice],
) -> AppResult<()> {
    if contract.status != "TRADING" {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.optionContractIsNotTradableStatus",
            p0 = contract.status
        )));
    }
    if contract.underlying != "BTCUSDT" || contract.quote_asset != "USDT" {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.onlyBtcusdtUsdtOptionContractsAreAllowed"
        )));
    }
    if request.quantity < contract.min_qty || request.quantity > contract.max_qty {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.orderQuantityMustBeWithinBinanceContract",
            p0 = contract.min_qty,
            p1 = contract.max_qty
        )));
    }
    if decimal_scale(request.quantity) > contract.quantity_scale {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.orderQuantityExceedsTheBinancePrecisionLimit",
            p0 = contract.quantity_scale
        )));
    }
    if !strategy::decimal_matches_step(request.quantity, contract.quantity_step_size) {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.orderQuantityMustFollowBinanceStepSize",
            p0 = contract.quantity_step_size
        )));
    }
    if contract.min_price > Decimal::ZERO && request.limit_price < contract.min_price {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.limitPriceIsBelowTheBinanceContract",
            p0 = contract.min_price
        )));
    }
    if contract.max_price > Decimal::ZERO && request.limit_price > contract.max_price {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.limitPriceExceedsTheBinanceContractMaximum",
            p0 = contract.max_price
        )));
    }
    if decimal_scale(request.limit_price) > contract.price_scale {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.limitPriceExceedsTheBinancePrecisionLimit",
            p0 = contract.price_scale
        )));
    }
    if !strategy::decimal_matches_step(request.limit_price, contract.price_tick_size) {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.limitPriceMustFollowBinanceTickSize",
            p0 = contract.price_tick_size
        )));
    }

    if let Some(mark) = marks.iter().find(|mark| mark.symbol == contract.symbol) {
        if mark.low_price_limit > Decimal::ZERO && request.limit_price < mark.low_price_limit {
            return Err(AppError::RiskRejected(crate::msg!(
                "backend.limitPriceIsBelowTheCurrentBinance",
                p0 = mark.low_price_limit
            )));
        }
        if mark.high_price_limit > Decimal::ZERO && request.limit_price > mark.high_price_limit {
            return Err(AppError::RiskRejected(crate::msg!(
                "backend.limitPriceExceedsTheCurrentBinanceUpper",
                p0 = mark.high_price_limit
            )));
        }
    }
    Ok(())
}

/// 执行本地风控规则。
pub(super) fn enforce_risk_rules(
    request: &StrategyRequest,
    contract: &OptionContract,
    account: &OptionAccountSnapshot,
) -> AppResult<()> {
    let max_margin_usage = strategy::DEFAULT_MAX_MARGIN_USAGE;
    let margin_usage_tolerance = strategy::DEFAULT_MARGIN_USAGE_TOLERANCE_POINTS;
    let margin_usage_hard_limit = max_margin_usage + margin_usage_tolerance;
    let required = strategy::required_margin(contract, request.direction, request.quantity);

    if let Some(usdt) = account.asset("USDT") {
        let equity = if usdt.adjusted_equity > Decimal::ZERO {
            usdt.adjusted_equity
        } else {
            usdt.equity
        };
        if equity > Decimal::ZERO {
            let next_usage = ((usdt.initial_margin + required) / equity * dec!(100)).round_dp(2);
            if next_usage > margin_usage_hard_limit {
                return Err(AppError::RiskRejected(crate::msg!(
                    "backend.estimatedPostOrderMarginUsageExceedsHard",
                    p0 = next_usage,
                    p1 = margin_usage_hard_limit,
                    p2 = max_margin_usage,
                    p3 = margin_usage_tolerance
                )));
            }
        }
    }

    Ok(())
}

/// 校验到期 ITM 后的现货补齐能力。
pub(super) fn validate_spot_rebalance(
    order: &StrategyOrder,
    account: &SpotAccountSnapshot,
) -> AppResult<()> {
    if !account.can_trade {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.spotAccountCannotTradeExpiryRebalancingStopped"
        )));
    }
    let (asset, required) = spot_rebalance_required_asset(order);
    let available = spot_asset_free(account, asset);
    if available < required {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.insufficientForSpotRebalancingRequiredAvailable",
            p0 = asset,
            p1 = required.round_dp(2),
            p2 = available.round_dp(2)
        )));
    }
    Ok(())
}

/// 判断本地订单是否有可用于查询 Binance 的订单引用。
pub(super) fn has_exchange_order_ref(order: &StrategyOrder) -> bool {
    order
        .exchange_order_id
        .as_ref()
        .is_some_and(|id| !id.is_empty())
        || !order.client_order_id.trim().is_empty()
}

/// 判断订单是否可继续跟踪；接管订单可以没有交易所订单号。
pub(super) fn is_trackable_order(order: &StrategyOrder) -> bool {
    has_exchange_order_ref(order) || order.source == StrategyOrderSource::AdoptedPosition
}

/// 判断期权订单是否仍需要向交易所同步。
pub(super) fn should_sync_option_order(order: &StrategyOrder) -> bool {
    is_trackable_order(order)
        && order.settlement_price <= Decimal::ZERO
        && order.spot_client_order_id.is_empty()
        && !matches!(
            order.status,
            StrategyStatus::Settled
                | StrategyStatus::Rejected
                | StrategyStatus::Expired
                | StrategyStatus::Cancelled
                | StrategyStatus::RebalanceFailed
        )
}

/// 判断订单到期是否 ITM。
pub(super) fn is_in_the_money(order: &StrategyOrder, settlement_price: Decimal) -> bool {
    match order.direction {
        StrategyDirection::SellPut => settlement_price < order.strike,
        StrategyDirection::SellCall => settlement_price > order.strike,
    }
}

/// 返回 Decimal 规范化后的精度，用于匹配 Binance 合约精度限制。
pub(super) fn decimal_scale(value: Decimal) -> u32 {
    value.normalize().scale()
}
