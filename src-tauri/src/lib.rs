mod app_state;
mod binance;
mod commands;
mod domain;
mod persistence;
mod strategy;
mod tray;

use app_state::AppState;
use commands::{
    connect_binance, export_audit_logs, get_app_snapshot, get_pnl_report, get_positions,
    get_risk_snapshot, list_orders, place_manual_strategy, preview_strategy, save_auto_rule,
    scan_candidates, toggle_auto_strategy,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState::new().expect("failed to initialize application state");
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_app_snapshot,
            connect_binance,
            scan_candidates,
            preview_strategy,
            place_manual_strategy,
            save_auto_rule,
            toggle_auto_strategy,
            list_orders,
            get_positions,
            get_pnl_report,
            get_risk_snapshot,
            export_audit_logs
        ])
        .setup(|app| {
            tray::build(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
