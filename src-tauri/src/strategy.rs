//! 策略计算与风控派生逻辑。
//!
//! 这个模块只处理纯业务计算：候选过滤、收益估算、风险快照、订单状态归一化和
//! 自动复投默认规则。它不直接访问 Binance、不保存数据，也不读取系统凭据。

use std::collections::HashMap;

use chrono::Utc;
use rust_decimal::{Decimal, RoundingStrategy};
use rust_decimal_macros::dec;
use uuid::Uuid;

use crate::domain::{
    AutoInvestmentRule, BalanceRow, CandidateScanDiagnostics, CandidateScanResult, ChartPoint,
    ExchangeOrderAck, OptionAccountSnapshot, OptionContract, OptionMarkPrice, PnlReport,
    RecommendationLevel, RiskAlert, RiskRuleSet, RiskSeverity, RiskSnapshot, SimpleEarnSnapshot,
    SpotAccountSnapshot, StrategyCandidate, StrategyDirection, StrategyOrder, StrategyPreview,
    StrategyRequest, StrategyStatus,
};

const BROWSE_MIN_DAYS_TO_EXPIRY: i64 = 1;
const BROWSE_MAX_DAYS_TO_EXPIRY: i64 = 90;

struct BalanceValuation {
    usdt_value: Decimal,
    known: bool,
}

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

