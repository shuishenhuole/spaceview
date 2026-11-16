//! Tauri commands:前端唯一入口。
//! 后台扫描/删除在独立线程运行,进度与结果通过 `scan://*` / `delete://*` 事件推送。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::process::Command;

use tauri::{AppHandle, Emitter, State};

use crate::engine::{deleter, dto, junk, scanner, uninstall};
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
        // 典型场景:前端在扫描中切换路由,已主动取消但旧线程退出是协作式的,
        // 需要几毫秒感知。这里先补一发取消信号并短暂等待,等不到才报错,
        // 让"切走再切回来立刻重新扫描"无缝衔接,而不是弹"扫描正在进行中"。
        inner.cancel.lock().unwrap().store(true, Ordering::Relaxed);
        let deadline = Instant::now() + Duration::from_secs(2);
        while inner.scanning.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(25));
        }
        if inner.scanning.load(Ordering::SeqCst) {
            return Err("扫描正在进行中".into());
        }
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
    inner.pending_deletes.lock().unwrap().clear();
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
                    let mut result = *result;
                    // 扫描期间被删掉的文件:从终值里扣掉,避免已清理项残留在树里
                    // (先取出再释放锁,避免与其他线程形成 result → pending 的反向加锁顺序)
                    let pending = {
                        let mut guard = emit_inner.pending_deletes.lock().unwrap();
                        std::mem::take(&mut *guard)
                    };
                    result.apply_deleted(&pending);
                    *emit_inner.result.lock().unwrap() = Some(result.clone());
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
    if inner.deleting.load(Ordering::SeqCst) {
        return Err("删除正在后台进行".into());
    }
    if paths.is_empty() {
        return Ok(());
    }

    inner.deleting.store(true, Ordering::SeqCst);
    let cancel = Arc::new(AtomicBool::new(false));
    *inner.delete_cancel.lock().unwrap() = cancel.clone();
    let epoch = inner.epoch.load(Ordering::SeqCst);
    let total = paths.len();
    inner.delete_done.store(0, Ordering::SeqCst);
    inner.delete_total.store(total as u64, Ordering::SeqCst);

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
                    inner.delete_done.store(done as u64, Ordering::SeqCst);
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
            match guard.as_mut() {
                // 扫描已结束:直接在结果里扣减并推给前端
                Some(result) => {
                    result.apply_deleted(&summary.succeeded);
                    let _ = app.emit("scan://updated", dto::to_dto(result));
                }
                // 扫描仍在进行(还没有终值):先记下来,Done 时统一扣减
                None => {
                    inner
                        .pending_deletes
                        .lock()
                        .unwrap()
                        .extend(summary.succeeded.iter().cloned());
                }
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
        inner.delete_total.store(0, Ordering::SeqCst);
        inner.delete_done.store(0, Ordering::SeqCst);
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

/// 枚举可扫描的残留/垃圾位置(仅返回存在的目录)
#[tauri::command]
pub fn junk_roots() -> Vec<dto::JunkRootDto> {
    junk::junk_roots()
        .into_iter()
        .map(|(label, path)| dto::JunkRootDto {
            label: label.into(),
            path: path.to_string_lossy().into_owned(),
        })
        .collect()
}

/// 开始扫描残留文件(临时 / 缓存 / 残留 / 大文件)。结束时推送 `junk://done`。
#[tauri::command]
pub fn scan_junk(
    app: AppHandle,
    state: State<'_, ScanState>,
    roots: Vec<String>,
) -> Result<(), String> {
    let inner = Arc::clone(&state.inner);
    if inner.junk_scanning.swap(true, Ordering::SeqCst) {
        // 典型场景:前端切换路由时已请求取消,但旧线程退出是协作式的,
        // 这里短暂等待旧线程退出,让"切走再回来立刻重扫"无缝衔接。
        inner.junk_cancel.lock().unwrap().store(true, Ordering::Relaxed);
        let deadline = Instant::now() + Duration::from_secs(2);
        while inner.junk_scanning.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(25));
        }
        if inner.junk_scanning.load(Ordering::SeqCst) {
            // 旧线程仍未退出:保持占用标志(旧线程结束时会把它们清掉),直接报错
            return Err("残留扫描正在进行中".into());
        }
        // 旧线程已退出并把标志清为 false,重新置回 true 表示本轮新扫描占用
        inner.junk_scanning.store(true, Ordering::SeqCst);
    }
    let paths: Vec<PathBuf> = roots.iter().map(PathBuf::from).collect();
    if paths.is_empty() {
        inner.junk_scanning.store(false, Ordering::SeqCst);
        return Ok(());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    *inner.junk_cancel.lock().unwrap() = cancel.clone();

    std::thread::spawn(move || {
        let mut last_tick = Instant::now();
        let items = junk::scan_junk(&paths, cancel.clone(), |scanned| {
            if last_tick.elapsed() >= Duration::from_millis(500) {
                last_tick = Instant::now();
                let _ = app.emit("junk://tick", dto::JunkTickDto { scanned });
            }
        });
        let total_bytes: u64 = items.iter().map(|i| i.size).sum();
        let _ = app.emit(
            "junk://done",
            dto::JunkScanDoneDto {
                items: items
                    .into_iter()
                    .map(|i| dto::JunkItemDto {
                        path: i.path.to_string_lossy().into_owned(),
                        size: i.size,
                        is_dir: i.is_dir,
                        kind: i.kind.as_str().into(),
                        reason: i.reason,
                    })
                    .collect(),
                total_bytes,
            },
        );
        inner.junk_scanning.store(false, Ordering::SeqCst);
    });
    Ok(())
}

