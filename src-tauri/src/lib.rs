//! SpaceView(Tauri 2)Rust 侧入口。
//! `engine/` 是与 UI 无关的扫描/树/删除引擎,`commands.rs` 负责把它桥接到前端事件流。

pub mod commands;
pub mod engine;
pub mod state;

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
