//! 界面层:各面板的纯渲染代码。行为(改状态)仍由 app.rs 编排。

use std::path::PathBuf;

pub mod extension_panel;
pub mod files_panel;
pub mod status_bar;
pub mod toolbar;
pub mod tree_panel;
pub mod treemap_panel;

/// 右键菜单产生的一项操作,由各面板上报、app.rs 统一消费执行
#[derive(Clone)]
pub enum RowAction {
    /// 复制完整路径到剪贴板
    Copy(PathBuf),
    /// 删除到回收站(文件或目录)
    Delete(PathBuf),
    /// 在系统文件管理器中打开(目录打开自身,文件打开所在目录并选中)
    Reveal(PathBuf),
}

/// 列表/树面板共用的过滤条件(app.rs 每帧构建一次)
pub struct Filter {
    /// 已转小写的搜索词,空串表示不按名称过滤
    pub needle: String,
    /// 最小大小阈值,0 表示不限
    pub min_bytes: u64,
}

impl Filter {
    /// 是否处于过滤状态
    pub fn active(&self) -> bool {
        !self.needle.is_empty() || self.min_bytes > 0
    }

    /// 文件行:大小达标,且完整路径包含关键词(忽略大小写)
    pub fn matches_file(&self, path: &std::path::Path, size: u64) -> bool {
        size >= self.min_bytes
            && (self.needle.is_empty()
                || path.to_string_lossy().to_lowercase().contains(&self.needle))
    }

    /// 目录节点:按名称匹配 + 大小阈值
    pub fn matches_dir(&self, name: &str, size: u64) -> bool {
        size >= self.min_bytes && (self.needle.is_empty() || name.to_lowercase().contains(&self.needle))
    }
}
