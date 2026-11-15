//! 后台扫描引擎:用 jwalk 并行遍历目录树,构建目录节点 Arena,
//! 同时统计 TopN 大文件与扩展名占比。
//!
//! 与 UI 完全解耦:进度 / 每秒部分快照 / 完成结果通过 `on_event` 回调上报,
//! 由上层(Tauri commands)转成事件推给前端。
//!
//! 设计要点:
//! - 只为目录建立节点,文件只累加到直接父目录,最后做一次逆序自底向上聚合
//!   (父节点总是先于子节点入 Arena,因此逆序遍历即可完成聚合)
//! - AccessDenied 等错误条目被收集为 denied 样本,不中断整体扫描
//! - 取消通过外部 Arc<AtomicBool>,主循环每收到一个条目检查一次
//! - macOS 特殊处理:根扫描跳过 APFS 数据卷的重复挂载点,文件按实际占用计
//!   (st_blocks)而非逻辑长度,避免稀疏/压缩/克隆文件把总量撑大

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::engine::tree::{DirNode, Tree, ROOT};

pub const TOP_N_LIMIT: usize = 10_000;
const DENIED_SAMPLES: usize = 200;
/// 前端交互节流:进度每 500ms 推一次(原 200ms 过于频繁,IPC 会拖慢遍历)。
const TICK_INTERVAL: Duration = Duration::from_millis(500);
/// 部分快照更重(整棵树的克隆 + 聚合 + 序列化),保持 1s 一次;
/// 与 500ms 的进度 tick 共用同一个调度点,避免额外计时器检查。
const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(1000);

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
#[derive(Clone)]
pub struct ScanResult {
    pub tree: Tree,
    pub top_files: Vec<FileHit>, // 按 size 降序,最多 TOP_N_LIMIT 条
    pub ext_stats: Vec<ExtStat>, // 按 bytes 降序
    pub denied: Vec<PathBuf>,    // 无权限路径样本(上限 200)
    pub total_files: u64,
    pub total_bytes: u64,
}

/// 扫描线程 → 上层的事件
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
}

