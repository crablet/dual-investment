// Tauri 构建脚本。
//
// 图标会影响桌面启动和打包产物，因此显式声明变更触发重新构建。
fn main() {
    println!("cargo:rerun-if-changed=icons/icon.png");
    tauri_build::build();
}
