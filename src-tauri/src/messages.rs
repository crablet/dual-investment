//! 跨 IPC、SQLite 和日志的可翻译消息。只保存稳定代码与本地构造的安全参数。
//! 语言在前端渲染时决定，持久化记录不会因设备语言而改变含义。

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, sync::LazyLock};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub code: String,
    pub params: BTreeMap<String, MessageParam>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum MessageParam {
    Text(String),
    Message(Box<Message>),
}

impl Message {
    pub fn new(code: &str, params: BTreeMap<String, MessageParam>) -> Self {
        Self {
            code: code.into(),
            params,
        }
    }

    /// 技术错误只保留原有本地详情；禁止传 Binance 原始响应或签名请求。
    pub fn diagnostic(text: impl Into<String>) -> Self {
        Self::new(
            "ui.literal",
            BTreeMap::from([("text".into(), MessageParam::Text(text.into()))]),
        )
    }

    pub fn is_empty(&self) -> bool {
        self.code == "ui.empty"
    }
}

impl Default for Message {
    fn default() -> Self {
        Self::new("ui.empty", BTreeMap::new())
    }
}

impl From<String> for Message {
    fn from(value: String) -> Self {
        Self::diagnostic(value)
    }
}

impl From<&str> for Message {
    fn from(value: &str) -> Self {
        Self::diagnostic(value)
    }
}

impl From<&Message> for MessageParam {
    fn from(value: &Message) -> Self {
        Self::Message(Box::new(value.clone()))
    }
}

impl From<&&Message> for MessageParam {
    fn from(value: &&Message) -> Self {
        Self::from(*value)
    }
}

impl From<&crate::errors::AppError> for MessageParam {
    fn from(value: &crate::errors::AppError) -> Self {
        Self::from(&value.message())
    }
}

impl From<&&crate::errors::AppError> for MessageParam {
    fn from(value: &&crate::errors::AppError) -> Self {
        Self::from(*value)
    }
}

macro_rules! text_param {
    ($($type:ty),* $(,)?) => { $(
        impl From<&$type> for MessageParam {
            fn from(value: &$type) -> Self { Self::Text(value.to_string()) }
        }
    )* };
}
text_param!(
    str,
    &str,
    String,
    &String,
    usize,
    u32,
    u64,
    i64,
    u16,
    rust_decimal::Decimal,
    std::io::Error,
    serde_json::Error,
    rusqlite::Error,
    rust_decimal::Error,
    std::num::TryFromIntError
);

/// 仅进程诊断使用中文目录；IPC 和持久化始终序列化消息结构。
impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        static CATALOG: LazyLock<serde_json::Value> = LazyLock::new(|| {
            serde_json::from_str(include_str!("../../src/locales/zh-CN.json"))
                .expect("the bundled message catalog must be valid JSON")
        });
        let mut value = &*CATALOG;
        for part in self.code.split('.') {
            value = &value[part];
        }
        let template = value.as_str().unwrap_or(&self.code);
        let mut rest = template;
        while let Some(start) = rest.find('{') {
            f.write_str(&rest[..start])?;
            let Some(end) = rest[start..].find('}') else {
                return f.write_str(&rest[start..]);
            };
            let key = &rest[start + 1..start + end];
            if let Some(param) = self.params.get(key) {
                match param {
                    MessageParam::Text(text) => f.write_str(text)?,
                    MessageParam::Message(message) => write!(f, "{message}")?,
                }
            }
            rest = &rest[start + end + 1..];
        }
        f.write_str(rest)
    }
}

impl ToSql for Message {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        serde_json::to_string(self)
            .map(|value| ToSqlOutput::Owned(value.into()))
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
    }
}

impl FromSql for Message {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        serde_json::from_str(value.as_str()?).map_err(|error| FromSqlError::Other(Box::new(error)))
    }
}

#[macro_export]
macro_rules! msg {
    ($code:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::messages::Message::new($code, std::collections::BTreeMap::from([
            $((stringify!($name).to_owned(), $crate::messages::MessageParam::from(&$value))),*
        ]))
    };
}
