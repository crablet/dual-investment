//! 业务错误与 Binance 失败分类。
//!
//! 恢复、重试和连接策略只读取结构化原因；展示文字不参与交易决策。
//! Binance 错误上下文必须由本地生成，不保存原始响应、交易所 msg 或签名参数。

use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("配置错误：{0}")]
    Config(String),
    #[error("交易被风控拒绝：{0}")]
    RiskRejected(String),
    #[error("币安接口错误：{0}")]
    Binance(BinanceError),
    #[error("数据库错误：{0}")]
    Storage(String),
}

pub type AppResult<T> = Result<T, AppError>;

/// HTTP 状态和交易所错误码只在收到对应响应后填写。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinanceErrorKind {
    Timeout,
    Connection,
    BodyRead,
    Network,
    Request,
    Decode,
    MissingRows,
    Response,
    Http { status: u16, code: Option<i64> },
}

#[derive(Debug)]
pub struct BinanceError {
    kind: BinanceErrorKind,
    context: String,
}

impl AppError {
    pub fn binance(kind: BinanceErrorKind, context: impl Into<String>) -> Self {
        Self::Binance(BinanceError {
            kind,
            context: context.into(),
        })
    }

    /// 调用方只传本地构造的契约说明，不包含 Binance 响应值。
    pub fn response(context: impl Into<String>) -> Self {
        Self::binance(BinanceErrorKind::Response, context)
    }

    const fn binance_kind(&self) -> Option<BinanceErrorKind> {
        match self {
            Self::Binance(error) => Some(error.kind),
            _ => None,
        }
    }

    pub const fn classification(&self) -> &'static str {
        match self {
            Self::Config(_) => "config",
            Self::RiskRejected(_) => "risk_rejected",
            Self::Storage(_) => "storage",
            Self::Binance(error) => match error.kind {
                BinanceErrorKind::Timeout => "timeout",
                BinanceErrorKind::Connection => "connect_failed",
                BinanceErrorKind::BodyRead => "body_read_failed",
                BinanceErrorKind::Network => "network_failed",
                BinanceErrorKind::Request => "request_failed",
                BinanceErrorKind::Decode => "decode_failed",
                BinanceErrorKind::MissingRows => "missing_rows",
                BinanceErrorKind::Response => "response_contract",
                BinanceErrorKind::Http {
                    code: Some(-1006 | -1007),
                    ..
                } => "result_unknown",
                BinanceErrorKind::Http { status: 429, .. } => "rate_limited",
                BinanceErrorKind::Http {
                    status: 500..=599, ..
                } => "binance_server_error",
                BinanceErrorKind::Http { .. } => "binance_http",
            },
        }
    }

    /// 仅查询可使用短退避；此结果不能用来重发订单或资金动作。
    pub const fn is_retryable_query(&self) -> bool {
        matches!(
            self.binance_kind(),
            Some(
                BinanceErrorKind::Timeout
                    | BinanceErrorKind::Connection
                    | BinanceErrorKind::BodyRead
                    | BinanceErrorKind::Network
                    | BinanceErrorKind::Decode
                    | BinanceErrorKind::MissingRows
                    | BinanceErrorKind::Http {
                        status: 408 | 429 | 500..=599,
                        ..
                    }
                    | BinanceErrorKind::Http {
                        code: Some(-1006 | -1007),
                        ..
                    }
            )
        )
    }

    /// 只接受交易所明确的时间戳拒绝码；文案中出现 timestamp 不构成重发依据。
    pub const fn is_timestamp_error(&self) -> bool {
        matches!(
            self.binance_kind(),
            Some(BinanceErrorKind::Http {
                code: Some(-1021),
                ..
            })
        )
    }

    /// 只有明确的 HTTP 拒绝可判定提交失败，其余交易所错误必须先查询原请求。
    pub const fn is_uncertain_submit(&self) -> bool {
        match self.binance_kind() {
            Some(BinanceErrorKind::Http { status, code }) => {
                !matches!(status, 400..=499)
                    || matches!(status, 408 | 429)
                    || matches!(code, Some(-1006 | -1007))
            }
            Some(_) => true,
            None => false,
        }
    }

    /// 仅供连接失败后的自动重连使用，不能授权任何交易或资金请求。
    pub const fn can_auto_reconnect(&self) -> bool {
        !matches!(
            self.binance_kind(),
            Some(BinanceErrorKind::Http {
                code: Some(-1002 | -1022 | -2014 | -2015),
                ..
            })
        ) && (self.is_retryable_query() || self.is_timestamp_error())
    }
}

impl fmt::Display for BinanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self.kind {
            BinanceErrorKind::Timeout => "请求超时",
            BinanceErrorKind::Connection => "连接失败",
            BinanceErrorKind::BodyRead => "响应体读取失败",
            BinanceErrorKind::Network => "网络请求失败",
            BinanceErrorKind::Request => "请求构造失败",
            BinanceErrorKind::Decode => "响应解码失败",
            BinanceErrorKind::MissingRows | BinanceErrorKind::Response => {
                return formatter.write_str(&self.context);
            }
            BinanceErrorKind::Http { status, code } => {
                write!(
                    formatter,
                    "Binance {} 请求失败（HTTP {status}",
                    self.context
                )?;
                if let Some(code) = code {
                    write!(formatter, ", code {code}")?;
                }
                return formatter.write_str("）");
            }
        };
        write!(formatter, "Binance {} {reason}", self.context)
    }
}
