use std::collections::HashMap;

use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

use crate::domain::{
    AppSettings, AutoInvestmentRule, BalanceRow, ChartPoint, ExchangeOrderAck, MetricCard,
    OptionAccountSnapshot, OptionContract, OptionMarkPrice, PnlBreakdown, PnlReport,
    RecommendationLevel, RiskAlert, RiskRuleSet, RiskSeverity, RiskSnapshot, SpotAccountSnapshot,
    StrategyCandidate, StrategyDirection, StrategyOrder, StrategyPreview, StrategyRequest,
    StrategyStatus,
};

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

pub fn build_metrics(
    settings: &AppSettings,
    account: Option<&OptionAccountSnapshot>,
    orders: &[StrategyOrder],
) -> Vec<MetricCard> {
    let sync_state = if settings.connected {
        if let Some(time) = account.and_then(|snapshot| snapshot.time) {
            format!("Binance {}", time.format("%H:%M:%S UTC"))
        } else if account.is_some() {
            "Binance 期权账户".to_string()
        } else {
            "等待数据同步".to_string()
        }
    } else {
        "等待 API 连接".to_string()
    };
    let usdt = account.and_then(|snapshot| snapshot.asset("USDT"));
    let total_equity = usdt.map_or(Decimal::ZERO, |asset| {
        if asset.adjusted_equity > Decimal::ZERO {
            asset.adjusted_equity
        } else {
            asset.equity
        }
    });
    let available_margin = usdt.map_or(Decimal::ZERO, |asset| asset.available);
    let today_premium = today_option_premium(orders);
    vec![
        MetricCard {
            id: "portfolio".to_string(),
            title: "总资产估值".to_string(),
            value: format_money(total_equity),
            sub_value: sync_state.clone(),
            change: "Binance".to_string(),
            icon: "wallet".to_string(),
            tone: "gold".to_string(),
        },
        MetricCard {
            id: "margin".to_string(),
            title: "可用保证金".to_string(),
            value: format_money(available_margin),
            sub_value: sync_state.clone(),
            change: "Binance".to_string(),
            icon: "shield".to_string(),
            tone: "blue".to_string(),
        },
        MetricCard {
            id: "premium".to_string(),
            title: "今日权利金".to_string(),
            value: format_money(today_premium),
            sub_value: "本地订单统计".to_string(),
            change: "本地".to_string(),
            icon: "coins".to_string(),
            tone: "gold".to_string(),
        },
        MetricCard {
            id: "status".to_string(),
            title: "运行状态".to_string(),
            value: if settings.auto_enabled {
                "运行中"
            } else if settings.connected {
                "已连接"
            } else {
                "未连接"
            }
            .to_string(),
            sub_value: "Tauri 客户端".to_string(),
            change: sync_state.clone(),
            icon: "activity".to_string(),
            tone: if settings.auto_enabled {
                "green"
            } else if settings.connected {
                "gold"
            } else {
                "gray"
            }
            .to_string(),
        },
    ]
}

pub fn empty_pnl_report() -> PnlReport {
    PnlReport {
        cumulative: Decimal::ZERO,
        today: Decimal::ZERO,
        option_premium: Decimal::ZERO,
        realized_spot: Decimal::ZERO,
        floating_spot: Decimal::ZERO,
        fees: Decimal::ZERO,
        points: Vec::new(),
        breakdown: Vec::new(),
    }
}

pub fn build_balances(account: &OptionAccountSnapshot) -> Vec<BalanceRow> {
    account
        .assets
        .iter()
        .map(|asset| BalanceRow {
            asset: asset.asset.clone(),
            source: "期权保证金".to_string(),
            free: asset.available,
            locked: max_decimal(asset.margin_balance - asset.available, Decimal::ZERO),
            usdt_value: if asset.asset == "USDT" {
                asset.adjusted_equity
            } else {
                Decimal::ZERO
            },
        })
        .collect()
}

pub fn build_spot_balances(
    account: &SpotAccountSnapshot,
    btc_usdt_price: Decimal,
) -> Vec<BalanceRow> {
    account
        .balances
        .iter()
        .filter(|asset| asset.free > Decimal::ZERO || asset.locked > Decimal::ZERO)
        .map(|asset| BalanceRow {
            asset: asset.asset.clone(),
            source: "现货".to_string(),
            free: asset.free,
            locked: asset.locked,
            usdt_value: if asset.asset == "USDT" {
                asset.free + asset.locked
            } else if asset.asset == "BTC" {
                (asset.free + asset.locked) * btc_usdt_price
            } else {
                Decimal::ZERO
            },
        })
        .collect()
}

pub fn pnl_report_from_orders(orders: &[StrategyOrder]) -> PnlReport {
    pnl_report_from_orders_with_spot(orders, None)
}

