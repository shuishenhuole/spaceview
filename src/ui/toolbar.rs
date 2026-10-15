//! 顶部工具栏:盘符下拉、浏览按钮、目标预览、开始/取消扫描。
//! 只负责渲染并返回用户动作,行为由 app.rs 执行。

use std::path::PathBuf;

use crate::app::Target;

/// 渲染所需的只读视图
pub struct ToolbarView<'a> {
    pub drives: &'a [PathBuf],
    pub target: &'a Target,
    /// 当前目标解析出的实际路径(用于预览)
    pub target_path: PathBuf,
    pub scanning: bool,
    /// 当前是否深色主题(决定按钮文案)
    pub dark: bool,
}

/// 用户点击产生的动作
pub enum ToolbarAction {
    SelectDrive(usize),
    OpenFolderPicker,
    StartScan,
    CancelScan,
    ToggleTheme,
}

pub fn show(view: &ToolbarView, ui: &mut egui::Ui) -> Option<ToolbarAction> {
    let mut action = None;

    // 盘符下拉 / 自定义文件夹标签(扫描中禁用)
    let labels: Vec<String> = view
        .drives
        .iter()
        .map(|d| d.to_string_lossy().into_owned())
        .collect();
    match view.target {
        Target::Drive(i) => {
            let current = labels.get(*i).cloned().unwrap_or_default();
            ui.add_enabled_ui(!view.scanning, |ui| {
                egui::ComboBox::from_id_salt("drive_combo")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (idx, label) in labels.iter().enumerate() {
                            if ui.selectable_label(idx == *i, label).clicked() {
                                action = Some(ToolbarAction::SelectDrive(idx));
                            }
                        }
                    });
            });
        }
        Target::Folder(_) => {
            ui.label("自定义文件夹");
        }
    }

    ui.separator();

    ui.add_enabled_ui(!view.scanning, |ui| {
        if ui.button("浏览…").clicked() {
            action = Some(ToolbarAction::OpenFolderPicker);
        }
    });

    // 当前目标路径预览
    ui.label(
        egui::RichText::new(format!("扫描目标: {}", view.target_path.display()))
            .color(ui.visuals().weak_text_color()),
    );

    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // 主题切换(不受扫描状态影响)
        let theme_label = if view.dark { "☀ 浅色" } else { "🌙 深色" };
        if ui.button(theme_label).clicked() {
            action = Some(ToolbarAction::ToggleTheme);
        }
        if view.scanning {
            if ui
                .add(egui::Button::new(
                    egui::RichText::new("取消").color(egui::Color32::LIGHT_RED),
                ))
                .clicked()
            {
                action = Some(ToolbarAction::CancelScan);
            }
            ui.spinner();
        } else if ui.add(egui::Button::new("开始扫描")).clicked() {
            action = Some(ToolbarAction::StartScan);
        }
    });

    action
}