impl ScanResult {
    /// 一批路径已成功移到回收站后,一次性把内存模型同步到现实:
    /// 回减目录树大小、剔除 TopN、扣减扩展名统计与总计。
    ///
    /// 后台删除结束后调用本方法做一次批量更新,避免逐项线性查找造成的
    /// O(n²) 卡顿。
    pub fn apply_deleted(&mut self, deleted: &[PathBuf]) {
        if deleted.is_empty() {
            return;
        }
        let wanted: HashSet<&Path> = deleted.iter().map(|p| p.as_path()).collect();

        // 1) 文件命中:单次遍历 TopN,累计祖先回减与扩展名扣减
        let mut hit_paths: HashSet<PathBuf> = HashSet::new();
        let mut ext_delta: HashMap<String, (u64, u64)> = HashMap::new();
        let mut file_bytes = 0u64;
        let mut file_count = 0u64;
        for hit in &self.top_files {
            if !wanted.contains(hit.path.as_path()) {
                continue;
            }
            self.tree.subtract(hit.dir_idx, hit.size, 1);
            let ext = hit
                .path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_else(|| "(无扩展名)".to_string());
            let entry = ext_delta.entry(ext).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += hit.size;
            file_bytes += hit.size;
            file_count += 1;
            hit_paths.insert(hit.path.clone());
        }
        for st in &mut self.ext_stats {
            if let Some(&(count, bytes)) = ext_delta.get(&st.ext) {
                st.count = st.count.saturating_sub(count);
                st.bytes = st.bytes.saturating_sub(bytes);
            }
        }
        self.top_files.retain(|h| !hit_paths.contains(&h.path));
        self.total_bytes = self.total_bytes.saturating_sub(file_bytes);
        self.total_files = self.total_files.saturating_sub(file_count);

        // 2) 目录删除:剩余路径按路径找节点(一次全树遍历只建所需索引)
        let dir_paths: Vec<&PathBuf> = deleted
            .iter()
            .filter(|p| !hit_paths.contains(*p))
            .collect();
        if dir_paths.is_empty() {
            return;
        }
        let dir_wanted: HashSet<&Path> = dir_paths.iter().map(|p| p.as_path()).collect();
        let mut index: HashMap<PathBuf, u32> = HashMap::new();
        for i in 0..self.tree.nodes.len() as u32 {
            let p = self.tree.dir_path(i);
            if dir_wanted.contains(p.as_path()) {
                index.insert(p, i);
            }
        }
        for p in dir_paths {
            if let Some(&idx) = index.get(p.as_path()) {
                let (size, files) = {
                    let n = &self.tree.nodes[idx as usize];
                    (n.size, n.file_count)
                };
                self.tree.remove_subtree(idx);
                self.total_bytes = self.total_bytes.saturating_sub(size);
                self.total_files = self.total_files.saturating_sub(files);
                // 被删目录下的文件若还在 TopN,一并剔除
                self.top_files.retain(|h| !h.path.starts_with(p));
            }
        }
    }
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
                .unwrap_or(ROOT);
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

/// 启动一次扫描(同步运行,应在工作线程中调用)。
/// cancel 置 true 后扫描在下一条目处中止;paused 置 true 时暂停消费
/// (jwalk 内部队列写满后自然停止推进)。
pub fn run_scan<F: FnMut(ScanEvent)>(
    root: PathBuf,
    cancel: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    on_event: F,
) -> Result<(), String> {
    run(root, cancel, paused, on_event)
}

/// 单个文件计入磁盘用量的字节数。
/// macOS 的 APFS 上逻辑长度(len)与实际占用可能相差极大(稀疏文件如 Docker.raw、
/// 压缩文件、克隆文件),按 st_blocks 统计才与 Finder"占用"/du 口径一致;
/// 其他平台沿用逻辑长度。
#[cfg(target_os = "macos")]
pub fn file_disk_size(meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.blocks().saturating_mul(512)
}

#[cfg(not(target_os = "macos"))]
pub fn file_disk_size(meta: &std::fs::Metadata) -> u64 {
    meta.len()
}

/// 根目录扫描时需要整棵剪掉的重复挂载/外挂卷。
#[cfg(target_os = "macos")]
fn is_duplicate_mount(path: &Path, root: &Path) -> bool {
    root == Path::new("/")
        && (path == Path::new("/System/Volumes/Data")
            || (path != Path::new("/Volumes") && path.starts_with("/Volumes")))
}

fn run<F: FnMut(ScanEvent)>(
    root: PathBuf,
    cancel: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    mut emit: F,
) -> Result<(), String> {
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
    #[cfg(target_os = "macos")]
    let skip_root = root.clone();
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
            // 必须在遍历前从 read_dir 结果中移除,才能阻止 jwalk 继续下钻
            #[cfg(target_os = "macos")]
            entries.retain(|entry| match entry {
                Ok(e) => !is_duplicate_mount(&e.path(), &skip_root),
                Err(_) => true,
            });
            for entry in entries.iter_mut().flatten() {
                if !entry.file_type().is_dir() {
                    entry.client_state = entry.metadata().map(|m| file_disk_size(&m)).ok();
                }
            }
            let _ = denied_ref; // 错误统一在主流中收集
        });

    for entry_result in walk {
        if cancel.load(Ordering::Relaxed) {
            emit(ScanEvent::Cancelled);
            return Ok(());
        }
        // 暂停:挂起主循环消费,期间仍响应取消;恢复后从断点继续
        while paused.load(Ordering::Relaxed) {
            if cancel.load(Ordering::Relaxed) {
                emit(ScanEvent::Cancelled);
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }
        match entry_result {
            Ok(entry) => {
                let path = entry.path();
                cur = path.to_string_lossy().into_owned();
                if entry.file_type().is_dir() {
                    dirs += 1;
                    // 根目录节点在 Tree::new 中已建好(idx 0),直接复用
                    if path == root {
                        node_map.insert(path, ROOT);
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
                        deleted: false,
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
                            let i = node_map.get(dir).copied().unwrap_or(ROOT);
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
                        // 固定字面量作为统计键,展示层再翻译,避免语言切换破坏键匹配
                        .unwrap_or_else(|| "(无扩展名)".to_string());
                    let e = exts.entry(ext).or_insert((0, 0));
                    e.0 += 1;
                    e.1 += len;
                }
            }
            Err(err) => {
                if let Some(p) = err.path() {
                    let mut d = denied.lock().unwrap();
                    if d.len() < DENIED_SAMPLES {
                        d.push(p.to_path_buf());
                    }
                    drop(d);
                    if let Some(&idx) = node_map.get(p) {
                        tree.nodes[idx as usize].denied = true;
                    }
                }
            }
        }
        // 统一 500ms 调度点:每 500ms 推一次进度;每 1s 顺便带一次部分快照。
        // 快照要克隆整棵树并序列化成 JSON,频率过高会反噬扫描速度,故不放宽到 500ms。
        if last_tick.elapsed() >= TICK_INTERVAL {
            last_tick = Instant::now();
            emit(ScanEvent::Tick {
                dirs,
                files,
                bytes,
                cur: cur.clone(),
            });
            if last_snap.elapsed() >= SNAPSHOT_INTERVAL {
                last_snap = Instant::now();
                emit(ScanEvent::Snapshot(Box::new(build_partial(
                    &tree, &top, &exts, &node_map, &denied, files, bytes,
                ))));
            }
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
                .unwrap_or(ROOT);
            FileHit {
                path,
                size,
                dir_idx,
            }
        })
        .collect();
    top_files.sort_unstable_by(|a, b| b.size.cmp(&a.size));

    let mut ext_stats: Vec<ExtStat> = exts
        .into_iter()
        .map(|(ext, (count, bytes))| ExtStat { ext, count, bytes })
        .collect();
    ext_stats.sort_unstable_by(|a, b| b.bytes.cmp(&a.bytes));

    emit(ScanEvent::Done(Box::new(ScanResult {
        tree,
        top_files,
        ext_stats,
        denied: denied.lock().unwrap().clone(),
        total_files: files,
        total_bytes: bytes,
    })));
    Ok(())
}

fn dir_display_name(path: &Path, root: &Path) -> String {
    if path == root {
        return path.to_string_lossy().into_owned();
    }
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}
