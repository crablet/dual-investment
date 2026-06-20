//! 候选筛选、保证金、收益和订单状态计算，不访问网络、数据库或凭据。

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
