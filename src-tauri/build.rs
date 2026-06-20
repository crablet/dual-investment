// 图标变化时重新构建 Tauri 应用。
fn main() {
    println!("cargo:rerun-if-changed=icons/icon.png");
    tauri_build::build();
}
