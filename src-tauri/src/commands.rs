use tauri::State;

use crate::app_state::AppState;
use crate::domain::{
    AppSettings, AppSnapshot, AutoInvestmentRule, BalanceRow, ConnectBinanceRequest,
    OrderAttemptView, PnlReport, RiskSnapshot, StrategyCandidate, StrategyOrder, StrategyPreview,
    StrategyRequest, ToggleAutoRequest,
};

#[tauri::command]
pub async fn get_app_snapshot(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    state.snapshot().await.map_err(to_command_error)
}

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

#[tauri::command]
pub async fn connect_saved_binance(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state
        .connect_saved_binance()
        .await
        .map_err(to_command_error)
}

#[tauri::command]
pub async fn forget_binance_credentials(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state
        .forget_binance_credentials()
        .await
        .map_err(to_command_error)
}

#[tauri::command]
pub async fn scan_candidates(state: State<'_, AppState>) -> Result<Vec<StrategyCandidate>, String> {
    state.scan_candidates().await.map_err(to_command_error)
}

#[tauri::command]
pub async fn preview_strategy(
    _state: State<'_, AppState>,
    request: StrategyRequest,
) -> Result<StrategyPreview, String> {
    AppState::preview_strategy(&request).map_err(to_command_error)
}

#[tauri::command]
pub async fn place_manual_strategy(
    state: State<'_, AppState>,
    request: StrategyRequest,
) -> Result<StrategyOrder, String> {
    state
        .place_manual_strategy(request)
        .await
        .map_err(to_command_error)
}

#[tauri::command]
pub async fn save_auto_rule(
    state: State<'_, AppState>,
    rule: AutoInvestmentRule,
) -> Result<AutoInvestmentRule, String> {
    state.save_auto_rule(rule).await.map_err(to_command_error)
}

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

#[tauri::command]
pub async fn list_orders(state: State<'_, AppState>) -> Result<Vec<StrategyOrder>, String> {
    state.list_orders().await.map_err(to_command_error)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn list_order_attempts(state: State<'_, AppState>) -> Result<Vec<OrderAttemptView>, String> {
    state.list_order_attempts().map_err(to_command_error)
}

#[tauri::command]
pub async fn sync_live_state(state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    state.sync_live_state().await.map_err(to_command_error)
}

#[tauri::command]
pub async fn run_settlement_scan(state: State<'_, AppState>) -> Result<Vec<StrategyOrder>, String> {
    state.run_settlement_scan().await.map_err(to_command_error)
}

#[tauri::command]
pub async fn run_auto_once(state: State<'_, AppState>) -> Result<Option<StrategyOrder>, String> {
    state.run_auto_once().await.map_err(to_command_error)
}

#[tauri::command]
pub async fn get_positions(state: State<'_, AppState>) -> Result<Vec<BalanceRow>, String> {
    state.balances().await.map_err(to_command_error)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn get_pnl_report(state: State<'_, AppState>) -> PnlReport {
    state.pnl_report()
}

#[tauri::command]
pub async fn get_risk_snapshot(state: State<'_, AppState>) -> Result<RiskSnapshot, String> {
    state.risk_snapshot().await.map_err(to_command_error)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn export_audit_logs(state: State<'_, AppState>) -> Result<String, String> {
    state.export_audit_logs().map_err(to_command_error)
}

#[allow(clippy::needless_pass_by_value)]
fn to_command_error(error: crate::domain::AppError) -> String {
    error.to_string()
}
