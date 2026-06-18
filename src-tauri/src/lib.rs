//! Tauri 应用入口库。
//!
//! 这里组装插件、IPC command 和应用状态。具体业务逻辑分散在状态、
//! Binance、策略和持久化模块中，入口只负责 wiring。

mod app_state;
mod binance;
mod commands;
mod domain;
mod errors;
mod logs;
mod messages;
mod persistence;
mod secrets;
mod strategy;

use app_state::AppState;
use commands::{
    clear_log_files, connect_binance, connect_saved_binance, finish_settlement_sweep,
    forget_binance_credentials, get_app_snapshot, get_log_snapshot, open_current_log_file,
    open_log_folder, place_manual_strategy, preview_strategy, recover_funding_attempt,
    recover_order_attempt, resolve_funding_attempt, scan_candidates, sync_live_state,
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
        .invoke_handler(tauri::generate_handler![
            get_app_snapshot,
            connect_binance,
            connect_saved_binance,
            forget_binance_credentials,
            scan_candidates,
            preview_strategy,
            place_manual_strategy,
            recover_order_attempt,
            recover_funding_attempt,
            resolve_funding_attempt,
            finish_settlement_sweep,
            sync_live_state,
            get_log_snapshot,
            open_current_log_file,
            open_log_folder,
            clear_log_files
        ])
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let app_state =
                AppState::new_in(data_dir).expect("failed to initialize application state");
            app_state.ensure_background_worker();
            app.manage(app_state);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
