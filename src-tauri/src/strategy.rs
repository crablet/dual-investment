//! 策略计算与保证金参数。
//!
//! 这个模块只处理纯业务计算：候选过滤、保证金估算、收益估算和订单状态归一化。
//! 它不直接访问 Binance、不保存数据，也不读取系统凭据。

mod balances;
mod candidates;
mod margin;
mod math;
mod orders;
mod preview;

pub use balances::{
    account_snapshot_from_balances, build_balances, build_simple_earn_balances,
    build_spot_balances, pnl_report, portfolio_summary_from_positions,
};
pub use candidates::{
    CandidateBuildInputs, build_candidates, decimal_matches_step,
    option_sell_taker_limit_price_for_quantity, required_margin, status_from_exchange,
};
pub use margin::{DEFAULT_MARGIN_USAGE_TOLERANCE_POINTS, DEFAULT_MAX_MARGIN_USAGE};
pub use orders::{order_from_request, refresh_order_metrics};
pub use preview::preview_strategy;
