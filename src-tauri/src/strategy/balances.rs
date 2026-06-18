//! 账户资产和收益快照派生。
//!
//! 这里把期权保证金、现货和 Simple Earn 的真实账户快照转换成前端展示行，并按已保存订单
//! 汇总本机实际成交的卖出权利金。净仓与订单成交历史不能相互累加为同一义务。
//! 无法估值的资产保留数量事实，但不参与 USDT 汇总。

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Local, Utc};
use rust_decimal::Decimal;

use crate::domain::{
    AccountSnapshot, AssetBalance, BalanceRow, OptionAccountSnapshot, PnlReport, PortfolioSummary,
    PositionStatus, SimpleEarnSnapshot, SpotAccountSnapshot, StrategyDirection, StrategyOrder,
    TrackedPosition,
};

use super::orders::filled_premium;

struct BalanceValuation {
    usdt_value: Decimal,
    known: bool,
}

/// 按资产聚合各账户来源，缺少估值的整组不计入总额，和前端比例图保持相同口径。
pub fn account_snapshot_from_balances(balances: Vec<BalanceRow>) -> AccountSnapshot {
    let mut grouped = BTreeMap::<String, Vec<BalanceRow>>::new();
    let mut available_margin = Decimal::ZERO;
    for mut balance in balances {
        let asset = balance.asset.trim().to_ascii_uppercase();
        balance.asset.clone_from(&asset);
        if asset == "USDT" && balance.source == "optionMargin" {
            available_margin += balance.free;
        }
        grouped.entry(asset).or_default().push(balance);
    }

    let balances = grouped
        .into_iter()
        .map(|(asset, mut details)| {
            details.sort_by(|left, right| left.source.cmp(&right.source));
            AssetBalance {
                asset,
                free: details.iter().map(|row| row.free).sum(),
                locked: details.iter().map(|row| row.locked).sum(),
                usdt_value: details.iter().map(|row| row.usdt_value).sum(),
                usdt_value_known: details.iter().all(|row| row.usdt_value_known),
                details,
            }
        })
        .collect::<Vec<_>>();
    AccountSnapshot {
        total_usdt_value: balances
            .iter()
            .filter(|balance| balance.usdt_value_known)
            .map(|balance| balance.usdt_value)
            .sum(),
        available_margin,
        unpriced_asset_count: balances
            .iter()
            .filter(|balance| !balance.usdt_value_known)
            .count(),
        balances,
        ..AccountSnapshot::default()
    }
}

/// 当前义务只取未到期的卖方净仓，合约张数按 unit 折算为 BTC。
pub fn portfolio_summary_from_positions(positions: &[TrackedPosition]) -> PortfolioSummary {
    let mut summary = PortfolioSummary::default();
    let now = Utc::now();
    for position in positions
        .iter()
        .filter(|position| position.status == PositionStatus::Open && position.expiry_at > now)
    {
        let base_quantity = position.quantity * position.unit;
        match position.direction {
            StrategyDirection::SellPut => {
                summary.put_count += 1;
                summary.put_quantity += base_quantity;
                summary.put_notional += position.strike * base_quantity;
            }
            StrategyDirection::SellCall => {
                summary.call_count += 1;
                summary.call_quantity += base_quantity;
            }
        }
    }
    summary
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
                source: "optionMargin".to_string(),
                free: asset.available,
                locked: (asset.margin_balance - asset.available).max(Decimal::ZERO),
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
                source: "spot".to_string(),
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

/// 汇总本机实际成交的卖出权利金，按日期和方向拆分。
///
/// 净仓均价不能倒推历史权利金。
pub fn pnl_report(orders: &[StrategyOrder]) -> PnlReport {
    let mut sell_put_premium = Decimal::ZERO;
    let mut sell_call_premium = Decimal::ZERO;
    let mut today_recorded_premium = Decimal::ZERO;
    let today = Local::now().date_naive();
    for order in orders {
        let premium = filled_premium(order);
        match order.direction {
            StrategyDirection::SellPut => sell_put_premium += premium,
            StrategyDirection::SellCall => sell_call_premium += premium,
        }
        if order.created_at.with_timezone(&Local).date_naive() == today {
            today_recorded_premium += premium;
        }
    }
    PnlReport {
        recorded_premium: sell_put_premium + sell_call_premium,
        today_recorded_premium,
        sell_put_premium,
        sell_call_premium,
    }
}
