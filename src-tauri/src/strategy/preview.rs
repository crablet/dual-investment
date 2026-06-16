//! 手动下单复核使用的纯 Decimal 预览。这里不读取账户、不查询合约，也不发送请求。

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::domain::{
    AppError, AppResult, StrategyDirection, StrategyPreview, StrategyPreviewRequest,
};

/// 预览只校验输入的基本合法性，真实下单仍须核对 Binance 合约和账户约束。
pub fn preview_strategy(request: &StrategyPreviewRequest) -> AppResult<StrategyPreview> {
    if request.quantity < dec!(0.01) {
        return Err(AppError::RiskRejected(crate::msg!(
            "backend.previewQuantityMustBeAtLeastContracts"
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
    let premium = request
        .limit_price
        .checked_mul(request.quantity)
        .ok_or_else(preview_overflow)?
        .round_dp(2);
    let breakeven = match request.direction {
        StrategyDirection::SellPut => request.strike.checked_sub(request.limit_price),
        StrategyDirection::SellCall => request.strike.checked_add(request.limit_price),
    }
    .ok_or_else(preview_overflow)?;

    Ok(StrategyPreview { premium, breakeven })
}

fn preview_overflow() -> AppError {
    AppError::RiskRejected(crate::msg!(
        "backend.previewAmountExceedsTheSupportedCalculationRange"
    ))
}
