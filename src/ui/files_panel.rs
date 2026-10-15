//! 大文件 TopN 面板:可按大小/路径排序的列表(仅渲染前 1000 行防卡顿)。

use crate::engine::scanner::{FileHit, ScanResult};
use crate::engine::tree::human_size;

/// 排序方式
#[derive(PartialEq, Clone, Copy)]
pub enum SortBy {
    Size,
    Name,
}

const RENDER_LIMIT: usize = 1000;

pub fn show(result: &ScanResult, sort_by: &mut SortBy, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label("排序:");
        ui.selectable_value(sort_by, SortBy::Size, "按大小");
        ui.selectable_value(sort_by, SortBy::Name, "按路径");
        ui.label(
            egui::RichText::new(format!(
                "(共 {} 条,渲染前 {} 条)",
                result.top_files.len(),
                RENDER_LIMIT
            ))
            .weak()
            .small(),
        );
    });

    // 按路径排序时临时复制排序(数据只在 Done 状态,量可控)
    let sorted_name: Vec<FileHit>;
    let rows: &[FileHit] = if *sort_by == SortBy::Name {
        let mut v = result.top_files.clone();
        v.sort_by(|a, b| a.path.cmp(&b.path));
        sorted_name = v;
        &sorted_name
    } else {
        &result.top_files
    };

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_sized([100.0, 20.0], egui::Label::new("大小").selectable(false));
                ui.label("文件(悬停看完整路径)");
            });
            ui.separator();
            for (i, hit) in rows.iter().take(RENDER_LIMIT).enumerate() {
                let name = hit
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| hit.path.display().to_string());
                let frame = egui::Frame::new()
                    .fill(if i % 2 == 0 {
                        ui.visuals().faint_bg_color
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .inner_margin(egui::Margin::symmetric(4, 1));
                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [96.0, 18.0],
                            egui::Label::new(human_size(hit.size)).selectable(false),
                        );
                        ui.strong(name).on_hover_text(hit.path.display().to_string());
                    });
                });
            }
        });
}
