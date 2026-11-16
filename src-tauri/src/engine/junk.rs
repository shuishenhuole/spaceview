//! 残留 / 垃圾扫描引擎:在用户显式选择的常见位置(临时目录、浏览器缓存、
//! 下载目录等)里找出可清理项:
//!   - 临时文件(*.tmp / ~$ / 崩溃转储 / 未完成下载)
//!   - 缓存目录中的文件
//!   - 卸载残留(空目录、*.old / *.bak)
//!   - 日志文件、长期未访问的大文件
//!
//! 只扫描给定根目录、绝不写入;所有删除统一走回收站(trash),可随时恢复。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use jwalk::WalkDirGeneric;

/// 单次扫描最多收集的条目数(防止极端目录把列表撑爆)
pub const MAX_JUNK_ITEMS: usize = 5000;
/// 计入"大文件"的最小字节数
pub const LARGE_FILE_MIN: u64 = 100 * 1024 * 1024;
/// 未访问超过该天数才计为"残留"
const STALE_SECS: u64 = 180 * 24 * 3600;

const TEMP_EXT: &[&str] = &["tmp", "temp", "partial", "part", "crdownload", "dmp", "chk"];
const LEFTOVER_EXT: &[&str] = &["old", "bak", "orig", "backup"];
const LOG_EXT: &[&str] = &["log"];
const TEMP_DIR_NAMES: &[&str] = &["temp", "tmp"];
const CACHE_DIR_NAMES: &[&str] = &[
    "cache",
    "code cache",
    "gpucache",
    "cachestorage",
    "cache_storage",
    "crashpad",
];

#[derive(Clone, Copy, PartialEq)]
pub enum JunkKind {
    Temp,
    Cache,
    Leftover,
    Large,
    Log,
}

impl JunkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            JunkKind::Temp => "temp",
            JunkKind::Cache => "cache",
            JunkKind::Leftover => "leftover",
            JunkKind::Large => "large",
            JunkKind::Log => "log",
        }
    }
}

#[derive(Clone)]
pub struct JunkItem {
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
    pub kind: JunkKind,
    pub reason: String,
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// 常见可扫描位置(存在才返回)
pub fn junk_roots() -> Vec<(&'static str, PathBuf)> {
    let mut v: Vec<(&'static str, PathBuf)> = Vec::new();
    let mut consider = |label: &'static str, path: PathBuf| {
        if path.is_dir() {
            v.push((label, path));
        }
    };

    // 系统临时目录(%TEMP% / /tmp)
    consider("系统临时目录", std::env::temp_dir());
    if let Some(home) = home_dir() {
        #[cfg(windows)]
        {
            consider("用户临时目录", home.join("AppData/Local/Temp"));
            consider("崩溃转储", home.join("AppData/Local/CrashDumps"));
            consider(
                "Chrome 缓存",
                home.join("AppData/Local/Google/Chrome/User Data/Default/Cache"),
            );
            consider(
                "Edge 缓存",
                home.join("AppData/Local/Microsoft/Edge/User Data/Default/Cache"),
            );
            consider("下载目录", home.join("Downloads"));
        }
        #[cfg(not(windows))]
        {
            consider("用户缓存", home.join(".cache"));
            consider("下载目录", home.join("Downloads"));
        }
    }
    #[cfg(windows)]
    consider("Windows 临时目录", PathBuf::from(r"C:\Windows\Temp"));

    v
}

fn is_stale(mtime: u64) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    now > mtime && now - mtime >= STALE_SECS
}

