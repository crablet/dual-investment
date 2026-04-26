//! Tauri IPC command 边界。
//!
//! command 层只做参数转发和错误转换，真实业务在 `AppState` 中完成。所有 command
//! 按 Tauri 约定返回 `Result<T, String>`，让前端能直接展示可读错误。
#![allow(clippy::needless_pass_by_value)]

use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::app_state::AppState;
use crate::domain::{
    AdoptOptionPositionsRequest, AdoptableOptionPositionView, AppSettings, AppSnapshot,
    AutoInvestmentRule, CandidateScanResult, ConnectBinanceRequest, LogSnapshot, StrategyOrder,
    StrategyRequest, ToggleAutoRequest,
};

/// 获取应用完整快照。
#[tauri::command]
pub async fn get_app_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    state.snapshot().await.map_err(to_command_error)
}

/// 使用用户输入凭据连接 Binance。
#[tauri::command]
pub async fn connect_binance(
    state: State<'_, AppState>,
    request: ConnectBinanceRequest,
) -> Result<AppSettings, String> {
    state
        .connect_binance(request)
        .await
        .map_err(to_command_error)
}

/// 使用系统 keyring 中保存的凭据连接 Binance。
#[tauri::command]
pub async fn connect_saved_binance(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state
        .connect_saved_binance()
        .await
        .map_err(to_command_error)
}

/// 删除系统 keyring 中保存的 Binance 凭据。
#[tauri::command]
pub async fn forget_binance_credentials(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state
        .forget_binance_credentials()
        .await
        .map_err(to_command_error)
}

/// 扫描实盘候选机会。
#[tauri::command]
pub async fn scan_candidates(state: State<'_, AppState>) -> Result<CandidateScanResult, String> {
    state.scan_candidates().await.map_err(to_command_error)
}

/// 发送手动真实策略订单。
#[tauri::command]
pub async fn place_manual_strategy(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StrategyRequest,
) -> Result<StrategyOrder, String> {
    state
        .place_manual_strategy(request, app)
        .await
        .map_err(to_command_error)
}

/// 保存自动复投规则。
#[tauri::command]
pub async fn save_auto_rule(
    state: State<'_, AppState>,
    rule: AutoInvestmentRule,
) -> Result<AutoInvestmentRule, String> {
    state.save_auto_rule(rule).await.map_err(to_command_error)
}

/// 开启或暂停自动复投。
#[tauri::command]
pub async fn toggle_auto_strategy(
    state: State<'_, AppState>,
    request: ToggleAutoRequest,
) -> Result<AppSettings, String> {
    state
        .toggle_auto_strategy(request)
        .await
        .map_err(to_command_error)
}

/// 按恢复记录重新查询 Binance 并恢复订单。
#[tauri::command]
pub async fn recover_order_attempt(
    state: State<'_, AppState>,
    attempt_id: String,
) -> Result<StrategyOrder, String> {
    state
        .recover_order_attempt(attempt_id)
        .await
        .map_err(to_command_error)
}

/// 列出可由本机接管监控的 Binance 期权持仓。
#[tauri::command]
pub async fn list_adoptable_option_positions(
    state: State<'_, AppState>,
) -> Result<Vec<AdoptableOptionPositionView>, String> {
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
) -> Result<Vec<StrategyOrder>, String> {
    state
        .adopt_option_positions(request)
        .await
        .map_err(to_command_error)
}

/// 同步实盘状态并返回快照。
#[tauri::command]
pub async fn sync_live_state(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    state.sync_live_state().await.map_err(to_command_error)
}

/// 手动触发到期结算扫描。
#[tauri::command]
pub async fn run_settlement_scan(state: State<'_, AppState>) -> Result<Vec<StrategyOrder>, String> {
    state.run_settlement_scan().await.map_err(to_command_error)
}

/// 手动执行一次自动复投流程。
#[tauri::command]
pub async fn run_auto_once(state: State<'_, AppState>) -> Result<Option<StrategyOrder>, String> {
    state.run_auto_once().await.map_err(to_command_error)
}

/// 读取文件日志快照。
#[tauri::command]
pub fn get_log_snapshot(state: State<'_, AppState>) -> Result<LogSnapshot, String> {
    state.log_snapshot().map_err(to_command_error)
}

/// 用系统默认应用打开今日日志文件。
#[tauri::command]
pub fn open_current_log_file(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let path = state
        .ensure_current_log_file()
        .map_err(to_command_error)?
        .display()
        .to_string();
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|error| format!("无法打开日志文件：{error}"))
}

/// 用系统文件管理器打开日志目录。
#[tauri::command]
pub fn open_log_folder(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let path = state
        .ensure_log_directory()
        .map_err(to_command_error)?
        .display()
        .to_string();
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|error| format!("无法打开日志目录：{error}"))
}

/// 删除全部本地日志文件。
#[tauri::command]
pub fn clear_log_files(state: State<'_, AppState>) -> Result<LogSnapshot, String> {
    state.clear_log_files().map_err(to_command_error)
}

/// 把内部错误转换成 Tauri command 的字符串错误。
#[allow(clippy::needless_pass_by_value)]
fn to_command_error(error: crate::domain::AppError) -> String {
    error.to_string()
}
