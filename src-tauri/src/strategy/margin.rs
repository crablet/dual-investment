//! 保证金占用约束。
//!
//! 备资与归集共用占用目标；额外容差仅供最终下单校验使用。
//! 调整参数时同步更新 docs/trading-safety.md。

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// 默认保证金占用率风控上限。
pub const DEFAULT_MAX_MARGIN_USAGE: Decimal = dec!(90);

/// 最终下单风控允许账户快照相对配置上限上浮的百分点。
pub const DEFAULT_MARGIN_USAGE_TOLERANCE_POINTS: Decimal = dec!(5);
