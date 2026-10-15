//! Treemap 面板:渲染当前目录的子块,左键点击返回下钻目标,
//! 右键弹出菜单(复制路径 / 打开文件夹 / 删除到回收站)。

use crate::engine::tree::{human_size, Tree};
use crate::engine::treemap::squarify;
use crate::ui::files_panel::show_row_menu;
use crate::ui::RowAction;

/// 按像素宽度估算截断文本:中文≈1em、ASCII≈0.55em,超出时以省略号结尾
fn fit_width(s: &str, max_px: f32, font_px: f32) -> String {
    let mut w = 0.0f32;
    let mut out = String::new();
    for ch in s.chars() {
        let cw = if ch.is_ascii() {
            font_px * 0.55
        } else {
            font_px * 1.05
        };
        if w + cw > max_px {
            out.push('…');
            break;
        }
        w += cw;
        out.push(ch);
    }
    out
}

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
    // 当前目录总大小,用于色块悬停占比
    let cur_size = tree.nodes[current as usize].size;

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

        // 文字:仅在矩形够大时绘制;高度充足时名字与占用大小分两行,空间不够则合并为一行
        // 截断按像素宽度估算(中文≈1em、ASCII≈0.55em),避免中文名溢出
        if r.width() > 70.0 && r.height() > 18.0 {
            let size_str = human_size(size);
            if r.height() > 36.0 {
                let short = fit_width(&label, r.width() - 8.0, 12.0);
                painter.text(
                    r.center() - egui::vec2(0.0, 8.0),
                    egui::Align2::CENTER_CENTER,
                    &short,
                    egui::FontId::proportional(12.0),
                    ui.visuals().strong_text_color(),
                );
                painter.text(
                    r.center() + egui::vec2(0.0, 9.0),
                    egui::Align2::CENTER_CENTER,
                    &size_str,
                    egui::FontId::proportional(11.0),
                    ui.visuals().strong_text_color(),
                );
            } else {
                let line = fit_width(
                    &format!("{label} · {size_str}"),
                    r.width() - 8.0,
                    12.0,
                );
                painter.text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    line,
                    egui::FontId::proportional(12.0),
                    ui.visuals().strong_text_color(),
                );
            }
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
        let pct = size as f32 / cur_size.max(1) as f32 * 100.0;
        let resp2 = resp.on_hover_text(format!(
            "{label}\n{}\n{}  {} · 占当前视图 {:.1}%",
            menu_path.display(),
            human_size(size),
            count_tag,
            pct
        ));
        // 悬停块加亮边框
        if resp2.hovered() {
            painter.rect_stroke(
                *r,
                1.0,
                egui::Stroke::new(2.0, ui.visuals().strong_text_color()),
                egui::StrokeKind::Inside,
            );
        }
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
