//! 策略计算与风控派生逻辑。
//!
//! 这个模块只处理纯业务计算：候选过滤、收益估算、风险提醒和订单状态归一化。
//! 它不直接访问 Binance、不保存数据，也不读取系统凭据。

mod balances;
mod candidates;
mod math;
mod orders;
mod risk;

pub use balances::{
    build_balances, build_simple_earn_balances, build_spot_balances,
    pnl_report_from_orders_with_spot,
};
pub use candidates::{
    CandidateBuildInputs, build_candidates, decimal_matches_step,
    option_sell_taker_limit_price_for_quantity, required_margin, status_from_exchange,
};
pub use orders::{annualized_from_order_terms, order_from_adopted_position, order_from_request};
pub use risk::{
    DEFAULT_MARGIN_USAGE_TOLERANCE_POINTS, DEFAULT_MAX_MARGIN_USAGE, risk_alerts_from_account,
};
