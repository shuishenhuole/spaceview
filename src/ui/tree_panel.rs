//! 左侧目录树面板:递归渲染目录节点及大小占比条。
//! 节点行右键:复制路径 / 在资源管理器中打开 / 删除目录到回收站。

use crate::engine::tree::{Tree, human_size, ROOT};
use crate::ui::files_panel::show_row_menu;
use crate::ui::{Filter, RowAction};

/// 每节点最多渲染的子目录数(整盘大目录防卡顿,其余按“未列出”提示)
const MAX_CHILDREN_SHOWN: usize = 200;
/// 过滤模式下最多显示的命中目录数
const FLAT_LIMIT: usize = 500;

/// 树面板对本帧的回报:右键动作 + 单击导航到的目录索引(联动 Treemap)
pub struct TreePanelOut {
    pub action: Option<RowAction>,
    pub nav: Option<u32>,
}

/// 面板入口:标题 + 双向滚动区。
/// 过滤激活时切换为“搜索结果”扁平列表,不再递归展开树。
pub fn show(ui: &mut egui::Ui, tree: &Tree, filter: &Filter) -> TreePanelOut {
    let mut out = TreePanelOut {
        action: None,
        nav: None,
    };
    if filter.active() {
        show_flat(ui, tree, filter, &mut out);
        return out;
    }
    ui.strong("目录树");
    egui::ScrollArea::both().show(ui, |ui| {
        let root_size = tree.nodes[ROOT as usize].size;
        show_dir_rows(ui, tree, ROOT, root_size, 0, &mut out);
    });
    out
}

/// 过滤模式:全节点扫一遍,按名称+大小阈值命中,扁平显示(上限 FLAT_LIMIT 条)
fn show_flat(ui: &mut egui::Ui, tree: &Tree, filter: &Filter, out: &mut TreePanelOut) {
    ui.strong(format!("目录树 — 搜索 “{}”", filter.needle));
    let mut shown = 0usize;
    let mut total = 0usize;
    egui::ScrollArea::both().show(ui, |ui| {
        // 跳过根节点自身(索引 0),按 Arena 创建序遍历
        for (i, node) in tree.nodes.iter().enumerate().skip(1) {
            if !filter.matches_dir(&node.name, node.size) {
                continue;
            }
            total += 1;
            if shown >= FLAT_LIMIT {
                continue;
            }
            shown += 1;
            let path = tree.dir_path(i as u32);
            let label = format!(
                "{}  {}{}",
                node.name,
                human_size(node.size),
                if node.denied { " [受限]" } else { "" }
            );
            let resp = ui.add(
                egui::Label::new(egui::RichText::new(label)).sense(egui::Sense::click()),
            );
            if resp.clicked() {
                out.nav = Some(i as u32);
            }
            resp.on_hover_text(format!("{}\n单击在 Treemap 中定位", path.display()))
                .context_menu(|ui| {
                    if let Some(a) = show_row_menu(&path, ui) {
                        out.action = Some(a);
                    }
                });
        }
    });
    if total == 0 {
        ui.label(egui::RichText::new("无匹配目录").weak().italics());
    } else if total > shown {
        ui.label(
            egui::RichText::new(format!("共命中 {total} 个目录,仅显示前 {shown} 个"))
                .weak()
                .small(),
        );
    }
}

/// 递归渲染一个目录节点的子项:名称 + 大小 + 占根目录比例条。
/// 单击行 → 回报 nav(Treemap 联动下钻);右键 → 菜单动作。
fn show_dir_rows(
    ui: &mut egui::Ui,
    tree: &Tree,
    idx: u32,
    root_size: u64,
    depth: usize,
    out: &mut TreePanelOut,
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
            // ProgressBar 本身无 click 语义,叠一层交互矩形承接单击与右键
            let click = ui.interact(
                resp.rect,
                ui.id().with(("trow", c)),
                egui::Sense::click(),
            );
            if click.clicked() {
                out.nav = Some(c);
            }
            click.context_menu(|ui| {
                if let Some(a) = show_row_menu(&dir_path, ui) {
                    out.action = Some(a);
                }
            });
        } else {
            let resp = egui::CollapsingHeader::new(label)
                .id_salt(("dir", c))
                .default_open(depth == 0 && node.size > 0)
                .show(ui, |ui| show_dir_rows(ui, tree, c, root_size, depth + 1, out))
                .header_response;
            if resp.clicked() {
                out.nav = Some(c);
            }
            resp.context_menu(|ui| {
                if let Some(a) = show_row_menu(&dir_path, ui) {
                    out.action = Some(a);
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
