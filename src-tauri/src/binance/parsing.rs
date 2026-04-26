use std::{collections::HashMap, str::FromStr};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;

use crate::domain::{
    AppError, AppResult, ExchangeOrderAck, ExerciseRecord, OptionAccountSnapshot,
    OptionAssetBalance, OptionContract, OptionMarkPrice, OptionPosition, SimpleEarnAssetBalance,
    SimpleEarnFlexibleProduct, SpotAccountSnapshot, SpotAssetBalance,
};

/// 解析期权账户快照，字段名保持贴近 Binance EAPI 响应。
pub(super) fn parse_account_snapshot(value: &Value) -> AppResult<OptionAccountSnapshot> {
    let assets = value
        .get("asset")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Binance("Binance 账户响应缺少 asset 字段".to_string()))?
        .iter()
        .map(parse_asset_balance)
        .collect::<AppResult<Vec<_>>>()?;

    Ok(OptionAccountSnapshot {
        assets,
        can_trade: value
            .get("canTrade")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// 解析当前期权持仓列表。
pub(super) fn parse_option_positions(value: &Value) -> AppResult<Vec<OptionPosition>> {
    let rows = value
        .as_array()
        .or_else(|| value.get("data").and_then(Value::as_array))
        .or_else(|| value.get("rows").and_then(Value::as_array))
        .ok_or_else(|| AppError::Binance("Binance 期权持仓响应不是数组".to_string()))?;

    rows.iter()
        .map(parse_option_position)
        .filter(|result| {
            result
                .as_ref()
                .map_or(true, |position| position.quantity > Decimal::ZERO)
        })
        .collect()
}

/// 解析单条期权持仓。Binance 字段在不同响应版本中可能略有差异，这里只抽取接管所需事实。
fn parse_option_position(value: &Value) -> AppResult<OptionPosition> {
    let raw_quantity =
        decimal_from_any_field(value, &["quantity", "positionAmt", "positionAmount"])?
            .unwrap_or(Decimal::ZERO);
    let side = optional_string_field(value, "side").unwrap_or_else(|| {
        if raw_quantity < Decimal::ZERO {
            "SHORT"
        } else {
            "LONG"
        }
        .to_string()
    });
    let quantity = if raw_quantity < Decimal::ZERO {
        -raw_quantity
    } else {
        raw_quantity
    };
    Ok(OptionPosition {
        symbol: string_field(value, "symbol")?,
        side,
        quantity,
        entry_price: decimal_from_any_field(
            value,
            &["entryPrice", "avgPrice", "averagePrice", "costPrice"],
        )?
        .unwrap_or(Decimal::ZERO),
        mark_price: optional_decimal_field(value, "markPrice")?.unwrap_or(Decimal::ZERO),
        update_time: date_time_from_any_millis_field(value, &["updateTime", "time"]),
    })
}

/// 解析单个期权保证金资产。
fn parse_asset_balance(value: &Value) -> AppResult<OptionAssetBalance> {
    Ok(OptionAssetBalance {
        asset: string_field(value, "asset")?,
        margin_balance: decimal_field(value, "marginBalance")?,
        equity: decimal_field(value, "equity")?,
        available: decimal_field(value, "available")?,
        initial_margin: decimal_field(value, "initialMargin")?,
        maint_margin: decimal_field(value, "maintMargin")?,
        adjusted_equity: decimal_field(value, "adjustedEquity")?,
    })
}

/// 解析现货账户快照。
pub(super) fn parse_spot_account(value: &Value) -> AppResult<SpotAccountSnapshot> {
    let balances = value
        .get("balances")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Binance("Binance 现货账户响应缺少 balances 字段".to_string()))?
        .iter()
        .map(parse_spot_asset_balance)
        .collect::<AppResult<Vec<_>>>()?;

    Ok(SpotAccountSnapshot {
        balances,
        can_trade: value
            .get("canTrade")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// 解析单个现货资产余额。
fn parse_spot_asset_balance(value: &Value) -> AppResult<SpotAssetBalance> {
    Ok(SpotAssetBalance {
        asset: string_field(value, "asset")?,
        free: decimal_field(value, "free")?,
        locked: decimal_field(value, "locked")?,
    })
}

pub(super) struct SimpleEarnPositionPage {
    pub(super) rows: Vec<SimpleEarnAssetBalance>,
    pub(super) total: Option<usize>,
}

/// 解析 Simple Earn 活期/定期持仓分页。
pub(super) fn parse_simple_earn_position_page(
    value: &Value,
    source: &str,
    amount_keys: &[&str],
    flexible: bool,
) -> AppResult<SimpleEarnPositionPage> {
    let rows = value
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| AppError::Binance("Binance Simple Earn 响应缺少 rows 字段".to_string()))?
        .iter()
        .map(|row| parse_simple_earn_position(row, source, amount_keys, flexible))
        .collect::<AppResult<Vec<_>>>()?;
    let total = value
        .get("total")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok());
    Ok(SimpleEarnPositionPage { rows, total })
}

/// 解析单条 Simple Earn 持仓。
fn parse_simple_earn_position(
    value: &Value,
    source: &str,
    amount_keys: &[&str],
    flexible: bool,
) -> AppResult<SimpleEarnAssetBalance> {
    let amount = decimal_from_any_field(value, amount_keys)?
        .ok_or_else(|| AppError::Binance("Binance Simple Earn 持仓响应缺少数量字段".to_string()))?;
    Ok(SimpleEarnAssetBalance {
        asset: string_field(value, "asset")?,
        source: source.to_string(),
        product_id: optional_string_field(value, "productId").unwrap_or_default(),
        amount,
        flexible,
        can_redeem: value
            .get("canRedeem")
            .and_then(Value::as_bool)
            .unwrap_or(flexible),
    })
}

/// 解析 Simple Earn 活期产品列表。
pub(super) fn parse_simple_earn_flexible_product_page(
    value: &Value,
) -> AppResult<Vec<SimpleEarnFlexibleProduct>> {
    value
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AppError::Binance("Binance Simple Earn 产品列表响应缺少 rows 字段".to_string())
        })?
        .iter()
        .map(parse_simple_earn_flexible_product)
        .collect()
}

