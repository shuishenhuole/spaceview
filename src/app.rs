//! 应用主界面:持有全局状态机(空闲 / 扫描中 / 完成)。

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

/// 扫描目标来源:盘符 或 用户选择的任意文件夹
#[derive(Clone, PartialEq)]
pub enum Target {
    Drive(usize), //  drives[drive] 的下标
    Folder(PathBuf),
}

/// 顶层应用结构体
pub struct AnalyseApp {
    /// 本机枚举出的盘符,如 ["C:\\", "D:\\"]
    drives: Vec<PathBuf>,
    /// 当前选中的扫描目标
    target: Target,
    /// rfd 文件夹对话框在工作线程的结果回传通道(对话框会阻塞,不能在主线程弹)
    folder_rx: Option<Receiver<Option<PathBuf>>>,
}

impl Default for AnalyseApp {
    fn default() -> Self {
        let drives = list_drives();
        // 默认选中 C 盘;找不到则取第一个
        let target = Target::Drive(
            drives
                .iter()
                .position(|d| d.starts_with("C:"))
                .unwrap_or(0),
        );
        AnalyseApp {
            drives,
            target,
            folder_rx: None,
        }
    }
}

/// 枚举 A-Z 中实际存在的盘符
fn list_drives() -> Vec<PathBuf> {
    ('A'..='Z')
        .map(|c| PathBuf::from(format!("{c}:\\")))
        .filter(|p| p.is_dir())
        .collect()
}

impl AnalyseApp {
    /// 当前选择解析为实际扫描根路径
    fn target_path(&self) -> PathBuf {
        match &self.target {
            Target::Drive(i) => self.drives.get(*i).cloned().unwrap_or_else(|| PathBuf::from("C:\\")),
            Target::Folder(p) => p.clone(),
        }
    }

    /// 在工作线程打开系统文件夹选择对话框,避免阻塞 UI 重绘
    fn open_folder_picker(&mut self) {
        if self.folder_rx.is_some() {
            return; // 已有对话框未返回
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new().pick_folder();
            let _ = tx.send(picked);
        });
        self.folder_rx = Some(rx);
    }

    /// 工具栏:盘符下拉 + 浏览按钮 + 路径预览 + 扫描按钮
    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        let mut new_target: Option<Target> = None;

        // 盘符下拉
        let labels: Vec<String> = self.drives.iter().map(|d| d.to_string_lossy().into_owned()).collect();
        let mut combo_open = false;
        if let Target::Drive(i) = &self.target {
            let current = labels.get(*i).cloned().unwrap_or_default();
            egui::ComboBox::from_id_salt("drive_combo")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (idx, label) in labels.iter().enumerate() {
                        if ui.selectable_label(idx == *i, label).clicked() {
                            new_target = Some(Target::Drive(idx));
                            combo_open = true;
                        }
                    }
                });
            let _ = combo_open;
        } else {
            ui.label("自定义文件夹:");
        }

        ui.separator();

        if ui.button("浏览…").clicked() {
            self.open_folder_picker();
        }

        // 当前目标路径预览
        ui.label(
            egui::RichText::new(format!("扫描目标: {}", self.target_path().display()))
                .color(ui.visuals().weak_text_color()),
        );

        if ui.add_enabled(false, egui::Button::new("开始扫描")).clicked() {
            // 第 3 步接入扫描引擎
        }

        if let Some(t) = new_target {
            self.target = t;
        }
    }
}

impl eframe::App for AnalyseApp {
    /// 每帧先 logic(改状态、收线程事件,不画 UI),再 ui(构建界面)
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 轮询文件夹对话框结果
        if let Some(rx) = &self.folder_rx {
            match rx.try_recv() {
                Ok(picked) => {
                    if let Some(p) = picked {
                        self.target = Target::Folder(p);
                    }
                    self.folder_rx = None;
                    ctx.request_repaint();
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
                Err(TryRecvError::Disconnected) => {
                    self.folder_rx = None;
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| self.show_toolbar(ui));
        });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("磁盘空间分析工具");
            ui.label("骨架完成,扫描功能将在下一步接入。");
        });
    }
}
