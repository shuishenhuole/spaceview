//! 构建脚本:调用 Tauri 的构建逻辑,并把图标文件加入「改动即重跑」监听。
//!
//! 背景:Windows 的 exe 图标(任务栏、跳转列表、文件资源管理器里显示的那个)
//! 是编译期由 tauri-build 把 `icons/icon.ico` 写进 exe 资源段的。
//! 如果这里不声明 rerun-if-changed,换掉 `icons/` 下的图标后 Cargo 认为
//! build.rs 没有变化 → 不重跑 → exe 里仍是旧图标,只有前端界面换了。
//! 因此下面显式列出图标文件,保证重新生成图标后 exe 图标会同步更新。

fn main() {
    // 图标变更 → 重跑构建脚本 → 重新嵌入 exe 资源段
    for icon in [
        "icons/icon.ico",
        "icons/icon.png",
        "icons/32x32.png",
        "icons/128x128.png",
        "icons/128x128@2x.png",
        "icons/icon.icns",
    ] {
        println!("cargo:rerun-if-changed={icon}");
    }

    tauri_build::build()
}
