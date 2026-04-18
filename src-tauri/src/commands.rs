use tauri::State;

use crate::app_state::AppState;
use crate::domain::{
    AppSettings, AppSnapshot, AutoInvestmentRule, BalanceRow, ConnectBinanceRequest, PnlReport,
    RiskSnapshot, StrategyCandidate, StrategyOrder, StrategyPreview, StrategyRequest,
    ToggleAutoRequest,
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
pub async fn scan_candidates(state: State<'_, AppState>) -> Result<Vec<StrategyCandidate>, String> {
    state.scan_candidates().await.map_err(to_command_error)
}

#[tauri::command]
pub async fn preview_strategy(
    state: State<'_, AppState>,
    request: StrategyRequest,
) -> Result<StrategyPreview, String> {
    state
        .preview_strategy(request)
        .await
        .map_err(to_command_error)
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
pub fn get_positions(state: State<'_, AppState>) -> Vec<BalanceRow> {
    state.balances()
}

#[tauri::command]
pub fn get_pnl_report(state: State<'_, AppState>) -> PnlReport {
    state.pnl_report()
}

#[tauri::command]
pub fn get_risk_snapshot(state: State<'_, AppState>) -> RiskSnapshot {
    state.risk_snapshot()
}

#[tauri::command]
pub fn export_audit_logs(state: State<'_, AppState>) -> Result<String, String> {
    state.export_audit_logs().map_err(to_command_error)
}

fn to_command_error(error: crate::domain::AppError) -> String {
    error.to_string()
}
