//! Binance HTTP 辅助函数。
//!
//! 这里集中处理网络错误分类、查询类重试判断和签名 query 编码。错误消息只保留方法、
//! 路径和原因，不能包含 API Secret、签名原文或 Binance 原始响应。

use crate::domain::AppError;

/// 把 `reqwest` 错误转成脱敏的 Binance 错误消息。
pub(super) fn request_error(method: &str, path: &str, error: &reqwest::Error) -> AppError {
    let reason = if error.is_timeout() {
        "请求超时"
    } else if error.is_connect() {
        "连接失败"
    } else if error.is_decode() {
        "响应解码失败"
    } else if error.is_body() {
        "响应体读取失败"
    } else if error.is_request() {
        "请求构造失败"
    } else {
        "网络请求失败"
    };
    AppError::Binance(format!("Binance {method} {path} {reason}"))
}

/// 判断是否为 Binance 时间戳偏移错误，可通过重新同步 server time 后重试。
pub(super) fn is_timestamp_error(error: &AppError) -> bool {
    matches!(
        error,
        AppError::Binance(message)
            if message.contains("code -1021")
                || message.contains("timestamp")
                || message.contains("Timestamp")
    )
}

/// 判断查询类 GET 请求是否可短退避重试；真实资金/下单动作不能套用这个判断盲目重发。
pub(super) fn is_transient_query_error(error: &AppError) -> bool {
    matches!(
        error,
        AppError::Binance(message)
            if message.contains("请求超时")
                || message.contains("连接失败")
                || message.contains("响应体读取失败")
                || message.contains("网络请求失败")
                || message.contains("响应缺少 rows 字段")
                || message.contains("HTTP 429")
                || message.contains("HTTP 500")
                || message.contains("HTTP 502")
                || message.contains("HTTP 503")
                || message.contains("HTTP 504")
    )
}

/// 构造 Binance 签名原文。
///
/// 返回值只用于本次签名和发送请求，调用方不得写入日志、SQLite 或前端事件。
pub(super) fn encode_query(params: &[(String, String)]) -> String {
    params
        .iter()
        .map(|(key, value)| format!("{key}={}", urlencoding::encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}
