//! 下单进度事件发送。
//!
//! 事件只用于前端交互反馈，内容必须是脱敏摘要，不能作为订单或资金事实来源。

use crate::messages::Message;

use tauri::{AppHandle, Emitter};

use crate::domain::{OrderProgressEvent, OrderProgressStage, OrderProgressStatus, StrategyRequest};

const ORDER_PROGRESS_EVENT: &str = "order-progress";

pub(super) struct OrderProgressEmitter {
    app: AppHandle,
    flow_id: String,
}

impl OrderProgressEmitter {
    pub(super) fn manual(app: AppHandle, request: &StrategyRequest) -> Self {
        Self {
            app,
            flow_id: request.client_flow_id.clone(),
        }
    }

    /// 向前端发送脱敏进度事件。
    ///
    pub(super) fn emit(
        &self,
        stage: OrderProgressStage,
        status: OrderProgressStatus,
        message: Message,
        option_symbol: Option<&str>,
        exchange_order_id: Option<&str>,
    ) {
        let event = OrderProgressEvent {
            flow_id: self.flow_id.clone(),
            stage,
            status,
            message,
            option_symbol: option_symbol.map(str::to_string),
            exchange_order_id: exchange_order_id.map(str::to_string),
        };
        let _ = self.app.emit(ORDER_PROGRESS_EVENT, event);
    }
}
