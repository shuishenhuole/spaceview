//! Tauri commands:前端唯一入口。
//! 后台扫描/删除在独立线程运行,进度与结果通过 `scan://*` / `delete://*` 事件推送。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, State};

use crate::engine::{deleter, dto, scanner};
use crate::state::ScanState;

/// 枚举可扫描的顶层存储位置(Windows 盘符 / POSIX 挂载点)
#[cfg(windows)]
fn platform_drives() -> Vec<String> {
    ('A'..='Z')
        .map(|c| format!("{c}:\\"))
        .filter(|p| std::path::Path::new(p).is_dir())
        .collect()
}

#[cfg(not(windows))]
fn platform_drives() -> Vec<String> {
    let mut v = vec!["/".to_owned()];
    for mount_dir in ["/Volumes", "/media", "/run/media"] {
        if let Ok(rd) = std::fs::read_dir(mount_dir) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    v.push(e.path().to_string_lossy().into_owned());
                }
            }
        }
    }
    v
}

#[tauri::command]
pub fn list_drives() -> Vec<String> {
    platform_drives()
}

/// 开始扫描。扫描期间每秒推送 `scan://snapshot`,结束时推送 `scan://done`。
#[tauri::command]
pub fn start_scan(
    app: AppHandle,
    state: State<'_, ScanState>,
    root: String,
) -> Result<(), String> {
    let inner = Arc::clone(&state.inner);
    if inner.deleting.load(Ordering::SeqCst) {
        return Err("删除正在后台进行,请等待完成或取消后再扫描".into());
    }
    if inner.scanning.load(Ordering::SeqCst) {
        return Err("扫描正在进行中".into());
    }
    // 让上一轮残留的线程(若有)尽快退出
    inner.cancel.lock().unwrap().store(true, Ordering::Relaxed);
    inner.epoch.fetch_add(1, Ordering::SeqCst);
    let epoch = inner.epoch.load(Ordering::SeqCst);

    let cancel = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(false));
    *inner.cancel.lock().unwrap() = cancel.clone();
    *inner.paused.lock().unwrap() = paused.clone();
    *inner.result.lock().unwrap() = None;
    inner.scanning.store(true, Ordering::SeqCst);

    let root_path = PathBuf::from(&root);

    // 事件转发线程:扫描线程只把事件投递进通道,serde 序列化与 WebView emit
    // 全部在这里执行。整棵树的快照 JSON 可达数十 MB,若留在遍历线程里会
    // 明显拖慢扫描;前端侧另有 500ms 节流,两边都不做高频 IPC。
    let (tx, rx) = std::sync::mpsc::channel::<scanner::ScanEvent>();
    let emit_app = app.clone();
    let emit_inner = Arc::clone(&inner);
    std::thread::spawn(move || {
        while let Ok(ev) = rx.recv() {
            match ev {
                scanner::ScanEvent::Tick {
                    dirs,
                    files,
                    bytes,
                    cur,
                } => {
                    let _ = emit_app.emit(
                        "scan://tick",
                        dto::ScanTickDto {
                            dirs,
                            files,
                            bytes,
                            cur,
                        },
                    );
                }
                scanner::ScanEvent::Snapshot(snapshot) => {
                    let _ = emit_app.emit("scan://snapshot", dto::to_dto(&snapshot));
                }
                scanner::ScanEvent::Done(result) => {
                    *emit_inner.result.lock().unwrap() = Some((*result).clone());
                    // 先落状态再通知前端,避免前端收到 done 后立刻删除被误判"扫描中"
                    if epoch == emit_inner.epoch.load(Ordering::SeqCst) {
                        emit_inner.scanning.store(false, Ordering::SeqCst);
                    }
                    let _ = emit_app.emit("scan://done", dto::to_dto(&result));
                    break;
                }
                scanner::ScanEvent::Cancelled => {
                    let _ = emit_app.emit("scan://cancelled", ());
                    break;
                }
            }
        }
        // 通道关闭(扫描失败/取消)时的兜底清理,只清理本轮标志
        if epoch == emit_inner.epoch.load(Ordering::SeqCst) {
            emit_inner.scanning.store(false, Ordering::SeqCst);
        }
    });

    // 扫描线程:只做遍历与数据构建,事件投递是一次近乎零成本的 channel send
    std::thread::spawn(move || {
        let res = scanner::run_scan(root_path, cancel, paused, |ev| {
            let _ = tx.send(ev);
        });
        if let Err(e) = res {
            let _ = app.emit("scan://failed", e);
        }
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, ScanState>) {
    state
        .inner
        .cancel
        .lock()
        .unwrap()
        .store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn set_paused(state: State<'_, ScanState>, paused: bool) {
    state
        .inner
        .paused
        .lock()
        .unwrap()
        .store(paused, Ordering::Relaxed);
}

/// 勾选路径批量移到回收站。结束后推送 `scan://updated`(已同步的新结果)
/// 与 `delete://done`。
#[tauri::command]
pub fn delete_paths(
    app: AppHandle,
    state: State<'_, ScanState>,
    paths: Vec<String>,
) -> Result<(), String> {
    let inner = Arc::clone(&state.inner);
    if inner.scanning.load(Ordering::SeqCst) {
        return Err("扫描进行中,请等待完成后再删除".into());
    }
    if inner.deleting.load(Ordering::SeqCst) {
        return Err("删除正在后台进行".into());
    }
    if inner.result.lock().unwrap().is_none() {
        return Err("当前没有可同步的扫描结果".into());
    }
    if paths.is_empty() {
        return Ok(());
    }

    inner.deleting.store(true, Ordering::SeqCst);
    let cancel = Arc::new(AtomicBool::new(false));
    *inner.delete_cancel.lock().unwrap() = cancel.clone();
    let epoch = inner.epoch.load(Ordering::SeqCst);
    let total = paths.len();

    std::thread::spawn(move || {
        let mut done = 0usize;
        let mut ok = 0usize;
        let summary = deleter::run_delete(
            paths.into_iter().map(PathBuf::from).collect(),
            cancel,
            |ev| match ev {
                deleter::DeleteEvent::Started(p) => {
                    let _ = app.emit(
                        "delete://started",
                        dto::DeleteStartedDto {
                            path: p.to_string_lossy().into_owned(),
                            done,
                            total,
                        },
                    );
                }
                deleter::DeleteEvent::Item(outcome) => {
                    done += 1;
                    let failed = outcome.error.is_some();
                    if !failed {
                        ok += 1;
                    }
                    let _ = app.emit(
                        "delete://item",
                        dto::DeleteItemDto {
                            path: outcome.path.to_string_lossy().into_owned(),
                            error: outcome.error,
                            ok,
                            done,
                            total,
                        },
                    );
                }
            },
        );

        // 只有扫描结果仍是发起删除时的那一轮,才把变化同步回模型
        if epoch == inner.epoch.load(Ordering::SeqCst) {
            let mut guard = inner.result.lock().unwrap();
            if let Some(result) = guard.as_mut() {
                result.apply_deleted(&summary.succeeded);
                let _ = app.emit("scan://updated", dto::to_dto(result));
            }
        }

        let _ = app.emit(
            "delete://done",
            dto::DeleteDoneDto {
                ok: summary.ok,
                failed: summary.errors.len(),
                cancelled: summary.cancelled,
                total: summary.total,
            },
        );
        inner.deleting.store(false, Ordering::SeqCst);
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_delete(state: State<'_, ScanState>) {
    state
        .inner
        .delete_cancel
        .lock()
        .unwrap()
        .store(true, Ordering::Relaxed);
}

/// 同步 Windows 原生标题栏深/浅色。
///
/// Tauri(wry) 的 set_theme 只设置 WebView2 的 preferred color scheme 与 tao
/// 窗口主题,不会下发 DWMWA_USE_IMMERSIVE_DARK_MODE,标题栏仍保持浅色。
/// 因此这里在切换主题时显式调用一次 DWM 接口(与旧版 egui 的处理一致)。
#[tauri::command]
pub fn set_titlebar_theme(window: tauri::WebviewWindow, dark: bool) {
    #[cfg(windows)]
    {
        // WebView2 的 prefers-color-scheme 也一并同步
        let _ = window.set_theme(Some(if dark {
            tauri::Theme::Dark
        } else {
            tauri::Theme::Light
        }));
        if let Ok(hwnd) = window.hwnd() {
            set_dwm_dark(hwnd.0 as usize as isize, dark);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (window, dark);
    }
}

#[cfg(windows)]
fn set_dwm_dark(hwnd: isize, dark: bool) {
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: isize,
            attribute: u32,
            value: *const u32,
            size: u32,
        ) -> i32;
    }
    // 20 = Win11 22H2+,19 = Win10 1903+;两个都下发以兼容不同系统
    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const DWMWA_USE_IMMERSIVE_DARK_MODE_OLD: u32 = 19;
    let value: u32 = if dark { 1 } else { 0 };
    unsafe {
        for attr in [
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            DWMWA_USE_IMMERSIVE_DARK_MODE_OLD,
        ] {
            let _ = DwmSetWindowAttribute(
                hwnd,
                attr,
                &value,
                std::mem::size_of::<u32>() as u32,
            );
        }
    }
}
