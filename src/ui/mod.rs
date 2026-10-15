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
    /// 在系统资源管理器中打开(目录打开自身,文件打开所在目录并选中)
    Reveal(PathBuf),
}
