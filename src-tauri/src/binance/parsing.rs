//! Binance JSON 响应解析。
//!
//! 本模块把交易所返回的字段转换为领域 DTO，并在缺少关键字段时返回明确错误。解析阶段
//! 不保留原始响应体，金额字段统一走 `Decimal`。

use crate::messages::Message;

use std::{collections::HashMap, str::FromStr};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde_json::Value;

use crate::errors::BinanceErrorKind;

use crate::domain::{
    AppError, AppResult, ExchangeOrderAck, ExerciseRecord, OptionAccountSnapshot,
    OptionAssetBalance, OptionContract, OptionMarkPrice, OptionOrderBook, OptionPosition,
    OptionTicker, OrderBookLevel, SimpleEarnAssetBalance, SimpleEarnFlexibleProduct,
    SimpleEarnRedemptionRecord, SimpleEarnSubscriptionRecord, SpotAccountSnapshot,
    SpotAssetBalance, UniversalTransferRecord,
};

/// 解析期权账户快照，字段名保持贴近 Binance EAPI 响应。
pub(super) fn parse_account_snapshot(value: &Value) -> AppResult<OptionAccountSnapshot> {
    let assets = value
        .get("asset")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AppError::response(crate::msg!("backend.binanceAccountResponseIsMissingAsset"))
        })?
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
    let rows = value.as_array().ok_or_else(|| {
        AppError::response(crate::msg!("backend.binanceOptionPositionResponseIsNotAn"))
    })?;

    rows.iter()
        .map(parse_option_position)
        .filter(|result| {
            result
                .as_ref()
                .map_or(true, |position| position.quantity > Decimal::ZERO)
        })
        .collect()
}

/// 解析当前 EAPI 持仓字段；数量负号表示义务仓，领域数量统一保留绝对值。
fn parse_option_position(value: &Value) -> AppResult<OptionPosition> {
    Ok(OptionPosition {
        symbol: string_field(value, "symbol")?,
        side: string_field(value, "side")?,
        quantity: decimal_field(value, "quantity")?.abs(),
        entry_price: decimal_field(value, "entryPrice")?,
        mark_price: optional_decimal_field(value, "markPrice")?.unwrap_or(Decimal::ZERO),
        update_time: date_time_from_millis_field(value, "time"),
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
        adjusted_equity: decimal_field(value, "adjustedEquity")?,
    })
}