pub fn pnl_report_from_orders_with_spot(
    orders: &[StrategyOrder],
    spot_price: Option<Decimal>,
) -> PnlReport {
    if orders.is_empty() {
        return empty_pnl_report();
    }

    let option_premium: Decimal = orders.iter().map(|order| order.premium).sum();
    let fees: Decimal = orders.iter().map(|order| order.fee).sum();
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
    let cumulative = option_premium + floating_spot - fees;
    PnlReport {
        cumulative,
        today,
        option_premium,
        realized_spot: Decimal::ZERO,
        floating_spot,
        fees,
        points: Vec::<ChartPoint>::new(),
        breakdown: if cumulative == Decimal::ZERO && option_premium == Decimal::ZERO {
            Vec::new()
        } else {
            vec![
                PnlBreakdown {
                    label: "期权权利金".to_string(),
                    value: option_premium,
                    color: "#d89b2b".to_string(),
                },
                PnlBreakdown {
                    label: "手续费".to_string(),
                    value: -fees,
                    color: "#cc4b37".to_string(),
                },
                PnlBreakdown {
                    label: "现货补齐浮动盈亏".to_string(),
                    value: floating_spot,
                    color: if floating_spot >= Decimal::ZERO {
                        "#2fa66a".to_string()
                    } else {
                        "#cc4b37".to_string()
                    },
                },
            ]
        },
    }
}

pub fn empty_risk_snapshot() -> RiskSnapshot {
    RiskSnapshot {
        score: 0,
        level: "未同步".to_string(),
        margin_usage: Decimal::ZERO,
        exercise_risk: Decimal::ZERO,
        daily_drawdown: Decimal::ZERO,
        rules: default_risk_rules(),
        alerts: Vec::new(),
    }
}

pub fn risk_snapshot_from_account(account: &OptionAccountSnapshot) -> RiskSnapshot {
    let rules = default_risk_rules();
    let usdt = account.asset("USDT");
    let margin_usage = usdt
        .and_then(|asset| percent_of(asset.initial_margin, asset.adjusted_equity))
        .unwrap_or(Decimal::ZERO);
    let maintenance_usage = usdt
        .and_then(|asset| percent_of(asset.maint_margin, asset.adjusted_equity))
        .unwrap_or(Decimal::ZERO);
    let unrealized_pnl = usdt.map_or(Decimal::ZERO, |asset| asset.unrealized_pnl);
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
        daily_drawdown: if unrealized_pnl < Decimal::ZERO {
            -unrealized_pnl
        } else {
            Decimal::ZERO
        },
        rules,
        alerts,
    }
}

#[allow(clippy::too_many_lines)]
pub fn build_candidates(
    contracts: &[OptionContract],
    marks: &[OptionMarkPrice],
    rule: &AutoInvestmentRule,
    account: &OptionAccountSnapshot,
    orders: &[StrategyOrder],
) -> Vec<StrategyCandidate> {
    if !rule.include_btc {
        return Vec::new();
    }

    let mark_by_symbol = marks
        .iter()
        .map(|mark| (mark.symbol.as_str(), mark))
        .collect::<HashMap<_, _>>();
    let today = Utc::now().date_naive();
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

    let mut candidates = contracts
        .iter()
        .filter_map(|contract| {
            let direction = direction_from_option_side(&contract.side)?;
            if rule.exclude_same_direction && has_open_direction(direction) {
                return None;
            }
            if contract.underlying != "BTCUSDT"
                || contract.quote_asset != "USDT"
                || contract.status != "TRADING"
                || rule.quantity < contract.min_qty
                || rule.quantity > contract.max_qty
            {
                return None;
            }

            let days_to_expiry = (contract.expiry - today).num_days();
            if days_to_expiry < rule.min_days_to_expiry || days_to_expiry > rule.max_days_to_expiry
            {
                return None;
            }

            let mark = mark_by_symbol.get(contract.symbol.as_str()).copied()?;
            if mark.mark_price <= Decimal::ZERO {
                return None;
            }

            let delta_abs = abs_decimal(mark.delta);
            let iv_rank = (mark.mark_iv * dec!(100)).round_dp(2);
            let quantity = rule.quantity;
            let limit_price = mark.mark_price.round_dp(contract.price_scale);
            let premium = (limit_price * quantity).round_dp(contract.price_scale);
            let annualized = estimate_annualized(
                premium,
                contract.strike_price,
                quantity * contract.unit,
                days_to_expiry.max(1),
            );
            let exercise_probability = (delta_abs * dec!(100)).round_dp(2);
            let breakeven = match direction {
                StrategyDirection::SellPut => contract.strike_price - mark.mark_price,
                StrategyDirection::SellCall => contract.strike_price + mark.mark_price,
            };
            let required_margin = required_margin(contract, direction, quantity);
            let available_margin = account
                .asset("USDT")
                .map_or(Decimal::ZERO, |asset| asset.available);
            let passes_rule = annualized >= rule.min_annualized
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
                format!("来自 Binance 合约和 mark price，满足当前自动规则，mark IV 代理值 {iv_rank}%")
            } else if available_margin < required_margin {
                "可用保证金不足，暂不建议下单".to_string()
            } else {
                format!("来自 Binance 合约和 mark price，未完全满足当前自动规则，mark IV 代理值 {iv_rank}%")
            };

            Some(StrategyCandidate {
                id: format!("{}-{}", contract.symbol, Utc::now().timestamp()),
                symbol: contract.underlying.clone(),
                option_symbol: contract.symbol.clone(),
                direction,
                expiry: contract.expiry,
                strike: contract.strike_price,
                quantity,
                limit_price,
                premium,
                annualized,
                delta_abs,
                iv_rank,
                exercise_probability,
                breakeven,
                recommendation,
                reason,
            })
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        recommendation_rank(right.recommendation)
            .cmp(&recommendation_rank(left.recommendation))
            .then_with(|| right.annualized.cmp(&left.annualized))
            .then_with(|| left.expiry.cmp(&right.expiry))
    });
    candidates.truncate(20);
    candidates
}