/// 按扩展名 / 文件名模式 / 所在目录性质归类;不匹配返回 None(不收集)
fn classify(path: &Path, size: u64, mtime: u64) -> Option<(JunkKind, String)> {
    // 1) 长期未访问的大文件
    if size >= LARGE_FILE_MIN && is_stale(mtime) {
        return Some((JunkKind::Large, "长期未访问的大文件".into()));
    }
    // 2) 文件名模式
    let name = path
        .file_name()?
        .to_string_lossy()
        .to_lowercase();
    if name.starts_with("~$") {
        return Some((JunkKind::Temp, "Office 临时文件".into()));
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if TEMP_EXT.contains(&ext.as_str()) {
        return Some((JunkKind::Temp, "临时文件".into()));
    }
    if LEFTOVER_EXT.contains(&ext.as_str()) {
        return Some((JunkKind::Leftover, "备份 / 旧版残留".into()));
    }
    if LOG_EXT.contains(&ext.as_str()) {
        return Some((JunkKind::Log, "日志文件".into()));
    }
    // 3) 路径所在目录的性质(临时 / 缓存目录里的任何非零文件)
    for comp in path.components() {
        let s = comp.as_os_str().to_string_lossy().to_lowercase();
        if CACHE_DIR_NAMES.contains(&s.as_str()) {
            return Some((JunkKind::Cache, "缓存文件".into()));
        }
        if TEMP_DIR_NAMES.contains(&s.as_str()) {
            return Some((JunkKind::Temp, "临时目录中的文件".into()));
        }
    }
    None
}

/// 并行遍历选定位置,收集可清理项;`on_tick` 用于进度上报。
pub fn scan_junk(
    roots: &[PathBuf],
    cancel: Arc<AtomicBool>,
    mut on_tick: impl FnMut(usize),
) -> Vec<JunkItem> {
    let items: Mutex<Vec<JunkItem>> = Mutex::new(Vec::new());
    let mut scanned = 0usize;

    for root in roots {
        if cancel.load(Ordering::Relaxed) || items.lock().unwrap().len() >= MAX_JUNK_ITEMS {
            break;
        }
        let walk = WalkDirGeneric::<((), Option<(u64, u64)>)>::new(root)
            .skip_hidden(false)
            .follow_links(false)
            .parallelism(jwalk::Parallelism::RayonNewPool(
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4)
                    .min(8),
            ))
            .process_read_dir(|_depth, _path, _state, entries| {
                for entry in entries.iter_mut().flatten() {
                    if entry.file_type().is_dir() {
                        entry.client_state = None;
                    } else {
                        entry.client_state = entry.metadata().ok().map(|m| {
                            let mtime = m
                                .modified()
                                .ok()
                                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                                .map(|d| d.as_secs())
                                .unwrap_or(0);
                            (m.len(), mtime)
                        });
                    }
                }
            });

        let root_path = root.clone();
        for entry in walk.into_iter().flatten() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            scanned += 1;
            if scanned % 500 == 0 {
                on_tick(scanned);
            }
            let path = entry.path();
            if path == root_path {
                continue;
            }
            if entry.file_type().is_dir() {
                // 空目录 = 卸载残留
                let empty = std::fs::read_dir(&path)
                    .map(|mut d| d.next().is_none())
                    .unwrap_or(false);
                if empty {
                    let mut guard = items.lock().unwrap();
                    if guard.len() < MAX_JUNK_ITEMS {
                        guard.push(JunkItem {
                            path: path.clone(),
                            size: 0,
                            is_dir: true,
                            kind: JunkKind::Leftover,
                            reason: "空目录".into(),
                        });
                    }
                }
                continue;
            }
            let Some((size, mtime)) = entry.client_state else {
                continue;
            };
            if size == 0 {
                continue;
            }
            if let Some((kind, reason)) = classify(&path, size, mtime) {
                let mut guard = items.lock().unwrap();
                if guard.len() >= MAX_JUNK_ITEMS {
                    break;
                }
                guard.push(JunkItem {
                    path: path.clone(),
                    size,
                    is_dir: false,
                    kind,
                    reason,
                });
            }
        }
        if items.lock().unwrap().len() >= MAX_JUNK_ITEMS {
            break;
        }
    }

    on_tick(scanned);
    let mut out = items.into_inner().unwrap_or_default();
    out.sort_by(|a, b| b.size.cmp(&a.size));
    out
}