/// 解析现货账户快照。
pub(super) fn parse_spot_account(value: &Value) -> AppResult<SpotAccountSnapshot> {
    let uid = value
        .get("uid")
        .and_then(Value::as_u64)
        .filter(|uid| *uid > 0)
        .ok_or_else(|| {
            AppError::response(crate::msg!(
                "backend.binanceSpotAccountResponseHasInvalidUid"
            ))
        })?
        .to_string();
    let balances = value
        .get("balances")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AppError::response(crate::msg!(
                "backend.binanceSpotAccountResponseIsMissingBalances"
            ))
        })?
        .iter()
        .map(parse_spot_asset_balance)
        .collect::<AppResult<Vec<_>>>()?;

    Ok(SpotAccountSnapshot {
        uid,
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
    amount_key: &str,
    flexible: bool,
) -> AppResult<SimpleEarnPositionPage> {
    let rows = value
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AppError::binance(
                BinanceErrorKind::MissingRows,
                crate::msg!("backend.binanceSimpleEarnResponseIsMissingRows"),
            )
        })?
        .iter()
        .map(|row| parse_simple_earn_position(row, source, amount_key, flexible))
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
    amount_key: &str,
    flexible: bool,
) -> AppResult<SimpleEarnAssetBalance> {
    Ok(SimpleEarnAssetBalance {
        asset: string_field(value, "asset")?,
        source: source.to_string(),
        product_id: optional_string_field(value, "productId").unwrap_or_default(),
        amount: decimal_field(value, amount_key)?,
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
            AppError::binance(
                BinanceErrorKind::MissingRows,
                crate::msg!("backend.binanceSimpleEarnProductListResponseIs"),
            )
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

pub(super) fn ensure_success_response(value: &Value, operation: &Message) -> AppResult<()> {
    match value.get("success").and_then(Value::as_bool) {
        Some(true) => Ok(()),
        Some(false) => Err(AppError::response(crate::msg!(
            "backend.didNotSucceed",
            p0 = operation
        ))),
        None => Err(AppError::response(crate::msg!(
            "backend.responseIsMissingSuccess",
            p0 = operation
        ))),
    }
}

/// 解析 Universal Transfer 历史分页。
pub(super) fn parse_universal_transfer_records(
    value: &Value,
) -> AppResult<Vec<UniversalTransferRecord>> {
    history_rows(value, &crate::msg!("backend.binanceTransferHistory"))?
        .iter()
        .map(parse_universal_transfer_record)
        .collect()
}

fn parse_universal_transfer_record(value: &Value) -> AppResult<UniversalTransferRecord> {
    Ok(UniversalTransferRecord {
        transfer_type: string_field(value, "type")?,
        asset: string_field(value, "asset")?,
        amount: decimal_field(value, "amount")?,
        status: string_field(value, "status")?,
        tran_id: string_field(value, "tranId")?,
    })
}

/// 解析 Simple Earn 活期申购历史分页。
pub(super) fn parse_simple_earn_subscription_records(
    value: &Value,
) -> AppResult<Vec<SimpleEarnSubscriptionRecord>> {
    history_rows(
        value,
        &crate::msg!("backend.binanceSimpleEarnSubscriptionHistory"),
    )?
    .iter()
    .map(parse_simple_earn_subscription_record)
    .collect()
}

fn parse_simple_earn_subscription_record(value: &Value) -> AppResult<SimpleEarnSubscriptionRecord> {
    Ok(SimpleEarnSubscriptionRecord {
        product_id: string_field(value, "productId")?,
        asset: string_field(value, "asset")?,
        amount: decimal_field(value, "amount")?,
        status: string_field(value, "status")?,
        purchase_id: string_field(value, "purchaseId")?,
    })
}

/// 解析 Simple Earn 活期赎回历史分页。
pub(super) fn parse_simple_earn_redemption_records(
    value: &Value,
) -> AppResult<Vec<SimpleEarnRedemptionRecord>> {
    history_rows(
        value,
        &crate::msg!("backend.binanceSimpleEarnRedemptionHistory"),
    )?
    .iter()
    .map(parse_simple_earn_redemption_record)
    .collect()
}

fn history_rows<'a>(value: &'a Value, operation: &Message) -> AppResult<&'a [Value]> {
    if let Some(rows) = value.get("rows").and_then(Value::as_array) {
        return Ok(rows.as_slice());
    }
    let total_is_zero = value.get("total").is_some_and(|total| {
        total.as_u64() == Some(0) || total.as_str().is_some_and(|text| text == "0")
    });
    if total_is_zero {
        return Ok(&[]);
    }
    Err(AppError::binance(
        BinanceErrorKind::MissingRows,
        crate::msg!("backend.responseIsMissingRows", p0 = operation),
    ))
}

fn parse_simple_earn_redemption_record(value: &Value) -> AppResult<SimpleEarnRedemptionRecord> {
    Ok(SimpleEarnRedemptionRecord {
        product_id: string_field(value, "productId")?,
        asset: string_field(value, "asset")?,
        amount: decimal_field(value, "amount")?,
        status: string_field(value, "status")?,
        redeem_id: string_field(value, "redeemId")?,
    })
}

fn date_time_from_millis_field(value: &Value, key: &str) -> Option<DateTime<Utc>> {
    value
        .get(key)
        .map(value_to_string)
        .and_then(|text| text.parse::<i64>().ok())
        .and_then(DateTime::<Utc>::from_timestamp_millis)
}

/// 解析现货交易对价格列表。
pub(super) fn parse_spot_price_tickers(value: &Value) -> AppResult<HashMap<String, Decimal>> {
    let rows = value.as_array().ok_or_else(|| {
        AppError::response(crate::msg!("backend.binanceTickerPriceResponseIsNotAn"))
    })?;
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
            AppError::response(crate::msg!(
                "backend.binanceExchangeinfoResponseIsMissingOptionsymbols"
            ))
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
        .ok_or_else(|| {
            AppError::response(crate::msg!("backend.optionContractIsMissingExpirydate"))
        })?;
    let expiry_at = DateTime::<Utc>::from_timestamp_millis(expiry_ms).ok_or_else(|| {
        AppError::response(crate::msg!("backend.optionContractExpirydateIsInvalid"))
    })?;
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
        expiry_at,
    })
}

