//! 编码签名参数并提取网络错误。错误只保留方法、路径和原因，重试规则见 errors.rs。

use crate::errors::{AppError, BinanceErrorKind};

/// 把 `reqwest` 错误转成脱敏的 Binance 错误消息。
pub(super) fn request_error(method: &str, path: &str, error: &reqwest::Error) -> AppError {
    let kind = if error.is_timeout() {
        BinanceErrorKind::Timeout
    } else if error.is_connect() {
        BinanceErrorKind::Connection
    } else if error.is_decode() {
        BinanceErrorKind::Decode
    } else if error.is_body() {
        BinanceErrorKind::BodyRead
    } else if error.is_request() {
        BinanceErrorKind::Request
    } else {
        BinanceErrorKind::Network
    };
    AppError::binance(kind, request_context(method, path))
}

/// 错误中只保留方法和接口路径，剥离所有查询参数。
pub(super) fn request_context(method: &str, path: &str) -> String {
    let endpoint = path.split('?').next().unwrap_or(path);
    format!("{method} {endpoint}")
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