/// 构造空收益报告。
pub fn empty_pnl_report() -> PnlReport {
    PnlReport {
        cumulative: Decimal::ZERO,
        today: Decimal::ZERO,
        option_premium: Decimal::ZERO,
        realized_spot: Decimal::ZERO,
        floating_spot: Decimal::ZERO,
        points: Vec::new(),
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
/// 现货账户里的 `LD*` 是 Simple Earn 镜像，必须忽略，避免和理财接口重复统计。
pub fn build_spot_balances(
    account: &SpotAccountSnapshot,
    spot_prices: &HashMap<String, Decimal>,
) -> Vec<BalanceRow> {
    account
        .balances
        .iter()
        .filter(|asset| {
            (asset.free > Decimal::ZERO || asset.locked > Decimal::ZERO)
                && !is_simple_earn_ledger_asset(&asset.asset)
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

    let btc_symbol = format!("{asset}BTC");
    let asset_btc = positive_price(spot_prices.get(&btc_symbol))?;
    let btc_usdt = positive_price(spot_prices.get("BTCUSDT"))?;
    Some(asset_btc * btc_usdt)
}

fn positive_price(price: Option<&Decimal>) -> Option<Decimal> {
    price.copied().filter(|value| *value > Decimal::ZERO)
}

fn is_simple_earn_ledger_asset(asset: &str) -> bool {
    if asset.starts_with("LD") && asset.len() > 4 {
        return true;
    }
    false
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
        realized_spot: Decimal::ZERO,
        floating_spot,
        points: Vec::<ChartPoint>::new(),
    }
}

/// 构造未连接状态下的空风险快照。
pub fn empty_risk_snapshot() -> RiskSnapshot {
    RiskSnapshot {
        score: 0,
        level: "未同步".to_string(),
        margin_usage: Decimal::ZERO,
        exercise_risk: Decimal::ZERO,
        rules: default_risk_rules(),
        alerts: Vec::new(),
    }
}

/// 从期权账户保证金信息派生风险快照。
pub fn risk_snapshot_from_account(account: &OptionAccountSnapshot) -> RiskSnapshot {
    let rules = default_risk_rules();
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
    let alerts = if margin_usage >= rules.max_margin_usage {
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
        rules,
        alerts,
    }
}

/// 根据 Binance 合约、mark price、账户和规则构造候选列表。
///
/// 候选可见范围固定为到期 1-90 天；自动复投规则只影响推荐等级，不决定候选是否可见。
/// 方向过滤必须使用 BTCUSDT 现货价：低买只保留现货价下方 Put，高卖只保留现货价上方 Call。
#[allow(clippy::too_many_lines)]
pub fn build_candidates(
    contracts: &[OptionContract],
    marks: &[OptionMarkPrice],
    rule: &AutoInvestmentRule,
    account: &OptionAccountSnapshot,
    orders: &[StrategyOrder],
    spot_price: Decimal,
    funding_available_usdt: Decimal,
) -> CandidateScanResult {
    let mut diagnostics = CandidateScanDiagnostics {
        raw_contracts: contracts.len(),
        raw_marks: marks.len(),
        browse_min_days: BROWSE_MIN_DAYS_TO_EXPIRY,
        browse_max_days: BROWSE_MAX_DAYS_TO_EXPIRY,
        ..CandidateScanDiagnostics::default()
    };

    if spot_price <= Decimal::ZERO {
        return CandidateScanResult {
            candidates: Vec::new(),
            diagnostics,
        };
    }

    let mark_by_symbol = marks
        .iter()
        .map(|mark| (mark.symbol.as_str(), mark))
        .collect::<HashMap<_, _>>();
    let has_open_direction = |direction| {
        orders.iter().any(|order| {
            order.direction == direction
                && matches!(
                    order.status,
                    StrategyStatus::Pending
                        | StrategyStatus::Running
                        | StrategyStatus::PartiallyFilled
                )
        })
    };

    let mut candidates = Vec::new();

    for contract in contracts {
        if contract.underlying != "BTCUSDT" || contract.quote_asset != "USDT" {
            continue;
        }
        diagnostics.btc_contracts += 1;

        if contract.status != "TRADING" {
            continue;
        }
        diagnostics.trading_contracts += 1;

        if rule.quantity < contract.min_qty || rule.quantity > contract.max_qty {
            continue;
        }
        diagnostics.quantity_matched += 1;

        if let Some(candidate) = (|| {
            let direction = direction_from_option_side(&contract.side)?;
            // 候选方向必须先经过现货价过滤，避免把价外方向错误展示成双币赢机会。
            if !strike_matches_direction(contract.strike_price, direction, spot_price) {
                return None;
            }
            diagnostics.direction_matched += 1;

            let days_to_expiry = days_until_expiry(contract.expiry_at);
            if !(BROWSE_MIN_DAYS_TO_EXPIRY..=BROWSE_MAX_DAYS_TO_EXPIRY).contains(&days_to_expiry) {
                return None;
            }
            diagnostics.expiry_matched += 1;

            let mark = mark_by_symbol.get(contract.symbol.as_str()).copied()?;
            if mark.mark_price <= Decimal::ZERO {
                return None;
            }
            diagnostics.mark_matched += 1;

            let delta_abs = abs_decimal(mark.delta);
            let iv_rank = (mark.mark_iv * dec!(100)).round_dp(2);
            let quantity = rule.quantity;
            // 候选展示使用 Binance mark price；真实下单会在编排层按 tick size 对齐。
            let limit_price = mark.mark_price.round_dp(contract.price_scale);
            let premium = (limit_price * quantity).round_dp(contract.price_scale);
            let annualized = estimate_annualized(
                premium,
                contract.strike_price,
                quantity * contract.unit,
                days_to_expiry.max(1),
            );
            let exercise_probability = (delta_abs * dec!(100)).round_dp(2);
            let strike_distance_percent =
                calculate_strike_distance_percent(contract.strike_price, spot_price);
            let breakeven = match direction {
                StrategyDirection::SellPut => contract.strike_price - limit_price,
                StrategyDirection::SellCall => contract.strike_price + limit_price,
            };
            let required_margin = required_margin(contract, direction, quantity);
            let available_margin = account
                .asset("USDT")
                .map_or(Decimal::ZERO, |asset| asset.available)
                + funding_available_usdt;
            // 规则命中只影响推荐等级；不命中规则但仍有保证金的候选会保留为 Watch，方便排查。
            let passes_rule = rule.include_btc
                && (!rule.exclude_same_direction || !has_open_direction(direction))
                && days_to_expiry >= rule.min_days_to_expiry
                && days_to_expiry <= rule.max_days_to_expiry
                && annualized >= rule.min_annualized
                && delta_abs <= rule.max_delta_abs
                && iv_rank <= rule.max_iv_rank
                && available_margin >= required_margin;
            let recommendation = if passes_rule {
                RecommendationLevel::Recommended
            } else if annualized >= Decimal::ZERO && available_margin >= required_margin {
                RecommendationLevel::Watch
            } else {
                RecommendationLevel::Reject
            };
            let reason = if passes_rule {
                format!(
                    "来自 Binance 合约和 mark price，满足当前自动规则，mark IV 代理值 {iv_rank}%"
                )
            } else if available_margin < required_margin {
                "可用保证金不足，暂不建议下单".to_string()
            } else {
                format!(
                    "来自 Binance 合约和 mark price，未完全满足当前自动规则，mark IV 代理值 {iv_rank}%"
                )
            };

            Some(StrategyCandidate {
                id: format!("{}-{}", contract.symbol, Utc::now().timestamp()),
                symbol: contract.underlying.clone(),
                option_symbol: contract.symbol.clone(),
                direction,
                expiry: contract.expiry,
                expiry_at: contract.expiry_at,
                strike: contract.strike_price,
                quantity,
                limit_price,
                premium,
                annualized,
                delta_abs,
                iv_rank,
                exercise_probability,
                breakeven,
                spot_price,
                strike_distance_percent,
                recommendation,
                reason,
            })
        })() {
            candidates.push(candidate);
        }
    }

    sort_candidates(&mut candidates);
    diagnostics.returned_candidates = candidates.len();
    diagnostics.returned_sell_put = candidates
        .iter()
        .filter(|candidate| candidate.direction == StrategyDirection::SellPut)
        .count();
    diagnostics.returned_sell_call = candidates
        .iter()
        .filter(|candidate| candidate.direction == StrategyDirection::SellCall)
        .count();

    CandidateScanResult {
        candidates,
        diagnostics,
    }
}

/// 基于下单请求构造本地预览。
///
/// 预览用于 UI 展示和订单字段初始计算，真实下单仍必须经过合约解析、风控和 Binance 回报。
pub fn preview_strategy(request: &StrategyRequest) -> StrategyPreview {
    let premium = (request.limit_price * request.quantity).round_dp(2);
    let breakeven = match request.direction {
        StrategyDirection::SellPut => request.strike - request.limit_price,
        StrategyDirection::SellCall => request.strike + request.limit_price,
    };
    let expiry_at = request.expiry_at;
    let expiry_days = days_until_expiry(expiry_at).max(1);
    let annualized = estimate_annualized(premium, request.strike, request.quantity, expiry_days);
    let candidate = StrategyCandidate {
        id: request
            .candidate_id
            .clone()
            .unwrap_or_else(|| format!("manual-{}", Uuid::new_v4())),
        symbol: request.symbol.clone(),
        option_symbol: request.option_symbol.clone().unwrap_or_else(|| {
            format!(
                "BTC-{}-{}-{}",
                request.expiry.format("%y%m%d"),
                request.strike,
                match request.direction {
                    StrategyDirection::SellPut => "P",
                    StrategyDirection::SellCall => "C",
                }
            )
        }),
        direction: request.direction,
        expiry: request.expiry,
        expiry_at,
        strike: request.strike,
        quantity: request.quantity,
        limit_price: request.limit_price,
        premium,
        annualized,
        delta_abs: Decimal::ZERO,
        iv_rank: Decimal::ZERO,
        exercise_probability: Decimal::ZERO,
        breakeven,
        spot_price: Decimal::ZERO,
        strike_distance_percent: Decimal::ZERO,
        recommendation: RecommendationLevel::Watch,
        reason: "基于用户输入限价的订单预览".to_string(),
    };

    let reserved_asset = match request.direction {
        StrategyDirection::SellPut => request.strike * request.quantity,
        StrategyDirection::SellCall => request.quantity,
    };
    let protected_limit_price = request.limit_price;
    let action = match request.direction {
        StrategyDirection::SellPut => "到期 ITM 后自动买入 BTC，完成补齐",
        StrategyDirection::SellCall => "到期 ITM 后自动卖出 BTC，完成补齐",
    };

    StrategyPreview {
        candidate,
        max_profit: premium,
        reserved_asset,
        protected_limit_price,
        summary: vec![
            format!("快速成交：以用户输入限价尝试 IOC 成交"),
            format!("{action}"),
            format!("预计年化 {}%，数量 {} 张", annualized, request.quantity),
        ],
    }
}

/// 从下单请求和 Binance 订单回报构造本地订单。
///
/// 成交数量、均价、手续费和交易所状态以 `ack` 为准；请求只提供策略方向和用户确认的参数。
pub fn order_from_request(request: &StrategyRequest, ack: &ExchangeOrderAck) -> StrategyOrder {
    let preview = preview_strategy(request);
    let now = ack.update_time.or(ack.create_time).unwrap_or_else(Utc::now);
    let avg_price = if ack.avg_price > Decimal::ZERO {
        ack.avg_price
    } else {
        ack.price
    };
    let premium = (avg_price * ack.executed_qty).round_dp(2);
    let status = status_from_exchange(&ack.status, ack.executed_qty, ack.quantity);
    StrategyOrder {
        id: format!("LIVE-{}", Uuid::new_v4().simple()),
        exchange_order_id: Some(ack.order_id.clone()),
        client_order_id: ack.client_order_id.clone(),
        symbol: request.symbol.clone(),
        option_symbol: if ack.symbol.is_empty() {
            preview.candidate.option_symbol
        } else {
            ack.symbol.clone()
        },
        direction: request.direction,
        status,
        expiry: request.expiry,
        expiry_at: request.expiry_at,
        strike: request.strike,
        quantity: request.quantity,
        filled_quantity: ack.executed_qty,
        avg_price,
        fee: ack.fee,
        premium,
        estimated_pnl: premium - ack.fee,
        breakeven: preview.candidate.breakeven,
        exchange_status: ack.status.clone(),
        created_at: now,
        updated_at: now,
        settlement_action: match request.direction {
            StrategyDirection::SellPut => "若到期低于执行价，自动买入 BTC 补齐换币".to_string(),
            StrategyDirection::SellCall => "若到期高于执行价，自动卖出 BTC 补齐换币".to_string(),
        },
        settlement_price: Decimal::ZERO,
        settled_at: None,
        spot_order_id: None,
        spot_client_order_id: String::new(),
        spot_order_status: String::new(),
        rebalance_message: String::new(),
        spot_filled_quantity: Decimal::ZERO,
        spot_avg_price: Decimal::ZERO,
        spot_fee: Decimal::ZERO,
    }
}

/// 按精确 UTC 到期时间计算剩余天数。
///
/// 候选筛选仍按“天”展示和配置，但不能只看日期。这里向上取整：不足一天仍按 1 天
/// 处理，既保留现有候选浏览语义，也避免低估临近到期风险。
fn days_until_expiry(expiry_at: chrono::DateTime<Utc>) -> i64 {
    let seconds = (expiry_at - Utc::now()).num_seconds();
    if seconds <= 0 {
        return 0;
    }
    (seconds + 86_399) / 86_400
}

/// 估算年化收益率。
fn estimate_annualized(
    premium: Decimal,
    strike: Decimal,
    quantity: Decimal,
    expiry_days: i64,
) -> Decimal {
    let reserved = strike * quantity;
    if reserved <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    let days = Decimal::from(expiry_days);
    ((premium / reserved) * (dec!(365) / days) * dec!(100)).round_dp(2)
}

/// 估算下单所需保证金。
///
/// 卖 Put 需要覆盖名义本金；卖 Call 按 Binance 合约保证金参数估算。
pub fn required_margin(
    contract: &OptionContract,
    direction: StrategyDirection,
    quantity: Decimal,
) -> Decimal {
    let contract_units = quantity * contract.unit;
    let notional = contract.strike_price * contract_units;
    let ratio_margin = notional * contract.initial_margin;
    let min_margin = contract.min_initial_margin * contract_units;
    let maintenance_margin = max_decimal(
        notional * contract.maintenance_margin,
        contract.min_maintenance_margin * contract_units,
    );
    match direction {
        StrategyDirection::SellPut => max_decimal(
            notional,
            max_decimal(maintenance_margin, max_decimal(ratio_margin, min_margin)),
        ),
        StrategyDirection::SellCall => {
            max_decimal(maintenance_margin, max_decimal(ratio_margin, min_margin))
        }
    }
}

/// 把期权卖单限价对齐到 Binance PRICE_FILTER tick。
///
/// 期权下单始终是卖出限价 IOC。向下对齐不会把最低可接受权利金抬高到 mark price 之上，
/// 同时避免把不在 tick size 网格上的价格发给 Binance。
pub fn option_sell_limit_price(price: Decimal, contract: &OptionContract) -> Decimal {
    let tick_aligned = align_decimal_down_to_step(price, contract.price_tick_size);
    let min_aligned = if contract.min_price > Decimal::ZERO {
        max_decimal(tick_aligned, contract.min_price)
    } else {
        tick_aligned
    };
    if contract.max_price > Decimal::ZERO {
        min_decimal(min_aligned, contract.max_price)
    } else {
        min_aligned
    }
}

/// 判断数值是否落在交易所 step/tick 网格上。
pub fn decimal_matches_step(value: Decimal, step: Decimal) -> bool {
    if step <= Decimal::ZERO {
        return true;
    }
    (value / step).normalize().scale() == 0
}

/// 把 Binance 订单状态映射成本地策略状态。
///
/// IOC 订单可能返回 `CANCELED` 但已经部分成交，因此需要结合成交数量判断。
pub fn status_from_exchange(
    status: &str,
    executed_qty: Decimal,
    requested_qty: Decimal,
) -> StrategyStatus {
    match status {
        "FILLED" => StrategyStatus::Running,
        "PARTIALLY_FILLED" => StrategyStatus::PartiallyFilled,
        "CANCELED" | "CANCELLED" => {
            if executed_qty > Decimal::ZERO {
                StrategyStatus::PartiallyFilled
            } else {
                StrategyStatus::Cancelled
            }
        }
        "EXPIRED" => {
            if executed_qty > Decimal::ZERO {
                StrategyStatus::PartiallyFilled
            } else {
                StrategyStatus::Expired
            }
        }
        "REJECTED" => StrategyStatus::Rejected,
        "NEW" | "PENDING" => StrategyStatus::Pending,
        _ if executed_qty > Decimal::ZERO && executed_qty < requested_qty => {
            StrategyStatus::PartiallyFilled
        }
        _ if executed_qty > Decimal::ZERO => StrategyStatus::Running,
        _ => StrategyStatus::Pending,
    }
}

fn direction_from_option_side(side: &str) -> Option<StrategyDirection> {
    match side {
        "PUT" => Some(StrategyDirection::SellPut),
        "CALL" => Some(StrategyDirection::SellCall),
        _ => None,
    }
}

/// 校验执行价是否符合双币赢方向。
fn strike_matches_direction(
    strike: Decimal,
    direction: StrategyDirection,
    spot_price: Decimal,
) -> bool {
    match direction {
        StrategyDirection::SellPut => strike < spot_price,
        StrategyDirection::SellCall => strike > spot_price,
    }
}

/// 计算执行价相对现货价的偏离百分比。
fn calculate_strike_distance_percent(strike: Decimal, spot_price: Decimal) -> Decimal {
    if spot_price <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    (((strike - spot_price) / spot_price) * dec!(100)).round_dp(2)
}

/// 推荐等级排序权重。
fn recommendation_rank(level: RecommendationLevel) -> u8 {
    match level {
        RecommendationLevel::Reject => 0,
        RecommendationLevel::Watch => 1,
        RecommendationLevel::Recommended => 2,
    }
}

/// 候选排序：推荐优先，其次年化高优先，再按精确到期时间近优先。
fn sort_candidates(candidates: &mut [StrategyCandidate]) {
    candidates.sort_by(|left, right| {
        recommendation_rank(right.recommendation)
            .cmp(&recommendation_rank(left.recommendation))
            .then_with(|| right.annualized.cmp(&left.annualized))
            .then_with(|| left.expiry_at.cmp(&right.expiry_at))
    });
}

/// 默认风控规则。
fn default_risk_rules() -> RiskRuleSet {
    RiskRuleSet {
        max_margin_usage: dec!(50),
    }
}

/// 统计当天订单权利金。
fn today_option_premium(orders: &[StrategyOrder]) -> Decimal {
    let today = Utc::now().date_naive();
    orders
        .iter()
        .filter(|order| order.created_at.date_naive() == today)
        .map(|order| order.premium)
        .sum::<Decimal>()
}

/// 计算百分比，分母为 0 时交给调用方决定默认值。
fn percent_of(part: Decimal, total: Decimal) -> Option<Decimal> {
    if total <= Decimal::ZERO {
        None
    } else {
        Some(((part / total) * dec!(100)).round_dp(2))
    }
}

/// Decimal 绝对值，避免为了简单计算把金融字段转成浮点数。
fn abs_decimal(value: Decimal) -> Decimal {
    if value < Decimal::ZERO { -value } else { value }
}

/// Decimal 最大值。
fn max_decimal(left: Decimal, right: Decimal) -> Decimal {
    if left >= right { left } else { right }
}

fn min_decimal(left: Decimal, right: Decimal) -> Decimal {
    if left <= right { left } else { right }
}

fn align_decimal_down_to_step(value: Decimal, step: Decimal) -> Decimal {
    if step <= Decimal::ZERO {
        return value;
    }
    (value / step).round_dp_with_strategy(0, RoundingStrategy::ToNegativeInfinity) * step
}
