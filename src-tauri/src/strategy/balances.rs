//! 账户资产和收益快照派生。
//!
//! 这里把期权保证金、现货和 Simple Earn 的真实账户快照转换成前端展示行，并按已保存订单
//! 派生收益报告。无法估值的资产保留数量事实，但不参与 USDT 汇总。

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use rust_decimal::Decimal;

use super::math::max_decimal;
use crate::domain::{
    BalanceRow, OptionAccountSnapshot, PnlReport, SimpleEarnSnapshot, SpotAccountSnapshot,
    StrategyDirection, StrategyOrder,
};

struct BalanceValuation {
    usdt_value: Decimal,
    known: bool,
}

/// 构造空收益报告。
fn empty_pnl_report() -> PnlReport {
    PnlReport {
        cumulative: Decimal::ZERO,
        today: Decimal::ZERO,
        option_premium: Decimal::ZERO,
        floating_spot: Decimal::ZERO,
    }
}

/// 从期权保证金账户构造前端资产行。
pub fn build_balances(
    account: &OptionAccountSnapshot,
    spot_prices: &HashMap<String, Decimal>,
) -> Vec<BalanceRow> {
    account
        .assets
        .iter()
        .map(|asset| {
            let valuation = if asset.asset == "USDT" {
                BalanceValuation {
                    usdt_value: asset.adjusted_equity,
                    known: true,
                }
            } else {
                asset_usdt_value(&asset.asset, asset.margin_balance, spot_prices)
            };
            BalanceRow {
                asset: asset.asset.clone(),
                source: "期权保证金".to_string(),
                free: asset.available,
                locked: max_decimal(asset.margin_balance - asset.available, Decimal::ZERO),
                usdt_value: valuation.usdt_value,
                usdt_value_known: valuation.known,
            }
        })
        .collect()
}

/// 从现货账户构造前端资产行。
///
/// 现货账户可能包含 Simple Earn 的 `LD{asset}` 镜像。只有同轮 Simple Earn 快照中
/// 存在对应正余额资产时才过滤，避免重复展示，也避免按前缀猜测而隐藏真实现货资产。
/// 该去重只用于展示；资金流程仍直接读取原始账户快照。
pub fn build_spot_balances(
    account: &SpotAccountSnapshot,
    earn: &SimpleEarnSnapshot,
    spot_prices: &HashMap<String, Decimal>,
) -> Vec<BalanceRow> {
    let simple_earn_ledger_assets = simple_earn_ledger_assets(earn);
    account
        .balances
        .iter()
        .filter(|asset| {
            (asset.free > Decimal::ZERO || asset.locked > Decimal::ZERO)
                && !simple_earn_ledger_assets.contains(&asset.asset)
        })
        .map(|asset| {
            let valuation = asset_usdt_value(&asset.asset, asset.free + asset.locked, spot_prices);
            BalanceRow {
                asset: asset.asset.clone(),
                source: "现货".to_string(),
                free: asset.free,
                locked: asset.locked,
                usdt_value: valuation.usdt_value,
                usdt_value_known: valuation.known,
            }
        })
        .collect()
}

/// 从 Simple Earn 活期/定期持仓构造前端资产行。
pub fn build_simple_earn_balances(
    earn: &SimpleEarnSnapshot,
    spot_prices: &HashMap<String, Decimal>,
) -> Vec<BalanceRow> {
    // 同一资产可能同时存在多个 Simple Earn 产品，展示时按资产和来源聚合，保留来源口径。
    let mut grouped: HashMap<(String, String), Decimal> = HashMap::new();
    for balance in &earn.balances {
        if balance.amount <= Decimal::ZERO {
            continue;
        }
        *grouped
            .entry((balance.asset.clone(), balance.source.clone()))
            .or_default() += balance.amount;
    }

    grouped
        .into_iter()
        .map(|((asset, source), amount)| {
            let valuation = asset_usdt_value(&asset, amount, spot_prices);
            BalanceRow {
                asset,
                source,
                free: amount,
                locked: Decimal::ZERO,
                usdt_value: valuation.usdt_value,
                usdt_value_known: valuation.known,
            }
        })
        .collect()
}