fn parse_simple_earn_flexible_product(value: &Value) -> AppResult<SimpleEarnFlexibleProduct> {
    Ok(SimpleEarnFlexibleProduct {
        asset: string_field(value, "asset")?,
        product_id: string_field(value, "productId")?,
        can_purchase: value
            .get("canPurchase")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_sold_out: value
            .get("isSoldOut")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        status: value.get("status").map(value_to_string).unwrap_or_default(),
        min_purchase_amount: optional_decimal_field(value, "minPurchaseAmount")?
            .unwrap_or(Decimal::ZERO),
    })
}

pub(super) fn ensure_success_response(value: &Value, operation: &str) -> AppResult<()> {
    match value.get("success").and_then(Value::as_bool) {
        Some(true) => Ok(()),
        Some(false) => Err(AppError::Binance(format!("{operation}未成功"))),
        None => Err(AppError::Binance(format!(
            "{operation}响应缺少 success 字段"
        ))),
    }
}

fn decimal_from_any_field(value: &Value, keys: &[&str]) -> AppResult<Option<Decimal>> {
    for key in keys {
        if let Some(amount) = optional_decimal_field(value, key)? {
            return Ok(Some(amount));
        }
    }
    Ok(None)
}

fn date_time_from_any_millis_field(value: &Value, keys: &[&str]) -> Option<DateTime<Utc>> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .map(value_to_string)
            .and_then(|text| text.parse::<i64>().ok())
            .and_then(DateTime::<Utc>::from_timestamp_millis)
    })
}

/// 解析现货交易对价格列表。
pub(super) fn parse_spot_price_tickers(value: &Value) -> AppResult<HashMap<String, Decimal>> {
    let rows = value
        .as_array()
        .ok_or_else(|| AppError::Binance("Binance ticker price 响应不是数组".to_string()))?;
    let mut prices = HashMap::with_capacity(rows.len());
    for row in rows {
        prices.insert(string_field(row, "symbol")?, decimal_field(row, "price")?);
    }
    Ok(prices)
}

/// 解析期权合约列表。
pub(super) fn parse_exchange_info(value: &Value) -> AppResult<Vec<OptionContract>> {
    value
        .get("optionSymbols")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AppError::Binance("Binance exchangeInfo 响应缺少 optionSymbols 字段".to_string())
        })?
        .iter()
        .map(parse_option_contract)
        .collect()
}