/// 请求取消尚未开始的残留扫描项
#[tauri::command]
pub fn cancel_junk(state: State<'_, ScanState>) {
    state
        .inner
        .junk_cancel
        .lock()
        .unwrap()
        .store(true, Ordering::Relaxed);
}

/// 扫描 Windows 已安装软件清单(注册表卸载项)。
/// 只读取 DisplayName/Version/Publisher/UninstallString 等信息。
#[tauri::command]
pub fn installed_apps() -> Vec<dto::InstalledAppDto> {
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};
        use winreg::RegKey;
        let locations = [
            (HKEY_CURRENT_USER, "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall", 0),
            (HKEY_LOCAL_MACHINE, "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall", KEY_WOW64_64KEY),
            (HKEY_LOCAL_MACHINE, "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall", KEY_WOW64_32KEY),
        ];
        let mut out = Vec::new();
        for (hive, path, view) in locations {
            let root = RegKey::predef(hive);
            let Ok(uninstall) = root.open_subkey_with_flags(path, KEY_READ | view) else { continue };
            for key_name in uninstall.enum_keys().flatten() {
                let Ok(key) = uninstall.open_subkey_with_flags(&key_name, KEY_READ) else { continue };
                let Ok(name) = key.get_value::<String, _>("DisplayName") else { continue };
                if name.trim().is_empty() { continue; }
                let uninstall_command = key.get_value::<String, _>("UninstallString").unwrap_or_default();
                let install_location = key.get_value::<String, _>("InstallLocation").unwrap_or_default();
                let publisher = key.get_value::<String, _>("Publisher").unwrap_or_default();
                let version = key.get_value::<String, _>("DisplayVersion").unwrap_or_default();
                let estimated_kb = key.get_value::<u32, _>("EstimatedSize").unwrap_or(0);
                out.push(dto::InstalledAppDto {
                    id: format!("{}:{}", name, uninstall_command),
                    name,
                    version,
                    publisher,
                    install_location,
                    uninstall_command,
                    estimated_size: estimated_kb as u64 * 1024,
                });
            }
        }
        out.sort_by_key(|a| a.name.to_lowercase());
        out.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
        out
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// 启动软件注册表中记录的官方卸载程序,不直接删除安装目录。
#[tauri::command]
pub fn launch_uninstaller(uninstall_command: String) -> Result<(), String> {
    let cmd = uninstall_command.trim();
    if cmd.is_empty() {
        return Err("该软件没有注册卸载程序".into());
    }
    #[cfg(windows)]
    {
        // 自行解析命令行后直接 spawn:若交给 `cmd /C start "<原串>"`,
        // Rust 会把内嵌引号转义成 \" ,cmd 不认这种转义,路径会被解析成
        // `\C:\Program Files\...\unins000.exe\`,Windows 报找不到文件。
        let (program, args) = uninstall::split_command(cmd);
        if program.is_empty() {
            return Err("卸载命令为空".into());
        }
        let path = std::path::Path::new(&program);

        // 个别软件把"安装目录"登记为卸载项:直接打开目录让用户自行处理
        if path.is_dir() {
            return Command::new("explorer")
                .arg(&program)
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("打开安装目录失败:{e}"));
        }
        if !path.exists() {
            return Err(format!("卸载程序不存在:{program}"));
        }

        // .bat/.cmd 必须经 cmd 执行:用 raw_arg 传原始命令串,保留注册表里的引号
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let spawned = if ext == "bat" || ext == "cmd" {
            use std::os::windows::process::CommandExt;
            Command::new("cmd").arg("/C").raw_arg(cmd).spawn()
        } else {
            Command::new(&program).args(&args).spawn()
        };
        spawned
            .map(|_| ())
            .map_err(|e| format!("启动卸载程序失败:{e}"))
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
        Err("软件卸载扫描目前仅支持 Windows".into())
    }
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
