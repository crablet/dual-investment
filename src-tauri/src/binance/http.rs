use crate::domain::AppError;

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

pub(super) fn is_timestamp_error(error: &AppError) -> bool {
    matches!(
        error,
        AppError::Binance(message)
            if message.contains("code -1021")
                || message.contains("timestamp")
                || message.contains("Timestamp")
    )
}

pub(super) fn is_transient_query_error(error: &AppError) -> bool {
    matches!(
        error,
        AppError::Binance(message)
            if message.contains("请求超时")
                || message.contains("连接失败")
                || message.contains("响应体读取失败")
                || message.contains("网络请求失败")
                || message.contains("HTTP 429")
                || message.contains("HTTP 500")
                || message.contains("HTTP 502")
                || message.contains("HTTP 503")
                || message.contains("HTTP 504")
    )
}

/// 构造 Binance 签名原文。
pub(super) fn encode_query(params: &[(String, String)]) -> String {
    params
        .iter()
        .map(|(key, value)| format!("{key}={}", urlencoding::encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}
