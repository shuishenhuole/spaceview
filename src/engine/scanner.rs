//! 后台扫描引擎:用 jwalk 并行遍历目录树,构建目录节点 Arena,
//! 同时统计 TopN 大文件与扩展名占比,通过 mpsc 通道向 UI 汇报进度。
//!
//! 设计要点:
//! - 只为目录建立节点,文件只累加到直接父目录,最后做一次逆序自底向上聚合
//!   (父节点总是先于子节点入 Arena,因此逆序遍历即可完成聚合)
//! - AccessDenied 等错误条目被收集为 denied 样本,不中断整体扫描
//! - 取消通过外部 Arc<AtomicBool>,主循环每收到一个条目检查一次

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::engine::tree::{DirNode, Tree};

pub const TOP_N_LIMIT: usize = 10_000;
const DENIED_SAMPLES: usize = 200;

/// 一个大文件命中记录
#[derive(Clone)]
pub struct FileHit {
    pub path: PathBuf,
    pub size: u64,
    pub dir_idx: u32, // 所在目录节点在 Tree 中的索引
}

/// 单个扩展名的统计:文件数 + 总字节
#[derive(Clone)]
pub struct ExtStat {
    pub ext: String,
    pub count: u64,
    pub bytes: u64,
}

/// 一次扫描的完整结果
pub struct ScanResult {
    pub tree: Tree,
    pub top_files: Vec<FileHit>, // 按 size 降序,最多 TOP_N_LIMIT 条
    pub ext_stats: Vec<ExtStat>,  // 按 bytes 降序
    pub denied: Vec<PathBuf>,     // 无权限路径样本(上限 200)
    pub total_files: u64,
    pub total_bytes: u64,
}

/// 扫描线程 → UI 线程的事件
pub enum ScanEvent {
    Tick {
        dirs: u64,
        files: u64,
        bytes: u64,
        cur: String,
    },
    /// 扫描过程中的部分快照(树已聚合、TopN/扩展名为当前值),供 UI 实时展示
    Snapshot(Box<ScanResult>),
    Done(Box<ScanResult>),
    Cancelled,
    Failed(String),
}

/// 克隆当前树并做自底向上聚合,得到"当前已扫描内容"的完整大小视图。
/// 扫描主循环中 node.size 只含直属文件,聚合放在克隆上进行,避免拖慢主遍历。
fn aggregate_snapshot(src: &Tree) -> Tree {
    let mut c = src.clone();
    for i in (1..c.nodes.len()).rev() {
        let (parent, size, count) = {
            let n = &c.nodes[i];
            (n.parent, n.size, n.file_count)
        };
        if let Some(p) = parent {
            let pn = &mut c.nodes[p as usize];
            pn.size += size;
            pn.file_count += count;
        }
    }
    c
}

/// 从当前 TopN 堆与扩展名表构建部分结果(不消耗主数据结构)
#[allow(clippy::type_complexity)]
fn build_partial(
    tree: &Tree,
    top: &BinaryHeap<(Reverse<u64>, PathBuf)>,
    exts: &HashMap<String, (u64, u64)>,
    node_map: &HashMap<PathBuf, u32>,
    denied: &Mutex<Vec<PathBuf>>,
    files: u64,
    bytes: u64,
) -> ScanResult {
    let mut top_files: Vec<FileHit> = top
        .clone()
        .into_vec()
        .into_iter()
        .map(|(Reverse(size), path)| {
            let dir_idx = node_map
                .get(path.parent().unwrap_or(Path::new("")))
                .copied()
                .unwrap_or(crate::engine::tree::ROOT);
            FileHit {
                path,
                size,
                dir_idx,
            }
        })
        .collect();
    top_files.sort_unstable_by(|a, b| b.size.cmp(&a.size));

    let mut ext_stats: Vec<ExtStat> = exts
        .iter()
        .map(|(ext, (count, bytes))| ExtStat {
            ext: ext.clone(),
            count: *count,
            bytes: *bytes,
        })
        .collect();
    ext_stats.sort_unstable_by(|a, b| b.bytes.cmp(&a.bytes));

    ScanResult {
        tree: aggregate_snapshot(tree),
        top_files,
        ext_stats,
        denied: denied.lock().unwrap().clone(),
        total_files: files,
        total_bytes: bytes,
    }
}

/// 启动后台扫描,返回事件接收端;cancel 置 true 后扫描在下一条目处中止
pub fn spawn_scan(root: PathBuf, cancel: Arc<AtomicBool>) -> Receiver<ScanEvent> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        match run(root, cancel, tx.clone()) {
            Ok(()) => {}
            Err(e) => {
                let _ = tx.send(ScanEvent::Failed(e));
            }
        }
    });
    rx
}

