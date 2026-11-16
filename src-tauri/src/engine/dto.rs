//! 引擎内存结构 → 前端可序列化 DTO 的转换层。
//! 字段统一为 camelCase,与 `src/types.ts` 一一对应。
//!
//! 性能:目录节点(可能数十万个)的 name/children 以借用形式序列化,
//! 避免每次快照都克隆几十万个小 Vec/String —— 这是大目录扫描时的主要分配开销。

use serde::Serialize;

use super::scanner::ScanResult;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DirNodeDto<'a> {
    pub parent: Option<u32>,
    pub name: &'a str,
    pub size: u64,
    pub file_count: u64,
    pub children: &'a [u32],
    pub denied: bool,
    pub deleted: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TreeDto<'a> {
    pub root_path: String,
    pub nodes: Vec<DirNodeDto<'a>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileHitDto {
    pub path: String,
    pub size: u64,
    pub dir_idx: u32,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExtStatDto {
    pub ext: String,
    pub count: u64,
    pub bytes: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanResultDto<'a> {
    pub tree: TreeDto<'a>,
    pub top_files: Vec<FileHitDto>,
    pub ext_stats: Vec<ExtStatDto>,
    pub denied: Vec<String>,
    pub total_files: u64,
    pub total_bytes: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanTickDto {
    pub dirs: u64,
    pub files: u64,
    pub bytes: u64,
    pub cur: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeleteStartedDto {
    pub path: String,
    pub done: usize,
    pub total: usize,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeleteItemDto {
    pub path: String,
    pub error: Option<String>,
    pub ok: usize,
    pub done: usize,
    pub total: usize,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeleteDoneDto {
    pub ok: usize,
    pub failed: usize,
    pub cancelled: bool,
    pub total: usize,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JunkRootDto {
    pub label: String,
    pub path: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JunkItemDto {
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    /// temp / cache / leftover / large / log(前端据此翻译)
    pub kind: String,
    pub reason: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JunkTickDto {
    pub scanned: usize,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct JunkScanDoneDto {
    pub items: Vec<JunkItemDto>,
    pub total_bytes: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InstalledAppDto {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub install_location: String,
    pub uninstall_command: String,
    pub estimated_size: u64,
}

fn path_str(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

pub fn to_dto(r: &ScanResult) -> ScanResultDto<'_> {
    ScanResultDto {
        tree: TreeDto {
            root_path: path_str(&r.tree.root_path),
            nodes: r
                .tree
                .nodes
                .iter()
                .map(|n| DirNodeDto {
                    parent: n.parent,
                    name: n.name.as_str(),
                    size: n.size,
                    file_count: n.file_count,
                    children: n.children.as_slice(),
                    denied: n.denied,
                    deleted: n.deleted,
                })
                .collect(),
        },
        top_files: r
            .top_files
            .iter()
            .map(|h| FileHitDto {
                path: path_str(&h.path),
                size: h.size,
                dir_idx: h.dir_idx,
            })
            .collect(),
        ext_stats: r
            .ext_stats
            .iter()
            .map(|e| ExtStatDto {
                ext: e.ext.clone(),
                count: e.count,
                bytes: e.bytes,
            })
            .collect(),
        denied: r.denied.iter().map(|p| path_str(p)).collect(),
        total_files: r.total_files,
        total_bytes: r.total_bytes,
    }
}
