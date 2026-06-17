//! 桌面二进制入口。
//!
//! 实际 Tauri 启动逻辑在库 crate 中，便于桌面入口和未来移动入口共享同一套 wiring。

// Windows 发布版直接显示桌面窗口；开发版保留控制台诊断输出。
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

fn main() {
    dual_investment_lib::run();
}
