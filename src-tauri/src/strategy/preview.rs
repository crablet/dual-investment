//! 手动下单复核使用的纯 Decimal 预览。这里不读取账户、不查询合约，也不发送请求。

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::domain::{
    AppError, AppResult, StrategyDirection, StrategyPreview, StrategyPreviewRequest,
};

/// 预览只校验输入的基本合法性，真实下单仍须核对 Binance 合约和账户约束。
pub fn preview_strategy(request: &StrategyPreviewRequest) -> AppResult<StrategyPreview> {
    if request.quantity < dec!(0.01) {
        return Err(AppError::RiskRejected(
            "预览数量不能小于 0.01 张".to_string(),
        ));
    }
    if request.strike <= Decimal::ZERO {
        return Err(AppError::RiskRejected("执行价必须大于 0".to_string()));
    }
    if request.limit_price <= Decimal::ZERO {
        return Err(AppError::RiskRejected("限价必须大于 0".to_string()));
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
    AppError::RiskRejected("预览金额超出可计算范围".to_string())
}