fn exchange_info_filter<'a>(value: &'a Value, filter_type: &str) -> AppResult<&'a Value> {
    // 价格 tickSize 和数量 stepSize 是下单硬约束，缺失时必须暴露为数据契约错误。
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
        .ok_or_else(|| {
            AppError::response(crate::msg!(
                "backend.binanceOptionContractIsMissingFilter",
                p0 = filter_type
            ))
        })
}

/// 解析 mark price 列表。
pub(super) fn parse_mark_prices(value: &Value) -> AppResult<Vec<OptionMarkPrice>> {
    value
        .as_array()
        .ok_or_else(|| AppError::response(crate::msg!("backend.binanceMarkResponseIsNotAnArray")))?
        .iter()
        .map(parse_mark_price)
        .collect()
}

/// 解析单条 mark price。
fn parse_mark_price(value: &Value) -> AppResult<OptionMarkPrice> {
    Ok(OptionMarkPrice {
        symbol: string_field(value, "symbol")?,
        mark_price: decimal_field(value, "markPrice")?,
        delta: decimal_field(value, "delta")?,
        high_price_limit: optional_decimal_field(value, "highPriceLimit")?.unwrap_or(Decimal::ZERO),
        low_price_limit: optional_decimal_field(value, "lowPriceLimit")?.unwrap_or(Decimal::ZERO),
    })
}

/// 解析期权 24h ticker 列表。
pub(super) fn parse_option_tickers(value: &Value) -> AppResult<Vec<OptionTicker>> {
    value
        .as_array()
        .ok_or_else(|| {
            AppError::response(crate::msg!("backend.binanceOptionTickerResponseIsNotAn"))
        })?
        .iter()
        .map(parse_option_ticker)
        .collect()
}

fn parse_option_ticker(value: &Value) -> AppResult<OptionTicker> {
    Ok(OptionTicker {
        symbol: string_field(value, "symbol")?,
        bid_price: optional_decimal_field(value, "bidPrice")?.unwrap_or(Decimal::ZERO),
    })
}

/// 解析单个期权订单簿。
pub(super) fn parse_option_order_book(value: &Value) -> AppResult<OptionOrderBook> {
    Ok(OptionOrderBook {
        bids: parse_order_book_side(value, "bids", &crate::msg!("backend.bids"))?,
    })
}

fn parse_order_book_side(
    value: &Value,
    key: &str,
    label: &Message,
) -> AppResult<Vec<OrderBookLevel>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| {
            AppError::response(crate::msg!(
                "backend.binanceOrderBookResponseIsMissingThe",
                p0 = label
            ))
        })?
        .iter()
        .map(|level| parse_order_book_level(level, label))
        .collect()
}

fn parse_order_book_level(value: &Value, label: &Message) -> AppResult<OrderBookLevel> {
    let values = value.as_array().ok_or_else(|| {
        AppError::response(crate::msg!(
            "backend.binanceOrderBookLevelIsNotAn",
            p0 = label
        ))
    })?;
    let price = decimal_array_value(values, 0, label, &crate::msg!("backend.price"))?;
    let quantity = decimal_array_value(values, 1, label, &crate::msg!("backend.quantity"))?;
    Ok(OrderBookLevel { price, quantity })
}

fn decimal_array_value(
    values: &[Value],
    index: usize,
    label: &Message,
    field: &Message,
) -> AppResult<Decimal> {
    let text = values
        .get(index)
        .map(value_to_string)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| {
            AppError::response(crate::msg!(
                "backend.binanceOrderBookIsMissing",
                p0 = label,
                p1 = field
            ))
        })?;
    Decimal::from_str(&text).map_err(|_| {
        AppError::response(crate::msg!(
            "backend.cannotParseBinanceOrderBook",
            p0 = label,
            p1 = field
        ))
    })
}

