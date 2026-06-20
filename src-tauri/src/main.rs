//! 桌面程序入口，启动逻辑在 lib.rs。

// Windows 发布版直接显示桌面窗口；开发版保留控制台诊断输出。
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

fn main() {
    dual_investment_lib::run();
}