/// 解析单个期权合约，包含精度和保证金参数。
fn parse_option_contract(value: &Value) -> AppResult<OptionContract> {
    let expiry_ms = value
        .get("expiryDate")
        .and_then(Value::as_i64)
        .ok_or_else(|| AppError::Binance("期权合约缺少 expiryDate".to_string()))?;
    let expiry_at = DateTime::<Utc>::from_timestamp_millis(expiry_ms)
        .ok_or_else(|| AppError::Binance("期权合约 expiryDate 无效".to_string()))?;
    let expiry = expiry_at.date_naive();
    let price_filter = exchange_info_filter(value, "PRICE_FILTER")?;
    let lot_filter = exchange_info_filter(value, "LOT_SIZE")?;

    Ok(OptionContract {
        symbol: string_field(value, "symbol")?,
        side: string_field(value, "side")?,
        strike_price: decimal_field(value, "strikePrice")?,
        underlying: string_field(value, "underlying")?,
        unit: decimal_field(value, "unit")?,
        min_qty: decimal_field(value, "minQty")?,
        max_qty: decimal_field(value, "maxQty")?,
        quantity_step_size: decimal_field(lot_filter, "stepSize")?,
        initial_margin: decimal_field(value, "initialMargin")?,
        maintenance_margin: decimal_field(value, "maintenanceMargin")?,
        min_initial_margin: decimal_field(value, "minInitialMargin")?,
        min_maintenance_margin: decimal_field(value, "minMaintenanceMargin")?,
        min_price: decimal_field(price_filter, "minPrice")?,
        max_price: decimal_field(price_filter, "maxPrice")?,
        price_tick_size: decimal_field(price_filter, "tickSize")?,
        price_scale: u32_field(value, "priceScale")?,
        quantity_scale: u32_field(value, "quantityScale")?,
        quote_asset: string_field(value, "quoteAsset")?,
        status: string_field(value, "status")?,
        expiry,
        expiry_at,
    })
}

fn exchange_info_filter<'a>(value: &'a Value, filter_type: &str) -> AppResult<&'a Value> {
    value
        .get("filters")
        .and_then(Value::as_array)
        .and_then(|filters| {
            filters.iter().find(|filter| {
                filter
                    .get("filterType")
                    .map(value_to_string)
                    .is_some_and(|current| current == filter_type)
            })
        })
        .ok_or_else(|| AppError::Binance(format!("Binance 期权合约缺少 {filter_type} 过滤器")))
}

/// 解析 mark price 列表。
pub(super) fn parse_mark_prices(value: &Value) -> AppResult<Vec<OptionMarkPrice>> {
    value
        .as_array()
        .ok_or_else(|| AppError::Binance("Binance mark 响应不是数组".to_string()))?
        .iter()
        .map(parse_mark_price)
        .collect()
}

/// 解析单条 mark price。
fn parse_mark_price(value: &Value) -> AppResult<OptionMarkPrice> {
    Ok(OptionMarkPrice {
        symbol: string_field(value, "symbol")?,
        mark_price: decimal_field(value, "markPrice")?,
        mark_iv: decimal_field(value, "markIV")?,
        delta: decimal_field(value, "delta")?,
        high_price_limit: optional_decimal_field(value, "highPriceLimit")?.unwrap_or(Decimal::ZERO),
        low_price_limit: optional_decimal_field(value, "lowPriceLimit")?.unwrap_or(Decimal::ZERO),
    })
}

/// 归一化期权/现货订单响应。
///
/// Binance 不同接口的成交数量、均价和手续费字段不完全一致，这里集中归一化。
pub(super) fn parse_order_ack(value: &Value) -> AppResult<ExchangeOrderAck> {
    let order_id = value
        .get("orderId")
        .map(value_to_string)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| AppError::Binance("Binance 下单响应缺少 orderId".to_string()))?;

    let executed_qty = optional_decimal_field(value, "executedQty")?.unwrap_or(Decimal::ZERO);
    let quantity = optional_decimal_field(value, "quantity")?
        .or(optional_decimal_field(value, "origQty")?)
        .unwrap_or(Decimal::ZERO);
    let price = optional_decimal_field(value, "price")?.unwrap_or(Decimal::ZERO);
    let avg_price = optional_decimal_field(value, "avgPrice")?
        .or_else(|| average_price_from_spot_ack(value, executed_qty))
        .unwrap_or(price);
    let fee = optional_decimal_field(value, "fee")?
        .or_else(|| fee_from_fills(value))
        .unwrap_or(Decimal::ZERO);
    let transact_time = value
        .get("transactTime")
        .and_then(Value::as_i64)
        .and_then(DateTime::<Utc>::from_timestamp_millis);

    Ok(ExchangeOrderAck {
        order_id,
        client_order_id: value
            .get("clientOrderId")
            .map(value_to_string)
            .unwrap_or_default(),
        symbol: string_field(value, "symbol")?,
        status: value
            .get("status")
            .map_or_else(|| "UNKNOWN".to_string(), value_to_string),
        executed_qty,
        avg_price,
        price,
        quantity,
        fee,
        create_time: value
            .get("createTime")
            .and_then(Value::as_i64)
            .and_then(DateTime::<Utc>::from_timestamp_millis)
            .or(transact_time),
        update_time: value
            .get("updateTime")
            .and_then(Value::as_i64)
            .and_then(DateTime::<Utc>::from_timestamp_millis)
            .or(transact_time),
    })
}

