//! 应用状态层校验规则。
//!
//! 本模块在真实下单和到期补齐前校验业务边界、Binance 合约过滤器、
//! 精度、保证金和本地风控。前端传入的字段只能作为请求意图，最终以这里和 Binance
//! 返回的事实为准。

use chrono::Utc;
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
    if request.symbol != "BTCUSDT" {
        return Err(AppError::RiskRejected(
            "v1 仅支持 BTC/USDT 策略".to_string(),
        ));
    }
    if request.quantity < dec!(0.01) {
        return Err(AppError::RiskRejected(
            "币安期权最小交易单位按 0.01 张处理".to_string(),
        ));
    }
    if request.strike <= Decimal::ZERO {
        return Err(AppError::RiskRejected("执行价必须大于 0".to_string()));
    }
    if request.limit_price <= Decimal::ZERO {
        return Err(AppError::RiskRejected("限价必须大于 0".to_string()));
    }
    Ok(())
}

/// 找到与下单请求匹配的 Binance 合约。
///
/// 优先使用请求里的 option symbol；没有时才按方向、到期日和执行价搜索。
pub(super) fn resolve_contract<'a>(
    request: &StrategyRequest,
    contracts: &'a [OptionContract],
) -> AppResult<&'a OptionContract> {
    if let Some(symbol) = request
        .option_symbol
        .as_deref()
        .map(str::trim)
        .filter(|symbol| !symbol.is_empty())
    {
        let contract = contracts
            .iter()
            .find(|contract| contract.symbol == symbol)
            .ok_or_else(|| AppError::RiskRejected("未找到指定的 Binance 期权合约".to_string()))?;
        ensure_contract_matches_request(request, contract)?;
        return Ok(contract);
    }

    contracts
        .iter()
        .find(|contract| {
            contract.underlying == request.symbol
                && contract.expiry == request.expiry
                && contract.strike_price == request.strike
                && contract.side == request.direction.option_type()
        })
        .ok_or_else(|| {
            AppError::RiskRejected("未找到与方向、到期日和执行价匹配的 Binance 合约".to_string())
        })
}

/// 确认前端传来的合约代码与请求参数一致。
pub(super) fn ensure_contract_matches_request(
    request: &StrategyRequest,
    contract: &OptionContract,
) -> AppResult<()> {
    if contract.underlying != request.symbol {
        return Err(AppError::RiskRejected(
            "期权合约标的与下单标的不一致".to_string(),
        ));
    }
    if contract.expiry != request.expiry {
        return Err(AppError::RiskRejected(
            "期权合约到期日与下单到期日不一致".to_string(),
        ));
    }
    if contract.strike_price != request.strike {
        return Err(AppError::RiskRejected(
            "期权合约执行价与下单执行价不一致".to_string(),
        ));
    }
    if contract.side != request.direction.option_type() {
        return Err(AppError::RiskRejected(
            "期权合约方向与下单方向不一致".to_string(),
        ));
    }
    Ok(())
}