fn run(root: PathBuf, cancel: Arc<AtomicBool>, tx: mpsc::Sender<ScanEvent>) -> Result<(), String> {
    if !root.is_dir() {
        return Err(format!("路径不存在或不是目录: {}", root.display()));
    }

    let denied: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
    let mut tree = Tree::new(root.clone());
    let mut node_map: HashMap<PathBuf, u32> = HashMap::new();
    let mut top: BinaryHeap<(Reverse<u64>, PathBuf)> = BinaryHeap::new();
    let mut exts: HashMap<String, (u64, u64)> = HashMap::new();
    let mut last_parent: Option<(PathBuf, u32)> = None;

    let mut dirs: u64 = 0;
    let mut files: u64 = 0;
    let mut bytes: u64 = 0;
    let mut cur = String::new();
    let mut last_tick = Instant::now();
    let mut last_snap = Instant::now();

    let denied_ref = &denied;
    let walk = jwalk::WalkDirGeneric::<((), Option<u64>)>::new(&root)
        .skip_hidden(false)
        .follow_links(false)
        .parallelism(jwalk::Parallelism::RayonNewPool(
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .min(16),
        ))
        .process_read_dir(move |_depth, _path, _parent_state, entries| {
            for entry in entries.iter_mut().flatten() {
                if !entry.file_type().is_dir() {
                    entry.client_state = entry.metadata().map(|m| m.len()).ok();
                }
            }
            let _ = denied_ref; // 错误统一在主流中收集
        });

    for entry_result in walk {
        if cancel.load(Ordering::Relaxed) {
            let _ = tx.send(ScanEvent::Cancelled);
            return Ok(());
        }
        match entry_result {
            Ok(entry) => {
                let path = entry.path();
                cur = path.to_string_lossy().into_owned();
                if entry.file_type().is_dir() {
                    dirs += 1;
                    // 根目录节点在 Tree::new 中已建好(idx 0),直接复用,避免重复建根导致聚合断裂
                    if path == root {
                        node_map.insert(path, crate::engine::tree::ROOT);
                        continue;
                    }
                    let parent = node_map.get(path.parent().unwrap_or(Path::new(""))).copied();
                    let idx = tree.push_node(DirNode {
                        parent,
                        name: dir_display_name(&path, &root),
                        size: 0,
                        file_count: 0,
                        children: Vec::new(),
                        denied: false,
                    });
                    if let Some(p) = parent {
                        tree.nodes[p as usize].children.push(idx);
                    }
                    node_map.insert(path, idx);
                } else if let Some(len) = entry.client_state {
                    files += 1;
                    bytes += len;
                    let dir = path.parent().unwrap_or(Path::new(""));
                    let dir_idx = match &last_parent {
                        Some((p, i)) if p == dir => *i,
                        _ => {
                            let i = node_map.get(dir).copied().unwrap_or(crate::engine::tree::ROOT);
                            last_parent = Some((dir.to_path_buf(), i));
                            i
                        }
                    };
                    let node = &mut tree.nodes[dir_idx as usize];
                    node.size += len;
                    node.file_count += 1;

                    if top.len() < TOP_N_LIMIT {
                        top.push((Reverse(len), path.clone()));
                    } else if let Some((Reverse(min), _)) = top.peek() {
                        if &len > min {
                            top.pop();
                            top.push((Reverse(len), path.clone()));
                        }
                    }

                    let ext = path
                        .extension()
                        .map(|e| e.to_string_lossy().to_lowercase())
                        .unwrap_or_else(|| "(无扩展名)".to_string());
                    let e = exts.entry(ext).or_insert((0, 0));
                    e.0 += 1;
                    e.1 += len;
                }
            }
            Err(err) => {
                if let Some(p) = err.path() {
                    let p = p.to_path_buf();
                    let mut d = denied.lock().unwrap();
                    if d.len() < DENIED_SAMPLES {
                        d.push(p);
                    }
                    drop(d);
                    if let Some(&idx) = node_map.get(err.path().unwrap()) {
                        tree.nodes[idx as usize].denied = true;
                    }
                }
            }
        }
        if last_tick.elapsed() > Duration::from_millis(200) {
            last_tick = Instant::now();
            if tx
                .send(ScanEvent::Tick {
                    dirs,
                    files,
                    bytes,
                    cur: cur.clone(),
                })
                .is_err()
            {
                return Ok(()); // UI 已关闭
            }
        }
        // 每秒推送一次部分快照,让 UI 在扫描过程中即可展示已扫描内容
        if last_snap.elapsed() > Duration::from_secs(1) {
            last_snap = Instant::now();
            let _ = tx.send(ScanEvent::Snapshot(Box::new(build_partial(
                &tree, &top, &exts, &node_map, &denied, files, bytes,
            ))));
        }
    }

    // 自底向上聚合目录大小 (父节点总是先于子节点创建,逆序即可)
    for i in (1..tree.nodes.len()).rev() {
        let (parent, size, count) = {
            let n = &tree.nodes[i];
            (n.parent, n.size, n.file_count)
        };
        if let Some(p) = parent {
            let pn = &mut tree.nodes[p as usize];
            pn.size += size;
            pn.file_count += count;
        }
    }

    let mut top_files: Vec<FileHit> = top
        .into_vec()
        .into_iter()
        .map(|(Reverse(size), path)| {
            let dir_idx = node_map
                .get(path.parent().unwrap_or(Path::new("")))
                .copied()
                .unwrap_or(crate::engine::tree::ROOT);
            FileHit { path, size, dir_idx }
        })
        .collect();
    top_files.sort_unstable_by(|a, b| b.size.cmp(&a.size));

    let mut ext_stats: Vec<ExtStat> = exts
        .into_iter()
        .map(|(ext, (count, bytes))| ExtStat { ext, count, bytes })
        .collect();
    ext_stats.sort_unstable_by(|a, b| b.bytes.cmp(&a.bytes));

    let denied = denied.lock().unwrap().clone();
    tx.send(ScanEvent::Done(Box::new(ScanResult {
        tree,
        top_files,
        ext_stats,
        denied,
        total_files: files,
        total_bytes: bytes,
    })))
    .map_err(|_| "UI 已关闭".to_string())
}

fn dir_display_name(path: &Path, root: &Path) -> String {
    if path == root {
        return path.to_string_lossy().into_owned();
    }
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}