/// 从现货订单累计成交额反推均价。
fn average_price_from_spot_ack(value: &Value, executed_qty: Decimal) -> Option<Decimal> {
    if executed_qty <= Decimal::ZERO {
        return None;
    }
    let quote_qty = optional_decimal_field(value, "cummulativeQuoteQty")
        .ok()
        .flatten()
        .or_else(|| {
            optional_decimal_field(value, "cumulativeQuoteQty")
                .ok()
                .flatten()
        })?;
    if quote_qty <= Decimal::ZERO {
        None
    } else {
        Some((quote_qty / executed_qty).round_dp(8))
    }
}

/// 从现货 fills 汇总手续费。
fn fee_from_fills(value: &Value) -> Option<Decimal> {
    value
        .get("fills")
        .and_then(Value::as_array)
        .map(|fills| {
            fills
                .iter()
                .filter_map(|fill| optional_decimal_field(fill, "commission").ok().flatten())
                .sum::<Decimal>()
        })
        .filter(|fee| *fee > Decimal::ZERO)
}

/// 解析行权记录。
///
/// Binance 历史接口可能返回数组、`data` 或 `rows` 包装，这里只提取结算价相关字段。
pub(super) fn parse_exercise_records(value: &Value) -> AppResult<Vec<ExerciseRecord>> {
    let rows = value
        .as_array()
        .or_else(|| value.get("data").and_then(Value::as_array))
        .or_else(|| value.get("rows").and_then(Value::as_array))
        .ok_or_else(|| AppError::Binance("Binance 行权记录响应不是数组".to_string()))?;

    rows.iter()
        .map(|row| {
            let settlement_price = optional_decimal_field(row, "settlePrice")?
                .or(optional_decimal_field(row, "settlementPrice")?)
                .or(optional_decimal_field(row, "exercisePrice")?)
                .or(optional_decimal_field(row, "price")?)
                .unwrap_or(Decimal::ZERO);
            Ok(ExerciseRecord {
                symbol: string_field(row, "symbol")?,
                settlement_price,
            })
        })
        .collect()
}

/// 读取必填字符串字段，空字符串视为缺失。
fn string_field(value: &Value, key: &str) -> AppResult<String> {
    value
        .get(key)
        .map(value_to_string)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| AppError::Binance(format!("Binance 响应缺少 {key} 字段")))
}

fn optional_string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .map(value_to_string)
        .filter(|text| !text.trim().is_empty())
}

/// 读取必填 u32 字段。
fn u32_field(value: &Value, key: &str) -> AppResult<u32> {
    let text = value
        .get(key)
        .map(value_to_string)
        .ok_or_else(|| AppError::Binance(format!("Binance 响应缺少 {key} 字段")))?;
    text.parse::<u32>()
        .map_err(|error| AppError::Binance(format!("Binance 字段 {key} 无法解析：{error}")))
}

/// 读取必填 Decimal 字段。
pub(super) fn decimal_field(value: &Value, key: &str) -> AppResult<Decimal> {
    optional_decimal_field(value, key)?
        .ok_or_else(|| AppError::Binance(format!("Binance 响应缺少 {key} 字段")))
}

/// 读取可选 Decimal 字段。
///
/// Binance 数字字段可能以字符串或数字形式返回，统一先转字符串再解析成 Decimal。
fn optional_decimal_field(value: &Value, key: &str) -> AppResult<Option<Decimal>> {
    value
        .get(key)
        .map(value_to_string)
        .filter(|text| !text.trim().is_empty())
        .map(|text| {
            Decimal::from_str(&text)
                .map_err(|error| AppError::Binance(format!("Binance 字段 {key} 无法解析：{error}")))
        })
        .transpose()
}

/// 把 JSON 标量转为字符串；数组和对象不参与金融字段解析。
pub(super) fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null | Value::Array(_) | Value::Object(_) => String::new(),
    }
}
