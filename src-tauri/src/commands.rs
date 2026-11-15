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
    std::thread::spawn(move || {
        let res = scanner::run_scan(root_path, cancel, paused, |ev| match ev {
            scanner::ScanEvent::Tick {
                dirs,
                files,
                bytes,
                cur,
            } => {
                let _ = app.emit(
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
                let _ = app.emit("scan://snapshot", dto::to_dto(&snapshot));
            }
            scanner::ScanEvent::Done(result) => {
                *inner.result.lock().unwrap() = Some((*result).clone());
                let _ = app.emit("scan://done", dto::to_dto(&result));
            }
            scanner::ScanEvent::Cancelled => {
                let _ = app.emit("scan://cancelled", ());
            }
        });
        if let Err(e) = res {
            let _ = app.emit("scan://failed", e);
        }
        // 只清理本轮标志,避免迟到的旧线程误清新一轮状态
        if inner.epoch.load(Ordering::SeqCst) == epoch {
            inner.scanning.store(false, Ordering::SeqCst);
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