/// 下单前合约与交易所约束校验。
///
/// 包含交易状态、v1 标的限制、数量精度、价格精度、价格上下限和保证金检查。
pub(super) fn validate_contract_order(
    request: &StrategyRequest,
    contract: &OptionContract,
    account: &OptionAccountSnapshot,
    marks: &[OptionMarkPrice],
) -> AppResult<()> {
    validate_contract_terms(request, contract, marks)?;
    let available = account
        .asset("USDT")
        .map_or(Decimal::ZERO, |asset| asset.available);
    let required = strategy::required_margin(contract, request.direction, request.quantity);
    if available < required {
        return Err(AppError::RiskRejected(format!(
            "可用 USDT 保证金不足，预计至少需要 {}，当前可用 {}",
            required.round_dp(2),
            available.round_dp(2)
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
        return Err(AppError::RiskRejected(format!(
            "期权合约当前不可交易，状态 {}",
            contract.status
        )));
    }
    if contract.underlying != "BTCUSDT" || contract.quote_asset != "USDT" {
        return Err(AppError::RiskRejected(
            "v1 仅允许 BTCUSDT/USDT 期权合约".to_string(),
        ));
    }
    if request.quantity < contract.min_qty || request.quantity > contract.max_qty {
        return Err(AppError::RiskRejected(format!(
            "下单数量必须在 Binance 合约范围 {} - {} 内",
            contract.min_qty, contract.max_qty
        )));
    }
    if decimal_scale(request.quantity) > contract.quantity_scale {
        return Err(AppError::RiskRejected(format!(
            "下单数量精度超过 Binance 合约限制 {} 位",
            contract.quantity_scale
        )));
    }
    if !strategy::decimal_matches_step(request.quantity, contract.quantity_step_size) {
        return Err(AppError::RiskRejected(format!(
            "下单数量必须按 Binance step size {} 递增",
            contract.quantity_step_size
        )));
    }
    if contract.min_price > Decimal::ZERO && request.limit_price < contract.min_price {
        return Err(AppError::RiskRejected(format!(
            "限价低于 Binance 合约最小价格 {}",
            contract.min_price
        )));
    }
    if contract.max_price > Decimal::ZERO && request.limit_price > contract.max_price {
        return Err(AppError::RiskRejected(format!(
            "限价高于 Binance 合约最大价格 {}",
            contract.max_price
        )));
    }
    if decimal_scale(request.limit_price) > contract.price_scale {
        return Err(AppError::RiskRejected(format!(
            "限价精度超过 Binance 合约限制 {} 位",
            contract.price_scale
        )));
    }
    if !strategy::decimal_matches_step(request.limit_price, contract.price_tick_size) {
        return Err(AppError::RiskRejected(format!(
            "限价必须按 Binance tick size {} 递增",
            contract.price_tick_size
        )));
    }

    if let Some(mark) = marks.iter().find(|mark| mark.symbol == contract.symbol) {
        if mark.low_price_limit > Decimal::ZERO && request.limit_price < mark.low_price_limit {
            return Err(AppError::RiskRejected(format!(
                "限价低于 Binance 当前价格下限 {}",
                mark.low_price_limit
            )));
        }
        if mark.high_price_limit > Decimal::ZERO && request.limit_price > mark.high_price_limit {
            return Err(AppError::RiskRejected(format!(
                "限价高于 Binance 当前价格上限 {}",
                mark.high_price_limit
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
                return Err(AppError::RiskRejected(format!(
                    "下单后保证金占用预计 {next_usage}%，超过风控硬上限 {margin_usage_hard_limit}%（配置上限 {max_margin_usage}% + 容差 {margin_usage_tolerance} 个百分点）"
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
        return Err(AppError::RiskRejected(
            "现货账户不可交易，停止到期补齐".to_string(),
        ));
    }
    let (asset, required) = spot_rebalance_required_asset(order);
    let available = spot_asset_free(account, asset);
    if available < required {
        return Err(AppError::RiskRejected(format!(
            "现货补齐 {asset} 不足，预计需要 {}，当前可用 {}",
            required.round_dp(2),
            available.round_dp(2)
        )));
    }
    Ok(())
}

/// 判断下单/现货补齐请求是否处于结果不确定状态，需要先查交易所事实再恢复。
pub(super) fn is_uncertain_submit_error(error: &AppError) -> bool {
    matches!(
        error,
        AppError::Binance(message)
            if message.contains("请求超时")
                || message.contains("连接失败")
                || message.contains("响应体读取失败")
                || message.contains("网络请求失败")
    )
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
        && !matches!(
            order.status,
            StrategyStatus::Settled
                | StrategyStatus::Rejected
                | StrategyStatus::Expired
                | StrategyStatus::Cancelled
                | StrategyStatus::RebalanceFailed
        )
}

/// 判断订单是否已经过期并需要结算处理。
pub(super) fn should_settle_order(order: &StrategyOrder) -> bool {
    Utc::now() >= order.expiry_at
        && matches!(
            order.status,
            StrategyStatus::Running | StrategyStatus::PartiallyFilled | StrategyStatus::Pending
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
