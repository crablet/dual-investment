//! 运行期辅助函数。
//!
//! 这里封装 Binance 权限验证和客户端订单号生成。
//! 权限验证调用只读账户接口；系统 keyring 的读写由上层连接流程处理。

use chrono::Utc;

use crate::binance::BinanceClient;
use crate::domain::{AppError, AppResult, StrategyDirection};

pub(super) async fn verify_binance_access(client: &BinanceClient) -> AppResult<()> {
    let (option_account, spot_account) =
        tokio::join!(client.verify_account(), client.spot_account());
    option_account?;
    let spot_account = spot_account?;
    if !spot_account.can_trade {
        return Err(AppError::Config(crate::msg!(
            "backend.binanceSpotAccountCannotTradeExpiryRebalancing"
        )));
    }
    Ok(())
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
pub(super) fn make_spot_client_order_id() -> String {
    format!("ds-{}", uuid::Uuid::new_v4().simple())
}
