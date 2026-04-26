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
        return Err(AppError::Config("请选择需要接管的期权持仓".to_string()));
    }
    Ok(normalized)
}

pub(super) fn adoptable_position_view(
    position: &OptionPosition,
    contract: &OptionContract,
    orders: &[StrategyOrder],
) -> AppResult<AdoptableOptionPositionView> {
    let direction = direction_from_contract(contract)?;
    let already_tracked = has_tracked_option_symbol(orders, &position.symbol);
    let (adoptable, reason) = adoptable_position_state(position, contract, already_tracked);
    Ok(AdoptableOptionPositionView {
        id: position.symbol.clone(),
        symbol: position.symbol.clone(),
        direction,
        expiry: contract.expiry,
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
        Err(AppError::RiskRejected(format!(
            "{} 不能接管：{}",
            position.symbol, reason
        )))
    }
}

pub(super) fn adoptable_position_state(
    position: &OptionPosition,
    contract: &OptionContract,
    already_tracked: bool,
) -> (bool, String) {
    if already_tracked {
        return (false, "本地已有跟踪记录".to_string());
    }
    if contract.underlying != "BTCUSDT" {
        return (false, "仅支持 BTC/USDT 期权".to_string());
    }
    if !is_short_position(position) {
        return (false, "仅支持接管卖出期权义务仓".to_string());
    }
    if position.quantity <= Decimal::ZERO {
        return (false, "持仓数量为 0".to_string());
    }
    if position.entry_price <= Decimal::ZERO {
        return (false, "Binance 持仓缺少有效入场价".to_string());
    }
    if Utc::now() >= contract.expiry_at {
        return (false, "期权已到期".to_string());
    }
    (true, "可接管".to_string())
}

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

pub(super) fn direction_from_contract(contract: &OptionContract) -> AppResult<StrategyDirection> {
    match contract.side.as_str() {
        "PUT" => Ok(StrategyDirection::SellPut),
        "CALL" => Ok(StrategyDirection::SellCall),
        side => Err(AppError::RiskRejected(format!(
            "不支持的 Binance 期权方向：{side}"
        ))),
    }
}

pub(super) fn is_short_position(position: &OptionPosition) -> bool {
    matches!(
        position.side.to_ascii_uppercase().as_str(),
        "SHORT" | "SELL" | "SHORT_POSITION"
    )
}
