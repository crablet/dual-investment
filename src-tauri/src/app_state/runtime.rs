//! 运行期辅助函数。
//!
//! 这里封装 Binance 客户端重建、异步查询任务、计划时间解析和客户端订单号生成。
//! 它只处理内存中的运行状态，不直接读取或写入 keyring。

use std::future::Future;

use chrono::{NaiveTime, Utc};

use super::RuntimeConfig;
use crate::binance::BinanceClient;
use crate::domain::{AppError, AppResult, StrategyDirection};

pub(super) async fn verify_binance_access(client: &BinanceClient) -> AppResult<()> {
    let option_account_task = spawn_binance_request({
        let client = client.clone();
        async move { client.verify_account().await }
    });
    let spot_account_task = spawn_binance_request({
        let client = client.clone();
        async move { client.spot_account().await }
    });
    let option_account = join_binance_request(option_account_task).await;
    let spot_account = join_binance_request(spot_account_task).await;
    option_account?;
    let spot_account = spot_account?;
    if !spot_account.can_trade {
        return Err(AppError::Config(
            "Binance 现货账户当前不可交易，无法执行到期补齐".to_string(),
        ));
    }
    Ok(())
}

/// 在 Tauri 异步运行时里启动一个 Binance 查询任务。
pub(super) fn spawn_binance_request<T, F>(
    future: F,
) -> tauri::async_runtime::JoinHandle<AppResult<T>>
where
    T: Send + 'static,
    F: Future<Output = AppResult<T>> + Send + 'static,
{
    tauri::async_runtime::spawn(future)
}

/// 等待 Binance 子任务并把 join 错误转换成业务错误。
pub(super) async fn join_binance_request<T>(
    handle: tauri::async_runtime::JoinHandle<AppResult<T>>,
) -> AppResult<T> {
    handle
        .await
        .map_err(|error| AppError::Binance(format!("Binance 异步请求任务失败：{error}")))?
}

/// 从运行期配置重建 Binance 客户端。
///
/// 这里读取的是内存凭据；持久化凭据只允许通过 `secrets.rs` 从 keyring 读取。
pub(super) fn client_from_runtime(runtime: &RuntimeConfig) -> AppResult<BinanceClient> {
    if !runtime.connected {
        return Err(AppError::Config("请先连接 Binance API".to_string()));
    }
    let api_key = runtime
        .api_key
        .clone()
        .ok_or_else(|| AppError::Config("缺少 Binance API Key".to_string()))?;
    let api_secret = runtime
        .api_secret
        .clone()
        .ok_or_else(|| AppError::Config("缺少 Binance API Secret".to_string()))?;
    BinanceClient::new(api_key, api_secret)
}

/// 从中文计划字符串中提取执行时间，无法解析时使用默认时间。
pub(super) fn parse_schedule_time(schedule: &str) -> NaiveTime {
    for segment in schedule
        .split(|character: char| character.is_whitespace() || character == ',' || character == '，')
    {
        if let Some((hour, minute)) = segment.split_once(':')
            && let (Ok(hour), Ok(minute)) = (hour.parse::<u32>(), minute.parse::<u32>())
            && let Some(time) = NaiveTime::from_hms_opt(hour, minute, 0)
        {
            return time;
        }
    }
    NaiveTime::from_hms_opt(11, 30, 0).expect("default auto schedule time is valid")
}

/// 生成期权订单客户端订单号。
pub(super) fn make_client_order_id(direction: StrategyDirection) -> String {
    format!(
        "di-{}-{}",
        direction.option_type().to_lowercase(),
        Utc::now().timestamp_millis()
    )
}

/// 生成现货补齐订单客户端订单号。
pub(super) fn make_spot_client_order_id(direction: StrategyDirection) -> String {
    format!(
        "di-spot-{}-{}",
        direction.option_type().to_lowercase(),
        Utc::now().timestamp_millis()
    )
}

/// 脱敏 API Key，只保留首尾少量字符用于确认连接身份。
pub(super) fn mask_api_key(api_key: &str) -> String {
    let trimmed = api_key.trim();
    if trimmed.len() <= 8 {
        return "****".to_string();
    }
    let prefix = &trimmed[..4];
    let suffix = &trimmed[trimmed.len() - 4..];
    format!("{prefix}...{suffix}")
}
