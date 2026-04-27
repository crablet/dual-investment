//! Tauri 应用入口库。
//!
//! 这里组装插件、IPC command、应用状态和系统托盘。具体业务逻辑分散在状态、
//! Binance、策略和持久化模块中，入口只负责 wiring。

mod app_state;
mod binance;
mod commands;
mod decimal_utils;
mod domain;
mod logs;
mod persistence;
mod secrets;
mod strategy;
mod tray;

use app_state::AppState;
use commands::{
    adopt_option_positions, clear_log_files, connect_binance, connect_saved_binance,
    forget_binance_credentials, get_app_snapshot, get_log_snapshot,
    list_adoptable_option_positions, open_current_log_file, open_log_folder, place_manual_strategy,
    recover_funding_attempt, recover_order_attempt, resolve_funding_attempt, run_auto_once,
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
            place_manual_strategy,
            save_auto_rule,
            toggle_auto_strategy,
            recover_order_attempt,
            recover_funding_attempt,
            resolve_funding_attempt,
            list_adoptable_option_positions,
            adopt_option_positions,
            sync_live_state,
            run_settlement_scan,
            run_auto_once,
            get_log_snapshot,
            open_current_log_file,
            open_log_folder,
            clear_log_files
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
