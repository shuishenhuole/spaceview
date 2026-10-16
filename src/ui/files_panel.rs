//! 大文件 TopN 面板:可按大小/路径排序的列表(仅渲染前 1000 行防卡顿),
//! 行上右键可复制路径 / 打开所在目录 / 删除到回收站。

use std::path::PathBuf;

use crate::engine::scanner::{FileHit, ScanResult};
use crate::engine::tree::human_size;
use crate::f;
use crate::i18n::b;
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
    focus_row: &mut Option<usize>,
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
    }
    // 行数变化后光标越界收回
    if rows.is_empty() {
        *focus_row = None;
    } else if let Some(f) = *focus_row {
        *focus_row = Some(f.min(rows.len() - 1));
    }
    // 键盘导航:↑/↓ 移动光标,空格切换勾选,Ctrl/Cmd+A 全选/取消全选(有文本框等控件持有焦点时不拦截)
    if !rows.is_empty() && ui.ctx().memory(|m| m.focused().is_none()) {
        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            *focus_row = Some(focus_row.map_or(0, |f| (f + 1).min(rows.len() - 1)));
        }
        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            *focus_row = Some(focus_row.map_or(0, |f| f.saturating_sub(1)));
        }
        if ui.input(|i| i.key_pressed(egui::Key::Space))
            && let Some(f) = *focus_row
        {
            let p = rows[f].path.clone();
            if selected.contains(&p) {
                selected.retain(|x| x != &p);
            } else {
                selected.push(p);
            }
        }
        // 全选:当前可见行已全部勾选则清空,否则补全
        if ui.input(|i| i.key_pressed(egui::Key::A) && (i.modifiers.command || i.modifiers.ctrl)) {
            let visible: Vec<PathBuf> = rows.iter().map(|h| h.path.clone()).collect();
            let all_sel = visible.iter().all(|p| selected.contains(p));
            if all_sel {
                selected.retain(|p| !visible.contains(p));
            } else {
                for p in visible {
                    if !selected.contains(&p) {
                        selected.push(p);
                    }
                }
            }
        }
    }
    ui.horizontal(|ui| {
        if !selected.is_empty() {
            ui.label(
                egui::RichText::new(f!("已选 {} 项", "{} selected", selected.len()))
                .color(egui::Color32::LIGHT_YELLOW),
            );
        }
        ui.label(
            egui::RichText::new(f!(
                "(匹配 {} 条,渲染前 {} 条;↑↓ 移动光标,空格勾选,Ctrl/Cmd+A 全选,Delete 删除所选)",
                "(matched {}, rendering first {}; ↑↓ move, Space toggle, Ctrl/Cmd+A select all, Delete removes selected)",
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
            egui::RichText::new(b(
                "无匹配项,请调整搜索词或大小阈值",
                "No matches — adjust the search term or size threshold",
            ))
            .weak()
            .italics(),
        );
        return row_action;
    }
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // 可点击列头排序:当前列显示方向箭头;最前方为全选勾选框
            ui.horizontal(|ui| {
                let visible: Vec<PathBuf> = rows.iter().map(|h| h.path.clone()).collect();
                let mut all_sel =
                    !visible.is_empty() && visible.iter().all(|p| selected.contains(p));
                if ui
                    .checkbox(&mut all_sel, b("全选", "All"))
                    .on_hover_text(b("勾选/取消全部可见行", "Check / uncheck all visible rows"))
                    .changed()
                {
                    if all_sel {
                        for p in visible {
                            if !selected.contains(&p) {
                                selected.push(p);
                            }
                        }
                    } else {
                        selected.retain(|p| !rows.iter().any(|h| &h.path == p));
                    }
                }
                let size_lbl = if *sort_by == SortBy::Size {
                    format!("{} ▼", b("大小", "Size"))
                } else {
                    b("大小", "Size").to_owned()
                };
                if ui
                    .add_sized([110.0, 20.0], egui::Button::new(size_lbl))
                    .clicked()
                {
                    *sort_by = SortBy::Size;
                }
                let name_lbl = if *sort_by == SortBy::Name {
                    format!("{} ▲", b("文件(按路径)", "File (path)"))
                } else {
                    b("文件", "File").to_owned()
                };
                if ui.button(name_lbl).clicked() {
                    *sort_by = SortBy::Name;
                }
                ui.label(
                    egui::RichText::new(b(
                        "(单击移动光标,Shift+单击范围勾选,双击打开所在目录,右键更多操作)",
                        "(click to focus, Shift+click range-select, double-click opens folder, right-click for more)",
                    ))
                    .weak()
                    .small(),
                );
            });
            ui.separator();
            for (i, hit) in rows.iter().take(RENDER_LIMIT).enumerate() {
                let name = hit
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| hit.path.display().to_string());
                let is_sel = selected.contains(&hit.path);
                let is_focus = *focus_row == Some(i);
                // 选中行用主题选中色高亮;光标行用比斑马纹更深的灰,
                // 以背景色即可区分焦点,无需行首的 ▸ 指示符(易与勾选框混淆)
                // faint_bg_color 是加法混合色,直接当填充会在浅色模式发黑,改用显式灰
                let (zebra, focus_bg) = if !ui.visuals().dark_mode {
                    (238u8, 210u8)
                } else {
                    (40u8, 74u8)
                };
                let fill = if is_sel {
                    ui.visuals().selection.bg_fill
                } else if is_focus {
                    egui::Color32::from_gray(focus_bg)
                } else if i % 2 == 0 {
                    egui::Color32::from_gray(zebra)
                } else {
                    egui::Color32::TRANSPARENT
                };
                let frame = egui::Frame::new()
                    .fill(fill)
                    .inner_margin(egui::Margin::symmetric(4, 1));
                let path = hit.path.clone();
                frame.show(ui, |ui| {
                    // 整行交互背景:先注册使其位于子控件之下,
                    // 承载右键菜单 / 双击打开目录 / 单击移动光标
                    let row_rect = egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(ui.available_width(), 20.0),
                    );
                    let bg = ui.interact(
                        row_rect,
                        ui.id().with(("filerow", i)),
                        egui::Sense::click(),
                    );
                    ui.horizontal(|ui| {
                        // 勾选框:选中集以路径为键
                        let mut sel = is_sel;
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
                        ui.add(egui::Label::new(egui::RichText::new(name).strong()).selectable(false));
                    });
                    ui.advance_cursor_after_rect(row_rect);
                    let bg = bg.on_hover_text(f!(
                        "{}\n双击打开所在目录,右键更多操作",
                        "{}\nDouble-click to open folder, right-click for menu",
                        path.display()
                    ));
                    if is_focus {
                        bg.scroll_to_me(Some(egui::Align::Center));
                    }
                    if bg.double_clicked() {
                        row_action = Some(RowAction::Reveal(path.clone()));
                    }
                    if bg.clicked() {
                        // Shift+单击:从旧光标(锚点)到当前行的区间并入勾选集
                        if ui.input(|i| i.modifiers.shift) {
                            match *focus_row {
                                Some(a) => {
                                    for k in a.min(i)..=a.max(i) {
                                        let p = rows[k].path.clone();
                                        if !selected.contains(&p) {
                                            selected.push(p);
                                        }
                                    }
                                }
                                None => {
                                    if !selected.contains(&hit.path) {
                                        selected.push(hit.path.clone());
                                    }
                                }
                            }
                        }
                        *focus_row = Some(i);
                    }
                    bg.context_menu(|ui| row_action = show_row_menu(&path, ui));
                });
            }
        });
    row_action
}

/// 右键菜单三项内容;返回被点击的动作(未点击为 None)
pub(crate) fn show_row_menu(path: &std::path::Path, ui: &mut egui::Ui) -> Option<RowAction> {
    let mut action = None;
    if ui.button(b("复制路径", "Copy path")).clicked() {
        action = Some(RowAction::Copy(path.to_path_buf()));
        ui.close();
    }
    if ui
        .button(b("在资源管理器中打开", "Open in Explorer"))
        .clicked()
    {
        action = Some(RowAction::Reveal(path.to_path_buf()));
        ui.close();
    }
    ui.separator();
    if ui
        .add(egui::Button::new(
            egui::RichText::new(b("删除到回收站", "Delete to Recycle Bin"))
                .color(egui::Color32::LIGHT_RED),
        ))
        .clicked()
    {
        action = Some(RowAction::Delete(path.to_path_buf()));
        ui.close();
    }
    action
}
