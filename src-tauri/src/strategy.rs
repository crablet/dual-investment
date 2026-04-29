//! 策略计算与风控派生逻辑。
//!
//! 这个模块只处理纯业务计算：候选过滤、收益估算、风险快照、订单状态归一化和
//! 自动复投默认规则。它不直接访问 Binance、不保存数据，也不读取系统凭据。

mod balances;
mod candidates;
mod math;
mod orders;
mod risk;
mod rules;

pub use balances::{
    build_balances, build_simple_earn_balances, build_spot_balances,
    pnl_report_from_orders_with_spot,
};
pub use candidates::{
    build_candidates, decimal_matches_step, option_sell_limit_price, required_margin,
    status_from_exchange,
};
pub use orders::{annualized_from_order_terms, order_from_adopted_position, order_from_request};
pub use risk::{DEFAULT_MAX_MARGIN_USAGE, empty_risk_snapshot, risk_snapshot_from_account};
pub use rules::default_auto_rule;
