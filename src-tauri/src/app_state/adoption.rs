//! Binance 外部期权持仓接管辅助逻辑。
//!
//! 接管只允许从 Binance 当前义务仓和合约信息创建本地跟踪订单，不会重放历史下单，
//! 也不会把接管状态混入 `order_attempts` 恢复记录。

use crate::messages::Message;

use std::collections::HashSet;

use chrono::Utc;
use rust_decimal::Decimal;

use crate::domain::{
    AdoptableOptionPositionView, AppError, AppResult, OptionContract, OptionPosition,
    StrategyDirection, StrategyOrder, StrategyStatus,
};

pub(super) fn normalized_unique_symbols(symbols: &[String]) -> AppResult<Vec<String>> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for symbol in symbols
        .iter()
        .map(|symbol| symbol.trim())
        .filter(|symbol| !symbol.is_empty())
    {
        if seen.insert(symbol.to_string()) {
            normalized.push(symbol.to_string());
        }
    }
    if normalized.is_empty() {
        return Err(AppError::Config(crate::msg!(
            "backend.selectOptionPositionsToTakeOver"
        )));
    }
    Ok(normalized)
}

/// 构造前端接管列表中的一行，包含当前是否可接管和原因。
pub(super) fn adoptable_position_view(
    position: &OptionPosition,
    contract: &OptionContract,
    orders: &[StrategyOrder],
) -> AppResult<AdoptableOptionPositionView> {
    let direction = direction_from_contract(contract)?;
    let already_tracked = has_tracked_option_symbol(orders, &position.symbol);
    let (adoptable, reason) = adoptable_position_state(position, contract, already_tracked);
    Ok(AdoptableOptionPositionView {
        symbol: position.symbol.clone(),
        direction,
        expiry_at: contract.expiry_at,
        strike: contract.strike_price,
        quantity: position.quantity,
        entry_price: position.entry_price,
        mark_price: position.mark_price,
        premium: (position.entry_price * position.quantity).round_dp(2),
        already_tracked,
        adoptable,
        reason,
    })
}

/// 接管提交前再次校验仓位仍然满足接管条件。
pub(super) fn ensure_position_can_be_adopted(
    position: &OptionPosition,
    contract: &OptionContract,
    orders: &[StrategyOrder],
) -> AppResult<()> {
    direction_from_contract(contract)?;
    let already_tracked = has_tracked_option_symbol(orders, &position.symbol);
    let (adoptable, reason) = adoptable_position_state(position, contract, already_tracked);
    if adoptable {
        Ok(())
    } else {
        Err(AppError::RiskRejected(crate::msg!(
            "backend.cannotTakeOver",
            p0 = position.symbol,
            p1 = reason
        )))
    }
}

/// 判断当前 Binance 持仓是否可接管，并给出可展示原因。
pub(super) fn adoptable_position_state(
    position: &OptionPosition,
    contract: &OptionContract,
    already_tracked: bool,
) -> (bool, Message) {
    if already_tracked {
        return (false, crate::msg!("backend.alreadyTrackedLocally"));
    }
    if contract.underlying != "BTCUSDT" {
        return (false, crate::msg!("backend.onlyBtcUsdtOptionsAreSupported"));
    }
    if !is_short_position(position) {
        return (
            false,
            crate::msg!("backend.onlyShortOptionObligationsCanBeTaken"),
        );
    }
    if position.quantity <= Decimal::ZERO {
        return (false, crate::msg!("backend.positionQuantityIsZero"));
    }
    if position.entry_price <= Decimal::ZERO {
        return (
            false,
            crate::msg!("backend.binancePositionHasNoValidEntryPrice"),
        );
    }
    if Utc::now() >= contract.expiry_at {
        return (false, crate::msg!("backend.optionHasExpired"));
    }
    (true, crate::msg!("backend.availableToTakeOver"))
}

/// 判断同一合约是否已经存在未终态的本地跟踪订单。
pub(super) fn has_tracked_option_symbol(orders: &[StrategyOrder], option_symbol: &str) -> bool {
    orders.iter().any(|order| {
        order.option_symbol == option_symbol
            && !matches!(
                order.status,
                StrategyStatus::Settled
                    | StrategyStatus::Rejected
                    | StrategyStatus::Expired
                    | StrategyStatus::Cancelled
            )
    })
}

/// 从 Binance 合约方向映射成本地策略方向。
pub(super) fn direction_from_contract(contract: &OptionContract) -> AppResult<StrategyDirection> {
    match contract.side.as_str() {
        "PUT" => Ok(StrategyDirection::SellPut),
        "CALL" => Ok(StrategyDirection::SellCall),
        side => Err(AppError::RiskRejected(crate::msg!(
            "backend.unsupportedBinanceOptionSide",
            p0 = side
        ))),
    }
}

/// 仅接管卖方义务仓，避免把买方权利仓误纳入到期补齐流程。
pub(super) fn is_short_position(position: &OptionPosition) -> bool {
    matches!(
        position.side.to_ascii_uppercase().as_str(),
        "SHORT" | "SELL" | "SHORT_POSITION"
    )
}
