//! 通过只读接口验证 Binance 权限，并生成客户端订单号。

use chrono::Utc;

use crate::binance::BinanceClient;
use crate::domain::{AppError, AppResult, StrategyDirection};

pub(super) async fn verify_binance_access(client: &BinanceClient) -> AppResult<String> {
    let (option_account, spot_account) =
        tokio::join!(client.verify_account(), client.spot_account());
    option_account?;
    let spot_account = spot_account?;
    if !spot_account.can_trade {
        return Err(AppError::Config(crate::msg!(
            "backend.binanceSpotAccountCannotTradeExpiryRebalancing"
        )));
    }
    Ok(spot_account.uid)
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