/// 按原始 ticker 计算 USDT 估值。
fn asset_usdt_value(
    asset: &str,
    quantity: Decimal,
    spot_prices: &HashMap<String, Decimal>,
) -> BalanceValuation {
    if quantity <= Decimal::ZERO {
        return BalanceValuation {
            usdt_value: Decimal::ZERO,
            known: true,
        };
    }
    if let Some(price) = asset_usdt_price(asset, spot_prices) {
        BalanceValuation {
            usdt_value: quantity * price,
            known: true,
        }
    } else {
        BalanceValuation {
            usdt_value: Decimal::ZERO,
            known: false,
        }
    }
}

fn asset_usdt_price(asset: &str, spot_prices: &HashMap<String, Decimal>) -> Option<Decimal> {
    if asset == "USDT" {
        return Some(Decimal::ONE);
    }

    let usdt_symbol = format!("{asset}USDT");
    if let Some(price) = positive_price(spot_prices.get(&usdt_symbol)) {
        return Some(price);
    }

    // 没有直接 USDT 交易对时，尝试通过 BTCUSDT 中转估值；仍失败则标记为暂无估值。
    let btc_symbol = format!("{asset}BTC");
    let asset_btc = positive_price(spot_prices.get(&btc_symbol))?;
    let btc_usdt = positive_price(spot_prices.get("BTCUSDT"))?;
    Some(asset_btc * btc_usdt)
}

fn positive_price(price: Option<&Decimal>) -> Option<Decimal> {
    // Binance 偶发返回 0 或缺失价格时，不把它当成有效估值来源。
    price.copied().filter(|value| *value > Decimal::ZERO)
}

fn simple_earn_ledger_assets(earn: &SimpleEarnSnapshot) -> HashSet<String> {
    // 只依据同一次 Binance Simple Earn 快照识别镜像，避免按长度猜测而漏掉 LDU，
    // 也避免误伤名称本身以 LD 开头的普通现货资产。
    earn.balances
        .iter()
        .filter(|balance| balance.amount > Decimal::ZERO)
        .map(|balance| format!("LD{}", balance.asset))
        .collect()
}

/// 从真实订单和现货价格派生收益报告。
///
/// 权利金和现货补齐浮动盈亏都来自已保存订单字段；没有订单时返回空报告。
pub fn pnl_report_from_orders_with_spot(
    orders: &[StrategyOrder],
    spot_price: Option<Decimal>,
) -> PnlReport {
    if orders.is_empty() {
        return empty_pnl_report();
    }

    let option_premium: Decimal = orders.iter().map(|order| order.premium).sum();
    let today = today_option_premium(orders);
    let floating_spot = spot_price.map_or(Decimal::ZERO, |price| {
        orders
            .iter()
            .filter(|order| {
                order.direction == StrategyDirection::SellPut
                    && order.spot_filled_quantity > Decimal::ZERO
                    && order.spot_avg_price > Decimal::ZERO
            })
            .map(|order| (price - order.spot_avg_price) * order.spot_filled_quantity)
            .sum::<Decimal>()
            .round_dp(2)
    });
    let cumulative = option_premium + floating_spot;
    PnlReport {
        cumulative,
        today,
        option_premium,
        floating_spot,
    }
}

/// 统计当天订单权利金。
fn today_option_premium(orders: &[StrategyOrder]) -> Decimal {
    // 收益报告的“今日”使用后端 UTC 日期，与订单创建时间字段保持同一时间基准。
    let today = Utc::now().date_naive();
    orders
        .iter()
        .filter(|order| order.created_at.date_naive() == today)
        .map(|order| order.premium)
        .sum::<Decimal>()
}
