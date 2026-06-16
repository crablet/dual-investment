//! Tauri IPC command 边界。
//!
//! 交易与状态操作转发给 `AppState`，纯计算预览调用策略层。业务错误统一返回
//! 脱敏摘要与连接重试资格；前端不从提示文案推断控制行为。
#![allow(clippy::needless_pass_by_value)]

use crate::messages::Message;

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::app_state::AppState;
use crate::domain::{
    AdoptOptionPositionsRequest, AdoptableOptionPositionView, AppSettings, AppSnapshot,
    ConnectBinanceRequest, FinishSettlementSweepRequest, FundingAttemptView, LogSnapshot,
    ResolveFundingAttemptRequest, SettlementRecoveryView, StrategyCandidate, StrategyOrder,
    StrategyPreview, StrategyPreviewRequest, StrategyRequest,
};

/// 获取应用完整快照。
#[tauri::command]
pub async fn get_app_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, CommandError> {
    state.snapshot().await.map_err(to_command_error)
}

/// 使用用户输入凭据连接 Binance。
#[tauri::command]
pub async fn connect_binance(
    state: State<'_, AppState>,
    request: ConnectBinanceRequest,
) -> Result<AppSettings, CommandError> {
    state
        .connect_binance(request)
        .await
        .map_err(to_command_error)
}

/// 使用系统 keyring 中保存的凭据连接 Binance。
#[tauri::command]
pub async fn connect_saved_binance(
    state: State<'_, AppState>,
) -> Result<AppSettings, CommandError> {
    state
        .connect_saved_binance()
        .await
        .map_err(to_command_error)
}

/// 删除系统 keyring 中保存的 Binance 凭据。
#[tauri::command]
pub async fn forget_binance_credentials(
    state: State<'_, AppState>,
) -> Result<AppSettings, CommandError> {
    state
        .forget_binance_credentials()
        .await
        .map_err(to_command_error)
}

/// 扫描实盘候选机会。
#[tauri::command]
pub async fn scan_candidates(
    state: State<'_, AppState>,
) -> Result<Vec<StrategyCandidate>, CommandError> {
    state.scan_candidates().await.map_err(to_command_error)
}

/// 发送手动真实策略订单。
#[tauri::command]
pub async fn place_manual_strategy(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StrategyRequest,
) -> Result<StrategyOrder, CommandError> {
    state
        .place_manual_strategy(request, app)
        .await
        .map_err(to_command_error)
}

/// 按恢复记录重新查询 Binance 并恢复订单。
#[tauri::command]
pub async fn recover_order_attempt(
    state: State<'_, AppState>,
    attempt_id: String,
) -> Result<StrategyOrder, CommandError> {
    state
        .recover_order_attempt(attempt_id)
        .await
        .map_err(to_command_error)
}

/// 按资金恢复记录重新查询 Binance 历史并推进状态。
#[tauri::command]
pub async fn recover_funding_attempt(
    state: State<'_, AppState>,
    attempt_id: String,
) -> Result<FundingAttemptView, CommandError> {
    state
        .recover_funding_attempt(attempt_id)
        .await
        .map_err(to_command_error)
}

/// 人工核对 Binance 事实后解除已阻断的资金恢复记录。
#[tauri::command]
pub async fn resolve_funding_attempt(
    state: State<'_, AppState>,
    request: ResolveFundingAttemptRequest,
) -> Result<FundingAttemptView, CommandError> {
    state
        .resolve_funding_attempt(&request)
        .await
        .map_err(to_command_error)
}

/// 用户确认结束失败的理财收尾，保留已有资金结果。
#[tauri::command]
pub async fn finish_settlement_sweep(
    state: State<'_, AppState>,
    request: FinishSettlementSweepRequest,
) -> Result<SettlementRecoveryView, CommandError> {
    state
        .finish_settlement_sweep(&request)
        .await
        .map_err(to_command_error)
}

/// 列出可由本机接管监控的 Binance 期权持仓。
#[tauri::command]
pub async fn list_adoptable_option_positions(
    state: State<'_, AppState>,
) -> Result<Vec<AdoptableOptionPositionView>, CommandError> {
    state
        .list_adoptable_option_positions()
        .await
        .map_err(to_command_error)
}

/// 接管选中的 Binance 期权持仓。
#[tauri::command]
pub async fn adopt_option_positions(
    state: State<'_, AppState>,
    request: AdoptOptionPositionsRequest,
) -> Result<Vec<StrategyOrder>, CommandError> {
    state
        .adopt_option_positions(request)
        .await
        .map_err(to_command_error)
}

/// 显式同步实盘事实；前端随后读取本地快照，避免多入口返回旧快照互相覆盖。
#[tauri::command]
pub async fn sync_live_state(state: State<'_, AppState>) -> Result<(), CommandError> {
    state.sync_live_state().await.map_err(to_command_error)
}

/// 手动触发到期结算扫描。
#[tauri::command]
pub async fn run_settlement_scan(
    state: State<'_, AppState>,
) -> Result<Vec<StrategyOrder>, CommandError> {
    state.run_settlement_scan().await.map_err(to_command_error)
}

/// 读取文件日志快照。
#[tauri::command]
pub fn get_log_snapshot(state: State<'_, AppState>) -> Result<LogSnapshot, CommandError> {
    state.log_snapshot().map_err(to_command_error)
}

/// 用系统默认应用打开今日日志文件。
#[tauri::command]
pub fn open_current_log_file(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), CommandError> {
    let path = state
        .ensure_current_log_file()
        .map_err(to_command_error)?
        .display()
        .to_string();
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|error| {
            to_command_error(crate::domain::AppError::Storage(crate::msg!(
                "backend.cannotOpenLogFile",
                p0 = error.to_string()
            )))
        })
}

/// 用系统文件管理器打开日志目录。
#[tauri::command]
pub fn open_log_folder(app: AppHandle, state: State<'_, AppState>) -> Result<(), CommandError> {
    let path = state
        .ensure_log_directory()
        .map_err(to_command_error)?
        .display()
        .to_string();
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|error| {
            to_command_error(crate::domain::AppError::Storage(crate::msg!(
                "backend.cannotOpenLogFolder",
                p0 = error.to_string()
            )))
        })
}

/// 删除全部本地日志文件。
#[tauri::command]
pub fn clear_log_files(state: State<'_, AppState>) -> Result<LogSnapshot, CommandError> {
    state.clear_log_files().map_err(to_command_error)
}

/// 纯本地预估，既不读取账户也不发送交易所请求。
#[tauri::command]
pub fn preview_strategy(request: StrategyPreviewRequest) -> Result<StrategyPreview, CommandError> {
    crate::strategy::preview_strategy(&request).map_err(to_command_error)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    message: Message,
    can_auto_reconnect: bool,
}

fn to_command_error(error: crate::domain::AppError) -> CommandError {
    CommandError {
        message: error.message(),
        can_auto_reconnect: error.can_auto_reconnect(),
    }
}
