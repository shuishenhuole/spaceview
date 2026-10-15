//! 大文件 TopN 面板:可按大小/路径排序的列表(仅渲染前 1000 行防卡顿),
//! 行上右键可复制路径 / 打开所在目录 / 删除到回收站。

use std::path::PathBuf;

use crate::engine::scanner::{FileHit, ScanResult};
use crate::engine::tree::human_size;
use crate::ui::{Filter, RowAction};

/// 排序方式
#[derive(PartialEq, Clone, Copy)]
pub enum SortBy {
    Size,
    Name,
}

const RENDER_LIMIT: usize = 1000;

pub fn show(
    result: &ScanResult,
    sort_by: &mut SortBy,
    selected: &mut Vec<PathBuf>,
    filter: &Filter,
    ui: &mut egui::Ui,
) -> Option<RowAction> {
    let mut row_action: Option<RowAction> = None;
    // 按过滤条件收集可见行(引用,不复制数据)
    let mut rows: Vec<&FileHit> = result
        .top_files
        .iter()
        .filter(|h| filter.matches_file(&h.path, h.size))
        .collect();
    if *sort_by == SortBy::Name {
        rows.sort_by(|a, b| a.path.cmp(&b.path));
    } else {
        // 扫描结果本身按大小降序;过滤后保持原顺序
    }
    ui.horizontal(|ui| {
        ui.label("排序:");
        ui.selectable_value(sort_by, SortBy::Size, "按大小");
        ui.selectable_value(sort_by, SortBy::Name, "按路径");
        if !selected.is_empty() {
            ui.label(egui::RichText::new(format!("已选 {} 项", selected.len())).color(egui::Color32::LIGHT_YELLOW));
        }
        ui.label(
            egui::RichText::new(format!(
                "(匹配 {} 条,渲染前 {} 条;勾选后可按 Delete 删除)",
                rows.len(),
                RENDER_LIMIT
            ))
            .weak()
            .small(),
        );
    });

    if rows.is_empty() {
        // 过滤后无匹配
        ui.label(
            egui::RichText::new("无匹配项,请调整搜索词或大小阈值")
                .weak()
                .italics(),
        );
        return row_action;
    }
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_sized([100.0, 20.0], egui::Label::new("大小").selectable(false));
                ui.label("文件(双击打开所在目录,右键更多操作)");
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
                        // 勾选框:选中集以路径为键
                        let mut sel = selected.contains(&hit.path);
                        if ui.checkbox(&mut sel, "").changed() {
                            if sel {
                                selected.push(hit.path.clone());
                            } else {
                                selected.retain(|p| p != &hit.path);
                            }
                        }
                        ui.add_sized(
                            [96.0, 18.0],
                            egui::Label::new(human_size(hit.size)).selectable(false),
                        );
                        // 文件名标签:click 语义支持右键菜单与双击打开所在目录
                        let path = hit.path.clone();
                        let resp = ui.add(
                            egui::Label::new(egui::RichText::new(name).strong())
                                .sense(egui::Sense::click()),
                        );
                        let resp = resp.on_hover_text(format!(
                            "{}\n双击打开所在目录,右键更多操作",
                            path.display()
                        ));
                        if resp.double_clicked() {
                            row_action = Some(RowAction::Reveal(path.clone()));
                        }
                        resp.context_menu(|ui| row_action = show_row_menu(&path, ui));
                    });
                });
            }
        });
    row_action
}

/// 右键菜单三项内容;返回被点击的动作(未点击为 None)
pub(crate) fn show_row_menu(path: &std::path::Path, ui: &mut egui::Ui) -> Option<RowAction> {
    let mut action = None;
    if ui.button("复制路径").clicked() {
        action = Some(RowAction::Copy(path.to_path_buf()));
        ui.close();
    }
    if ui.button("在资源管理器中打开").clicked() {
        action = Some(RowAction::Reveal(path.to_path_buf()));
        ui.close();
    }
    ui.separator();
    if ui
        .add(egui::Button::new(
            egui::RichText::new("删除到回收站").color(egui::Color32::LIGHT_RED),
        ))
        .clicked()
    {
        action = Some(RowAction::Delete(path.to_path_buf()));
        ui.close();
    }
    action
}