pub fn preview_strategy(request: &StrategyRequest) -> StrategyPreview {
    let premium = (request.limit_price * request.quantity).round_dp(2);
    let breakeven = match request.direction {
        StrategyDirection::SellPut => request.strike - request.limit_price,
        StrategyDirection::SellCall => request.strike + request.limit_price,
    };
    let expiry_days = (request.expiry - Utc::now().date_naive()).num_days().max(1);
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
        strike: request.strike,
        quantity: request.quantity,
        limit_price: request.limit_price,
        premium,
        annualized,
        delta_abs: Decimal::ZERO,
        iv_rank: Decimal::ZERO,
        exercise_probability: Decimal::ZERO,
        breakeven,
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

fn recommendation_rank(level: RecommendationLevel) -> u8 {
    match level {
        RecommendationLevel::Reject => 0,
        RecommendationLevel::Watch => 1,
        RecommendationLevel::Recommended => 2,
    }
}

fn default_risk_rules() -> RiskRuleSet {
    RiskRuleSet {
        max_single_order_usdt: dec!(500),
        max_daily_orders: dec!(20),
        max_margin_usage: dec!(50),
        max_drawdown: dec!(5),
        stop_after_consecutive_losses: 3,
    }
}

fn today_option_premium(orders: &[StrategyOrder]) -> Decimal {
    let today = Utc::now().date_naive();
    orders
        .iter()
        .filter(|order| order.created_at.date_naive() == today)
        .map(|order| order.premium)
        .sum::<Decimal>()
}

fn percent_of(part: Decimal, total: Decimal) -> Option<Decimal> {
    if total <= Decimal::ZERO {
        None
    } else {
        Some(((part / total) * dec!(100)).round_dp(2))
    }
}

fn format_money(value: Decimal) -> String {
    format!("{} USDT", value.round_dp(2))
}

fn abs_decimal(value: Decimal) -> Decimal {
    if value < Decimal::ZERO { -value } else { value }
}

fn max_decimal(left: Decimal, right: Decimal) -> Decimal {
    if left >= right { left } else { right }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Duration};

    #[test]
    fn sell_put_preview_computes_breakeven_below_strike() {
        let expiry = Utc::now().date_naive() + Duration::days(14);
        let request = StrategyRequest {
            candidate_id: None,
            symbol: "BTCUSDT".to_string(),
            option_symbol: None,
            direction: StrategyDirection::SellPut,
            expiry,
            strike: dec!(72000),
            quantity: dec!(0.01),
            limit_price: dec!(180),
            confirm_live: true,
        };

        let preview = preview_strategy(&request);
        assert!(preview.candidate.breakeven < request.strike);
        assert!(preview.max_profit > Decimal::ZERO);
    }

    #[test]
    fn sell_call_preview_computes_breakeven_above_strike() {
        let expiry = Utc::now().date_naive() + Duration::days(14);
        let request = StrategyRequest {
            candidate_id: None,
            symbol: "BTCUSDT".to_string(),
            option_symbol: None,
            direction: StrategyDirection::SellCall,
            expiry,
            strike: dec!(76000),
            quantity: dec!(0.01),
            limit_price: dec!(160),
            confirm_live: true,
        };

        let preview = preview_strategy(&request);
        assert!(preview.candidate.breakeven > request.strike);
        assert_eq!(preview.protected_limit_price, request.limit_price);
    }

    #[test]
    fn build_candidates_uses_live_contract_and_mark_inputs() {
        let expiry = Utc::now().date_naive() + Duration::days(14);
        let contract = option_contract("BTC-260626-72000-P", "PUT", expiry);
        let mark = OptionMarkPrice {
            symbol: contract.symbol.clone(),
            mark_price: dec!(600),
            mark_iv: dec!(0.52),
            delta: dec!(-0.20),
            high_price_limit: dec!(1000),
            low_price_limit: dec!(1),
        };
        let account = account_snapshot(dec!(1000));

        let candidates = build_candidates(
            &[contract],
            &[mark],
            &default_auto_rule(false),
            &account,
            &[],
        );

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].option_symbol, "BTC-260626-72000-P");
        assert_eq!(
            candidates[0].recommendation,
            RecommendationLevel::Recommended
        );
        assert_eq!(candidates[0].limit_price, dec!(600));
        assert_eq!(candidates[0].premium, dec!(6.00));
        assert_eq!(candidates[0].delta_abs, dec!(0.20));
        assert!(candidates[0].reason.contains("mark IV"));
    }

    #[test]
    fn ioc_cancel_without_fill_is_not_running_order() {
        let expiry = Utc::now().date_naive() + Duration::days(14);
        let request = strategy_request(expiry);
        let ack = ExchangeOrderAck {
            order_id: "4611875134427365377".to_string(),
            client_order_id: "di-put-1".to_string(),
            symbol: "BTC-260626-72000-P".to_string(),
            status: "CANCELED".to_string(),
            executed_qty: Decimal::ZERO,
            avg_price: Decimal::ZERO,
            price: dec!(200),
            quantity: dec!(0.01),
            fee: Decimal::ZERO,
            create_time: None,
            update_time: Some(DateTime::<Utc>::from_timestamp(1_772_000_000, 0).unwrap()),
        };

        let order = order_from_request(&request, &ack);

        assert_eq!(order.status, StrategyStatus::Cancelled);
        assert_eq!(order.premium, Decimal::ZERO);
        assert_eq!(order.filled_quantity, Decimal::ZERO);
        assert_eq!(
            order.exchange_order_id.as_deref(),
            Some("4611875134427365377")
        );
    }

    #[test]
    fn ioc_partial_fill_records_real_fill_and_status() {
        let expiry = Utc::now().date_naive() + Duration::days(14);
        let request = strategy_request(expiry);
        let ack = ExchangeOrderAck {
            order_id: "4611875134427365378".to_string(),
            client_order_id: "di-put-2".to_string(),
            symbol: "BTC-260626-72000-P".to_string(),
            status: "CANCELED".to_string(),
            executed_qty: dec!(0.005),
            avg_price: dec!(200),
            price: dec!(200),
            quantity: dec!(0.01),
            fee: dec!(0.01),
            create_time: None,
            update_time: Some(DateTime::<Utc>::from_timestamp(1_772_000_000, 0).unwrap()),
        };

        let order = order_from_request(&request, &ack);

        assert_eq!(order.status, StrategyStatus::PartiallyFilled);
        assert_eq!(order.filled_quantity, dec!(0.005));
        assert_eq!(order.premium, dec!(1.00));
        assert_eq!(order.estimated_pnl, dec!(0.99));
    }

    fn strategy_request(expiry: chrono::NaiveDate) -> StrategyRequest {
        StrategyRequest {
            candidate_id: None,
            symbol: "BTCUSDT".to_string(),
            option_symbol: Some("BTC-260626-72000-P".to_string()),
            direction: StrategyDirection::SellPut,
            expiry,
            strike: dec!(72000),
            quantity: dec!(0.01),
            limit_price: dec!(200),
            confirm_live: true,
        }
    }

    fn account_snapshot(available: Decimal) -> OptionAccountSnapshot {
        OptionAccountSnapshot {
            assets: vec![crate::domain::OptionAssetBalance {
                asset: "USDT".to_string(),
                margin_balance: available,
                equity: available,
                available,
                initial_margin: Decimal::ZERO,
                maint_margin: Decimal::ZERO,
                unrealized_pnl: Decimal::ZERO,
                adjusted_equity: available,
            }],
            can_trade: true,
            time: None,
        }
    }

    fn option_contract(symbol: &str, side: &str, expiry: chrono::NaiveDate) -> OptionContract {
        OptionContract {
            symbol: symbol.to_string(),
            side: side.to_string(),
            strike_price: dec!(72000),
            underlying: "BTCUSDT".to_string(),
            unit: Decimal::ONE,
            min_qty: dec!(0.01),
            max_qty: dec!(10),
            initial_margin: dec!(0.10),
            maintenance_margin: dec!(0.05),
            min_initial_margin: dec!(10),
            min_maintenance_margin: dec!(5),
            price_scale: 2,
            quantity_scale: 2,
            quote_asset: "USDT".to_string(),
            status: "TRADING".to_string(),
            expiry,
        }
    }
}
