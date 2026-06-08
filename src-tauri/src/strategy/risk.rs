//! 风控提醒派生。
//!
//! 风险提醒从 Binance 期权账户快照和 Rust 侧规则计算，前端只展示结果。默认保证金占用
//! 上限和最终下单校验容差在这里定义，调整时需要同步更新 README/AGENTS 中的业务约束。

use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use super::math::percent_of;
use crate::domain::{OptionAccountSnapshot, RiskAlert, RiskSeverity};

/// 默认保证金占用率风控上限。
pub const DEFAULT_MAX_MARGIN_USAGE: Decimal = dec!(90);

/// 最终下单风控允许账户快照相对配置上限上浮的百分点。
pub const DEFAULT_MARGIN_USAGE_TOLERANCE_POINTS: Decimal = dec!(5);

/// 从期权账户保证金信息派生需要展示的风险提醒。
pub fn risk_alerts_from_account(account: &OptionAccountSnapshot) -> Vec<RiskAlert> {
    let usdt = account.asset("USDT");
    let margin_usage = usdt
        .and_then(|asset| percent_of(asset.initial_margin, asset.adjusted_equity))
        .unwrap_or(Decimal::ZERO);
    if margin_usage >= DEFAULT_MAX_MARGIN_USAGE {
        vec![RiskAlert {
            id: "margin-usage".to_string(),
            severity: if margin_usage >= DEFAULT_MAX_MARGIN_USAGE {
                RiskSeverity::High
            } else {
                RiskSeverity::Medium
            },
            title: "保证金占用偏高".to_string(),
            message: format!("当前期权保证金占用约 {}%", margin_usage.round_dp(2)),
            created_at: Utc::now(),
        }]
    } else {
        Vec::new()
    }
}
