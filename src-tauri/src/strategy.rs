use chrono::{Duration, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

use crate::domain::{
    AppSettings, AutoInvestmentRule, BalanceRow, ChartPoint, MetricCard, PnlBreakdown, PnlReport,
    RecommendationLevel, RiskAlert, RiskRuleSet, RiskSeverity, RiskSnapshot, StrategyCandidate,
    StrategyDirection, StrategyOrder, StrategyPreview, StrategyRequest, StrategyStatus,
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

pub fn default_balances() -> Vec<BalanceRow> {
    vec![
        BalanceRow {
            asset: "BTC".to_string(),
            free: dec!(0.521479),
            locked: dec!(0.200000),
            usdt_value: dec!(36548.72),
        },
        BalanceRow {
            asset: "USDT".to_string(),
            free: dec!(8942.31),
            locked: dec!(5128.84),
            usdt_value: dec!(8942.31),
        },
    ]
}

pub fn default_candidates() -> Vec<StrategyCandidate> {
    let today = Utc::now().date_naive();
    let expiry_14 = today + Duration::days(14);
    let expiry_21 = today + Duration::days(21);

    vec![
        StrategyCandidate {
            id: "btc-put-72000-14d".to_string(),
            symbol: "BTCUSDT".to_string(),
            option_symbol: format!("BTC-{}-72000-P", expiry_14.format("%y%m%d")),
            direction: StrategyDirection::SellPut,
            expiry: expiry_14,
            strike: dec!(72000),
            quantity: dec!(0.01),
            premium: dec!(186.72),
            annualized: dec!(18.56),
            delta_abs: dec!(0.22),
            iv_rank: dec!(48),
            exercise_probability: dec!(22.31),
            breakeven: dec!(71813.28),
            recommendation: RecommendationLevel::Recommended,
            reason: "符合稳健 preset，收益/Delta/IV Rank 均在阈值内".to_string(),
        },
        StrategyCandidate {
            id: "btc-put-70000-21d".to_string(),
            symbol: "BTCUSDT".to_string(),
            option_symbol: format!("BTC-{}-70000-P", expiry_21.format("%y%m%d")),
            direction: StrategyDirection::SellPut,
            expiry: expiry_21,
            strike: dec!(70000),
            quantity: dec!(0.01),
            premium: dec!(210.45),
            annualized: dec!(16.21),
            delta_abs: dec!(0.18),
            iv_rank: dec!(52),
            exercise_probability: dec!(18.74),
            breakeven: dec!(69789.55),
            recommendation: RecommendationLevel::Recommended,
            reason: "更远到期，资金占用略高但被行权概率较低".to_string(),
        },
        StrategyCandidate {
            id: "btc-call-76000-14d".to_string(),
            symbol: "BTCUSDT".to_string(),
            option_symbol: format!("BTC-{}-76000-C", expiry_14.format("%y%m%d")),
            direction: StrategyDirection::SellCall,
            expiry: expiry_14,
            strike: dec!(76000),
            quantity: dec!(0.01),
            premium: dec!(158.36),
            annualized: dec!(15.02),
            delta_abs: dec!(0.26),
            iv_rank: dec!(55),
            exercise_probability: dec!(25.61),
            breakeven: dec!(76158.36),
            recommendation: RecommendationLevel::Watch,
            reason: "适合已有 BTC 仓位止盈，接近最低年化阈值".to_string(),
        },
        StrategyCandidate {
            id: "btc-put-74000-21d".to_string(),
            symbol: "BTCUSDT".to_string(),
            option_symbol: format!("BTC-{}-74000-P", expiry_21.format("%y%m%d")),
            direction: StrategyDirection::SellPut,
            expiry: expiry_21,
            strike: dec!(74000),
            quantity: dec!(0.01),
            premium: dec!(172.18),
            annualized: dec!(9.21),
            delta_abs: dec!(0.35),
            iv_rank: dec!(78),
            exercise_probability: dec!(37.88),
            breakeven: dec!(73827.82),
            recommendation: RecommendationLevel::Reject,
            reason: "年化低于阈值且 Delta/IV Rank 偏高".to_string(),
        },
    ]
}

pub fn seed_orders() -> Vec<StrategyOrder> {
    let now = Utc::now();
    let expiry = now.date_naive() + Duration::days(14);
    vec![
        StrategyOrder {
            id: "MO-SIM-0001".to_string(),
            exchange_order_id: None,
            symbol: "BTCUSDT".to_string(),
            option_symbol: format!("BTC-{}-72000-P", expiry.format("%y%m%d")),
            direction: StrategyDirection::SellPut,
            status: StrategyStatus::Running,
            expiry,
            strike: dec!(72000),
            quantity: dec!(0.01),
            premium: dec!(186.72),
            estimated_pnl: dec!(186.72),
            breakeven: dec!(71813.28),
            created_at: now - Duration::hours(2),
            updated_at: now - Duration::hours(2),
            settlement_action: "若到期低于执行价，自动买入 0.01 BTC".to_string(),
        },
        StrategyOrder {
            id: "MO-SIM-0002".to_string(),
            exchange_order_id: None,
            symbol: "BTCUSDT".to_string(),
            option_symbol: format!("BTC-{}-76000-C", expiry.format("%y%m%d")),
            direction: StrategyDirection::SellCall,
            status: StrategyStatus::Pending,
            expiry,
            strike: dec!(76000),
            quantity: dec!(0.01),
            premium: dec!(158.36),
            estimated_pnl: dec!(158.36),
            breakeven: dec!(76158.36),
            created_at: now - Duration::minutes(35),
            updated_at: now - Duration::minutes(35),
            settlement_action: "若到期高于执行价，自动卖出 0.01 BTC".to_string(),
        },
    ]
}

pub fn build_metrics(settings: &AppSettings) -> Vec<MetricCard> {
    vec![
        MetricCard {
            id: "portfolio".to_string(),
            title: "总资产估值".to_string(),
            value: "¥ 48,862.37".to_string(),
            sub_value: "≈ 0.697891 BTC".to_string(),
            change: "+1.28%".to_string(),
            icon: "wallet".to_string(),
            tone: "gold".to_string(),
        },
        MetricCard {
            id: "margin".to_string(),
            title: "可用保证金".to_string(),
            value: "¥ 12,356.18".to_string(),
            sub_value: "≈ 0.176412 BTC".to_string(),
            change: "+2.15%".to_string(),
            icon: "shield".to_string(),
            tone: "blue".to_string(),
        },
        MetricCard {
            id: "premium".to_string(),
            title: "今日权利金".to_string(),
            value: "¥ 186.72".to_string(),
            sub_value: "≈ 0.002664 BTC".to_string(),
            change: "+8.74%".to_string(),
            icon: "coins".to_string(),
            tone: "gold".to_string(),
        },
        MetricCard {
            id: "status".to_string(),
            title: "运行状态".to_string(),
            value: if settings.auto_enabled {
                "运行中"
            } else {
                "已暂停"
            }
            .to_string(),
            sub_value: "策略正常执行中".to_string(),
            change: "后台常驻".to_string(),
            icon: "activity".to_string(),
            tone: if settings.auto_enabled {
                "green"
            } else {
                "gray"
            }
            .to_string(),
        },
    ]
}

pub fn build_pnl_report() -> PnlReport {
    let points = [
        ("05-17", dec!(-1800)),
        ("05-19", dec!(1200)),
        ("05-21", dec!(2300)),
        ("05-23", dec!(6200)),
        ("05-25", dec!(8900)),
        ("05-27", dec!(12100)),
        ("05-29", dec!(14800)),
        ("05-31", dec!(16450)),
        ("06-02", dec!(18100)),
        ("06-03", dec!(18862.37)),
    ]
    .into_iter()
    .map(|(label, value)| ChartPoint {
        label: label.to_string(),
        value,
    })
    .collect();

    PnlReport {
        cumulative: dec!(48862.37),
        today: dec!(1862.45),
        option_premium: dec!(12356.18),
        realized_spot: dec!(21482.37),
        floating_spot: dec!(10693.52),
        fees: dec!(-1669.70),
        points,
        breakdown: vec![
            PnlBreakdown {
                label: "权利金收益".to_string(),
                value: dec!(12356.18),
                color: "#f5b625".to_string(),
            },
            PnlBreakdown {
                label: "已实现盈亏".to_string(),
                value: dec!(21482.37),
                color: "#52c66f".to_string(),
            },
            PnlBreakdown {
                label: "浮动盈亏".to_string(),
                value: dec!(10693.52),
                color: "#5f8ff5".to_string(),
            },
            PnlBreakdown {
                label: "手续费影响".to_string(),
                value: dec!(-1669.70),
                color: "#ef5b4c".to_string(),
            },
        ],
    }
}

pub fn build_risk_snapshot() -> RiskSnapshot {
    RiskSnapshot {
        score: 45,
        level: "中等风险".to_string(),
        margin_usage: dec!(42.31),
        exercise_risk: dec!(18.37),
        daily_drawdown: dec!(2.48),
        rules: RiskRuleSet {
            max_single_order_usdt: dec!(500),
            max_daily_orders: dec!(20),
            max_margin_usage: dec!(50),
            max_drawdown: dec!(5),
            stop_after_consecutive_losses: 3,
        },
        alerts: vec![
            RiskAlert {
                id: "risk-btc-itm".to_string(),
                severity: RiskSeverity::Medium,
                title: "BTC 接近上方行权价".to_string(),
                message: "当前 BTC 价格距离上方行权价约 2.15%，影响卖 Call 仓位 1 单".to_string(),
                created_at: Utc::now() - Duration::minutes(11),
            },
            RiskAlert {
                id: "risk-margin".to_string(),
                severity: RiskSeverity::Medium,
                title: "保证金占用率上升".to_string(),
                message: "当前保证金占用率达到 42.31%，仍低于 50% 限制".to_string(),
                created_at: Utc::now() - Duration::minutes(23),
            },
            RiskAlert {
                id: "risk-drawdown".to_string(),
                severity: RiskSeverity::Low,
                title: "单日回撤接近阈值".to_string(),
                message: "单日最大回撤 2.48%，低于 5% 暂停阈值".to_string(),
                created_at: Utc::now() - Duration::minutes(38),
            },
        ],
    }
}

pub fn preview_strategy(request: &StrategyRequest) -> StrategyPreview {
    let premium = estimate_premium(request.direction, request.strike, request.quantity);
    let breakeven = match request.direction {
        StrategyDirection::SellPut => request.strike - premium / request.quantity,
        StrategyDirection::SellCall => request.strike + premium / request.quantity,
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
        premium,
        annualized,
        delta_abs: dec!(0.24),
        iv_rank: dec!(51),
        exercise_probability: dec!(22.50),
        breakeven,
        recommendation: RecommendationLevel::Watch,
        reason: "手动策略预览，真实下单前会再次检查价格保护与资金占用".to_string(),
    };

    let reserved_asset = match request.direction {
        StrategyDirection::SellPut => request.strike * request.quantity,
        StrategyDirection::SellCall => request.quantity,
    };
    let protected_limit_price = premium * dec!(0.985);
    let action = match request.direction {
        StrategyDirection::SellPut => "到期 ITM 后自动买入 BTC，模拟双币赢买低换币",
        StrategyDirection::SellCall => "到期 ITM 后自动卖出 BTC，模拟双币赢止盈换币",
    };

    StrategyPreview {
        candidate,
        max_profit: premium,
        reserved_asset,
        protected_limit_price,
        summary: vec![
            format!("快速成交：以受保护限价尝试即时成交，不满足价格保护则放弃"),
            format!("{action}"),
            format!("预计年化 {}%，数量 {} 张", annualized, request.quantity),
        ],
    }
}

pub fn order_from_request(
    request: &StrategyRequest,
    exchange_order_id: Option<String>,
) -> StrategyOrder {
    let preview = preview_strategy(request);
    let now = Utc::now();
    StrategyOrder {
        id: format!("MO-{}", Uuid::new_v4().simple()),
        exchange_order_id,
        symbol: request.symbol.clone(),
        option_symbol: preview.candidate.option_symbol,
        direction: request.direction,
        status: StrategyStatus::Running,
        expiry: request.expiry,
        strike: request.strike,
        quantity: request.quantity,
        premium: preview.max_profit,
        estimated_pnl: preview.max_profit,
        breakeven: preview.candidate.breakeven,
        created_at: now,
        updated_at: now,
        settlement_action: match request.direction {
            StrategyDirection::SellPut => "若到期低于执行价，自动买入 BTC 补齐换币".to_string(),
            StrategyDirection::SellCall => "若到期高于执行价，自动卖出 BTC 补齐换币".to_string(),
        },
    }
}

fn estimate_premium(direction: StrategyDirection, strike: Decimal, quantity: Decimal) -> Decimal {
    let base_rate = match direction {
        StrategyDirection::SellPut => dec!(0.0026),
        StrategyDirection::SellCall => dec!(0.0022),
    };
    (strike * quantity * base_rate).round_dp(2)
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

#[cfg(test)]
mod tests {
    use super::*;

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
            live: false,
            confirm_live: false,
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
            live: false,
            confirm_live: false,
        };

        let preview = preview_strategy(&request);
        assert!(preview.candidate.breakeven > request.strike);
        assert!(preview.protected_limit_price < preview.max_profit);
    }
}
