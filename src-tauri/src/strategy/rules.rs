//! 自动复投规则默认值。
//!
//! 本模块只负责生成默认配置。规则是否允许自动下单、是否存在待恢复阻断，以及资金/风控
//! 检查都在应用状态编排和 Rust 风控中完成。

use rust_decimal_macros::dec;

use crate::domain::AutoInvestmentRule;

/// 创建默认自动复投规则。
///
/// `enabled` 由调用方传入；应用启动时会强制关闭自动执行，防止重启后直接自动下单。
pub fn default_auto_rule(enabled: bool) -> AutoInvestmentRule {
    AutoInvestmentRule {
        enabled,
        schedule: "每天 11:30".to_string(),
        quantity: dec!(0.01),
        min_annualized: dec!(15),
        max_delta_abs: dec!(0.30),
        max_iv_rank: dec!(60),
        min_days_to_expiry: 7,
        max_days_to_expiry: 45,
        include_btc: true,
        exclude_same_direction: true,
    }
}
