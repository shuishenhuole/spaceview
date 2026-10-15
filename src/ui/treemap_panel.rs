//! Treemap 面板:渲染当前目录的子块,左键点击返回下钻目标,
//! 右键弹出菜单(复制路径 / 打开文件夹 / 删除到回收站)。

use crate::engine::tree::{human_size, Tree};
use crate::engine::treemap::squarify;
use crate::ui::files_panel::show_row_menu;
use crate::ui::RowAction;

/// 渲染 current 目录的矩形树。返回 (被点击下钻的子目录索引, 右键菜单动作)。
pub fn show(ui: &mut egui::Ui, tree: &Tree, current: u32) -> (Option<u32>, Option<RowAction>) {
    let rect = {
        let (r, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        r
    };
    if !rect.is_positive() {
        return (None, None);
    }

    // 项 = 子目录 + 本目录直属文件(灰色块)
    let mut items: Vec<(Option<u32>, u64)> = tree
        .sorted_children(current)
        .into_iter()
        .map(|(idx, _, size)| (Some(idx), size))
        .collect();
    let children_sum: u64 = items.iter().map(|(_, s)| *s).sum();
    let own_files = tree.nodes[current as usize]
        .size
        .saturating_sub(children_sum);
    if own_files > 0 {
        items.push((None, own_files));
    }
    if items.is_empty() {
        ui.label(egui::RichText::new("空目录").weak());
        return (None, None);
    }

    let laid = squarify(&items, rect);
    let painter = ui.painter_at(
        // 稍放大避免裁剪边框
        rect.expand(1.0),
    );
    let mut clicked = None;
    let mut row_action: Option<RowAction> = None;

    for (n, (marker, r)) in laid.iter().enumerate() {
        let (label, size, count_tag) = match marker {
            Some(idx) => {
                let node = &tree.nodes[*idx as usize];
                (
                    node.name.clone(),
                    node.size,
                    format!("{} 个文件", node.file_count),
                )
            }
            None => (
                "本目录文件".to_owned(),
                own_files,
                format!("{} 个文件", tree.nodes[current as usize].file_count),
            ),
        };
        // 颜色:固定调色板轮换;直属文件块用灰色
        const PALETTE: [[u8; 3]; 8] = [
            [99, 143, 190], [121, 173, 121], [191, 139, 103], [163, 117, 163],
            [103, 163, 163], [173, 143, 99], [139, 103, 173], [110, 158, 110],
        ];
        let fill = match marker {
            Some(_) => {
                let c = PALETTE[n % PALETTE.len()];
                egui::Color32::from_rgb(c[0], c[1], c[2])
            }
            None => ui.visuals().faint_bg_color,
        };
        painter.rect_filled(*r, 1.0, fill);
        painter.rect_stroke(
            *r,
            1.0,
            egui::Stroke::new(1.0, ui.visuals().window_fill()),
            egui::StrokeKind::Inside,
        );

        // 文字:仅在矩形够大时绘制,宽度不足手动截断
        if r.width() > 70.0 && r.height() > 18.0 {
            let max_chars = (r.width() / 7.5).clamp(3.0, 60.0) as usize;
            let short: String = label.chars().take(max_chars).collect();
            painter.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                short,
                egui::FontId::proportional(12.0),
                ui.visuals().strong_text_color(),
            );
        }

        // 交互层:左键下钻 + 悬停详情 + 右键菜单
        // 灰色"本目录文件"块没有独立节点,菜单作用于当前目录本身
        let menu_path = match marker {
            Some(idx) => tree.dir_path(*idx),
            None => tree.dir_path(current),
        };
        let resp = ui.interact(*r, ui.id().with(("tm", n)), egui::Sense::click());
        if let Some(idx) = marker {
            if resp.clicked() {
                clicked = Some(*idx);
            }
        }
        let resp2 = resp.on_hover_text(format!(
            "{label}\n{}\n{}  {}",
            menu_path.display(),
            human_size(size),
            count_tag
        ));
        // 灰色块不提供删除(删目录会把直属文件一起删,语义留给具体目录块)
        if marker.is_some() {
            resp2.context_menu(|ui| {
                if let Some(a) = show_row_menu(&menu_path, ui) {
                    row_action = Some(a);
                }
            });
        }
    }
    (clicked, row_action)
}
