use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

use crate::domain::{
    AppSettings, AutoInvestmentRule, MetricCard, PnlReport, RecommendationLevel, RiskRuleSet,
    RiskSnapshot, StrategyCandidate, StrategyDirection, StrategyOrder, StrategyPreview,
    StrategyRequest, StrategyStatus,
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

pub fn build_metrics(settings: &AppSettings) -> Vec<MetricCard> {
    let sync_state = if settings.connected {
        "等待实盘数据同步"
    } else {
        "等待 API 连接"
    };
    vec![
        MetricCard {
            id: "portfolio".to_string(),
            title: "总资产估值".to_string(),
            value: "0.00 USDT".to_string(),
            sub_value: sync_state.to_string(),
            change: "实盘".to_string(),
            icon: "wallet".to_string(),
            tone: "gold".to_string(),
        },
        MetricCard {
            id: "margin".to_string(),
            title: "可用保证金".to_string(),
            value: "0.00 USDT".to_string(),
            sub_value: sync_state.to_string(),
            change: "实盘".to_string(),
            icon: "shield".to_string(),
            tone: "blue".to_string(),
        },
        MetricCard {
            id: "premium".to_string(),
            title: "今日权利金".to_string(),
            value: "0.00 USDT".to_string(),
            sub_value: "本地订单统计".to_string(),
            change: "实盘".to_string(),
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
            sub_value: "实盘模式".to_string(),
            change: sync_state.to_string(),
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

pub fn empty_risk_snapshot() -> RiskSnapshot {
    RiskSnapshot {
        score: 0,
        level: "未同步".to_string(),
        margin_usage: Decimal::ZERO,
        exercise_risk: Decimal::ZERO,
        daily_drawdown: Decimal::ZERO,
        rules: RiskRuleSet {
            max_single_order_usdt: dec!(500),
            max_daily_orders: dec!(20),
            max_margin_usage: dec!(50),
            max_drawdown: dec!(5),
            stop_after_consecutive_losses: 3,
        },
        alerts: Vec::new(),
    }
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
        premium,
        annualized,
        delta_abs: Decimal::ZERO,
        iv_rank: Decimal::ZERO,
        exercise_probability: Decimal::ZERO,
        breakeven,
        recommendation: RecommendationLevel::Watch,
        reason: "基于用户输入限价的实盘订单预览".to_string(),
    };

    let reserved_asset = match request.direction {
        StrategyDirection::SellPut => request.strike * request.quantity,
        StrategyDirection::SellCall => request.quantity,
    };
    let protected_limit_price = request.limit_price;
    let action = match request.direction {
        StrategyDirection::SellPut => "到期 ITM 后自动买入 BTC，完成实盘补齐",
        StrategyDirection::SellCall => "到期 ITM 后自动卖出 BTC，完成实盘补齐",
    };

    StrategyPreview {
        candidate,
        max_profit: premium,
        reserved_asset,
        protected_limit_price,
        summary: vec![
            format!("快速成交：以用户输入限价尝试 IOC 实盘成交"),
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
        id: format!("LIVE-{}", Uuid::new_v4().simple()),
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
    use chrono::Duration;

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
}
