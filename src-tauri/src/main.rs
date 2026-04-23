//! 桌面二进制入口。
//!
//! 实际 Tauri 启动逻辑在库 crate 中，便于移动端入口和测试共享同一套 wiring。

fn main() {
    dual_investment_lib::run();
}
