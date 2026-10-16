//! 顶部工具栏:存储位置下拉、浏览按钮、目标预览、开始/取消扫描。
//! 只负责渲染并返回用户动作,行为由 app.rs 执行。

use std::path::PathBuf;

use crate::app::Target;
use crate::i18n::{b, is_en};

/// 渲染所需的只读视图
pub struct ToolbarView<'a> {
    pub drives: &'a [PathBuf],
    pub target: &'a Target,
    /// 当前目标解析出的实际路径(用于预览)
    pub target_path: PathBuf,
    pub scanning: bool,
    /// 扫描是否处于暂停中(决定暂停/继续按钮文案)
    pub paused: bool,
    /// 当前是否深色主题(决定按钮文案)
    pub dark: bool,
}

/// 用户点击产生的动作
pub enum ToolbarAction {
    SelectDrive(usize),
    OpenFolderPicker,
    StartScan,
    CancelScan,
    TogglePause,
    ToggleTheme,
    ToggleLang,
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
            ui.label(b("自定义文件夹", "Custom folder"));
        }
    }

    ui.separator();

    ui.add_enabled_ui(!view.scanning, |ui| {
        if ui.button(b("浏览…", "Browse…")).clicked() {
            action = Some(ToolbarAction::OpenFolderPicker);
        }
    });

    // 当前目标路径预览
    ui.label(
        egui::RichText::new(crate::f!(
            "扫描目标: {}",
            "Target: {}",
            view.target_path.display()
        ))
        .color(ui.visuals().weak_text_color()),
    );

    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        // 语言切换:按钮显示"目标语言"
        if ui.button(if is_en() { "中文" } else { "EN" }).clicked() {
            action = Some(ToolbarAction::ToggleLang);
        }
        // 主题切换(不受扫描状态影响)
        let theme_label = if view.dark {
            b("☀ 浅色", "☀ Light")
        } else {
            b("🌙 深色", "🌙 Dark")
        };
        if ui.button(theme_label).clicked() {
            action = Some(ToolbarAction::ToggleTheme);
        }
        if view.scanning {
            if ui
                .add(egui::Button::new(
                    egui::RichText::new(b("取消", "Cancel")).color(egui::Color32::LIGHT_RED),
                ))
                .clicked()
            {
                action = Some(ToolbarAction::CancelScan);
            }
            // 暂停/继续:挂起扫描线程消费,已扫描结果仍可查看
            if ui
                .button(if view.paused {
                    b("继续", "Resume")
                } else {
                    b("暂停", "Pause")
                })
                .clicked()
            {
                action = Some(ToolbarAction::TogglePause);
            }
            // 暂停时静态图标替代转圈,避免视觉上仍在运行
            if view.paused {
                ui.label("⏸");
            } else {
                ui.spinner();
            }
        } else if ui
            .add(egui::Button::new(b("开始扫描", "Start Scan")))
            .clicked()
        {
            action = Some(ToolbarAction::StartScan);
        }
    });

    action
}