/// 归一化期权/现货订单响应。
///
/// Binance 不同接口的成交数量和均价字段不完全一致，这里集中归一化。
pub(super) fn parse_order_ack(value: &Value) -> AppResult<ExchangeOrderAck> {
    let order_id = value
        .get("orderId")
        .map(value_to_string)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| {
            AppError::response(crate::msg!("backend.binanceOrderResponseIsMissingOrderid"))
        })?;

    let executed_qty = optional_decimal_field(value, "executedQty")?.unwrap_or(Decimal::ZERO);
    if executed_qty < Decimal::ZERO {
        return Err(AppError::response(crate::msg!(
            "backend.binanceFilledQuantityCannotBeNegative"
        )));
    }
    let quantity = optional_decimal_field(value, "quantity")?
        .or(optional_decimal_field(value, "origQty")?)
        .unwrap_or(Decimal::ZERO);
    let price = optional_decimal_field(value, "price")?.unwrap_or(Decimal::ZERO);
    // 委托限价不是成交事实；零成交不产生均价，已成交订单必须提供可验证的实际均价。
    let avg_price = if executed_qty == Decimal::ZERO {
        Decimal::ZERO
    } else {
        let avg_price = match optional_decimal_field(value, "avgPrice")? {
            Some(avg_price) => avg_price,
            None => average_price_from_spot_ack(value, executed_qty)?,
        };
        if avg_price <= Decimal::ZERO {
            return Err(AppError::response(crate::msg!(
                "backend.filledBinanceOrderHasNoValidAverage"
            )));
        }
        avg_price
    };
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
fn average_price_from_spot_ack(value: &Value, executed_qty: Decimal) -> AppResult<Decimal> {
    // 现货市价单常只返回累计成交额，均价需要用 quoteQty / executedQty 反推。
    if executed_qty <= Decimal::ZERO {
        return Err(AppError::response(crate::msg!(
            "backend.binanceSpotAverageFillPriceRequiresA"
        )));
    }
    let quote_qty = decimal_field(value, "cummulativeQuoteQty")?;
    if quote_qty <= Decimal::ZERO {
        return Err(AppError::response(crate::msg!(
            "backend.filledBinanceSpotOrderHasNoValid"
        )));
    }
    Ok((quote_qty / executed_qty).round_dp(8))
}

/// 解析行权记录。
///
/// 当前 EAPI 行权记录返回数组，行权价由 exercisePrice 提供。
pub(super) fn parse_exercise_records(value: &Value) -> AppResult<Vec<ExerciseRecord>> {
    let rows = value.as_array().ok_or_else(|| {
        AppError::response(crate::msg!("backend.binanceExerciseRecordResponseIsNotAn"))
    })?;

    rows.iter()
        .map(|row| {
            Ok(ExerciseRecord {
                symbol: string_field(row, "symbol")?,
                settlement_price: decimal_field(row, "exercisePrice")?,
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
        .ok_or_else(|| {
            AppError::response(crate::msg!(
                "backend.binanceResponseIsMissingField",
                p0 = key
            ))
        })
}

fn optional_string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .map(value_to_string)
        .filter(|text| !text.trim().is_empty())
}

/// 读取必填 u32 字段。
fn u32_field(value: &Value, key: &str) -> AppResult<u32> {
    let text = value.get(key).map(value_to_string).ok_or_else(|| {
        AppError::response(crate::msg!(
            "backend.binanceResponseIsMissingField",
            p0 = key
        ))
    })?;
    text.parse::<u32>()
        .map_err(|_| AppError::response(crate::msg!("backend.cannotParseBinanceField", p0 = key)))
}

/// 读取必填 Decimal 字段。
pub(super) fn decimal_field(value: &Value, key: &str) -> AppResult<Decimal> {
    optional_decimal_field(value, key)?.ok_or_else(|| {
        AppError::response(crate::msg!(
            "backend.binanceResponseIsMissingField",
            p0 = key
        ))
    })
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
            Decimal::from_str(&text).map_err(|_| {
                AppError::response(crate::msg!("backend.cannotParseBinanceField", p0 = key))
            })
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
