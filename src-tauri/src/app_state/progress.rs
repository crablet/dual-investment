//! 下单进度事件发送。
//!
//! 事件只用于前端交互反馈，内容必须是脱敏摘要，不能作为订单或资金事实来源。

use chrono::Utc;
use tauri::{AppHandle, Emitter};

use crate::domain::{OrderProgressEvent, OrderProgressStage, OrderProgressStatus, StrategyRequest};

const ORDER_PROGRESS_EVENT: &str = "order-progress";

pub(super) struct OrderProgressEmitter {
    app: Option<AppHandle>,
    flow_id: Option<String>,
    source: String,
}

impl OrderProgressEmitter {
    pub(super) fn manual(app: AppHandle, request: &StrategyRequest) -> Self {
        Self {
            app: Some(app),
            flow_id: request.client_flow_id.clone(),
            source: "manual".to_string(),
        }
    }

    pub(super) fn disabled(source: &str) -> Self {
        Self {
            app: None,
            flow_id: None,
            source: source.to_string(),
        }
    }

    /// 向前端发送脱敏进度事件。
    ///
    /// 自动策略没有前端 flow id，因此使用 disabled emitter 静默执行。
    pub(super) fn emit(
        &self,
        stage: OrderProgressStage,
        status: OrderProgressStatus,
        message: impl Into<String>,
        option_symbol: Option<&str>,
        exchange_order_id: Option<&str>,
    ) {
        let (Some(app), Some(flow_id)) = (&self.app, &self.flow_id) else {
            return;
        };
        let event = OrderProgressEvent {
            flow_id: flow_id.clone(),
            source: self.source.clone(),
            stage,
            status,
            message: message.into(),
            timestamp: Utc::now(),
            option_symbol: option_symbol.map(str::to_string),
            exchange_order_id: exchange_order_id.map(str::to_string),
        };
        let _ = app.emit(ORDER_PROGRESS_EVENT, event);
    }
}
