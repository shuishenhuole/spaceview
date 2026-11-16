//! SpaceView(Tauri 2)Rust 侧入口。
//! `engine/` 是与 UI 无关的扫描/树/删除引擎,`commands.rs` 负责把它桥接到前端事件流。

pub mod commands;
pub mod engine;
pub mod state;

use tauri::Manager;
use state::ScanState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(ScanState::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_drives,
            commands::start_scan,
            commands::cancel_scan,
            commands::set_paused,
            commands::delete_paths,
            commands::cancel_delete,
            commands::junk_roots,
            commands::scan_junk,
            commands::cancel_junk,
            commands::set_titlebar_theme,
        ])
        .setup(|app| {
            // 窗口创建后立即同步原生标题栏(DWM 沉浸深色),不依赖前端 IPC 时机。
            // 默认深色;若用户持久化了浅色,前端的 watch 会立刻改成浅色。
            #[cfg(windows)]
            if let Some(window) = app.get_webview_window("main") {
                commands::set_titlebar_theme(window, true);
            }
            #[cfg(not(windows))]
            let _ = &app;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
