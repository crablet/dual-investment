//! 风控快照派生。
//!
//! 风险值从 Binance 期权账户快照和 Rust 侧规则计算，前端只展示结果。默认保证金占用
//! 上限在这里定义，调整时需要同步更新 README/AGENTS 中的业务约束。

use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use super::math::percent_of;
use crate::domain::{OptionAccountSnapshot, RiskAlert, RiskSeverity, RiskSnapshot};

/// 默认保证金占用率风控上限。
pub const DEFAULT_MAX_MARGIN_USAGE: Decimal = dec!(60);

/// 构造未连接状态下的空风险快照。
pub fn empty_risk_snapshot() -> RiskSnapshot {
    RiskSnapshot {
        score: 0,
        level: "未同步".to_string(),
        margin_usage: Decimal::ZERO,
        exercise_risk: Decimal::ZERO,
        alerts: Vec::new(),
    }
}

/// 从期权账户保证金信息派生风险快照。
pub fn risk_snapshot_from_account(account: &OptionAccountSnapshot) -> RiskSnapshot {
    let usdt = account.asset("USDT");
    let margin_usage = usdt
        .and_then(|asset| percent_of(asset.initial_margin, asset.adjusted_equity))
        .unwrap_or(Decimal::ZERO);
    let maintenance_usage = usdt
        .and_then(|asset| percent_of(asset.maint_margin, asset.adjusted_equity))
        .unwrap_or(Decimal::ZERO);
    let level = if margin_usage >= dec!(80) {
        "高风险"
    } else if margin_usage >= dec!(50) {
        "关注"
    } else {
        "健康"
    };
    let score = if margin_usage >= dec!(80) {
        80
    } else if margin_usage >= dec!(50) {
        55
    } else if margin_usage > Decimal::ZERO {
        25
    } else {
        0
    };
    let alerts = if margin_usage >= DEFAULT_MAX_MARGIN_USAGE {
        vec![RiskAlert {
            id: "margin-usage".to_string(),
            severity: if margin_usage >= dec!(80) {
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
    };

    RiskSnapshot {
        score,
        level: level.to_string(),
        margin_usage,
        exercise_risk: maintenance_usage,
        alerts,
    }
}
