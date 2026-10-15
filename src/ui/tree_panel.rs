//! 左侧目录树面板:递归渲染目录节点及大小占比条。
//! 节点行右键:复制路径 / 在资源管理器中打开 / 删除目录到回收站。

use crate::engine::tree::{Tree, human_size, ROOT};
use crate::ui::files_panel::show_row_menu;
use crate::ui::RowAction;

/// 每节点最多渲染的子目录数(整盘大目录防卡顿,其余按“未列出”提示)
const MAX_CHILDREN_SHOWN: usize = 200;

/// 面板入口:标题 + 双向滚动区;返回本帧右键菜单触发的动作
pub fn show(ui: &mut egui::Ui, tree: &Tree) -> Option<RowAction> {
    ui.strong("目录树");
    let mut row_action = None;
    egui::ScrollArea::both().show(ui, |ui| {
        let root_size = tree.nodes[ROOT as usize].size;
        show_dir_rows(ui, tree, ROOT, root_size, 0, &mut row_action);
    });
    row_action
}

/// 递归渲染一个目录节点的子项:名称 + 大小 + 占根目录比例条
fn show_dir_rows(
    ui: &mut egui::Ui,
    tree: &Tree,
    idx: u32,
    root_size: u64,
    depth: usize,
    row_action: &mut Option<RowAction>,
) {
    let mut kids: Vec<u32> = tree.nodes[idx as usize].children.clone();
    kids.sort_by_key(|c| std::cmp::Reverse(tree.nodes[*c as usize].size));
    let total = kids.len();
    for &c in kids.iter().take(MAX_CHILDREN_SHOWN) {
        let node = &tree.nodes[c as usize];
        let pct = if root_size > 0 {
            node.size as f32 / root_size as f32
        } else {
            0.0
        };
        let denied_tag = if node.denied { " [受限]" } else { "" };
        let label = format!(
            "{}  {}  {:.1}%{}",
            node.name,
            human_size(node.size),
            pct * 100.0,
            denied_tag
        );
        let dir_path = tree.dir_path(c);
        if node.children.is_empty() {
            let resp = ui.add(
                egui::ProgressBar::new(pct)
                    .text(label)
                    .desired_width(ui.available_width().min(360.0)),
            );
            resp.context_menu(|ui| {
                if let Some(a) = show_row_menu(&dir_path, ui) {
                    *row_action = Some(a);
                }
            });
        } else {
            let resp = egui::CollapsingHeader::new(label)
                .id_salt(("dir", c))
                .default_open(depth == 0 && node.size > 0)
                .show(ui, |ui| show_dir_rows(ui, tree, c, root_size, depth + 1, row_action))
                .header_response;
            resp.context_menu(|ui| {
                if let Some(a) = show_row_menu(&dir_path, ui) {
                    *row_action = Some(a);
                }
            });
        }
    }
    if total > MAX_CHILDREN_SHOWN {
        ui.label(
            egui::RichText::new(format!("… 还有 {} 个较小目录未列出", total - MAX_CHILDREN_SHOWN))
                .weak(),
        );
    }
}
