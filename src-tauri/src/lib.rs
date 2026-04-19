mod app_state;
mod binance;
mod commands;
mod domain;
mod persistence;
mod secrets;
mod strategy;
mod tray;

use app_state::AppState;
use commands::{
    connect_binance, connect_saved_binance, export_audit_logs, forget_binance_credentials,
    get_app_snapshot, get_pnl_report, get_positions, get_risk_snapshot, list_order_attempts,
    list_orders, place_manual_strategy, preview_strategy, recover_order_attempt, run_auto_once,
    run_settlement_scan, save_auto_rule, scan_candidates, sync_live_state, toggle_auto_strategy,
};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Starts the Tauri application.
///
/// # Panics
///
/// Panics if the local application state cannot be initialized or if the Tauri
/// runtime exits with an unrecoverable startup error.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            get_app_snapshot,
            connect_binance,
            connect_saved_binance,
            forget_binance_credentials,
            scan_candidates,
            preview_strategy,
            place_manual_strategy,
            save_auto_rule,
            toggle_auto_strategy,
            list_orders,
            list_order_attempts,
            recover_order_attempt,
            sync_live_state,
            run_settlement_scan,
            run_auto_once,
            get_positions,
            get_pnl_report,
            get_risk_snapshot,
            export_audit_logs
        ])
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let app_state =
                AppState::new_in(data_dir).expect("failed to initialize application state");
            app.manage(app_state);
            tray::build(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
