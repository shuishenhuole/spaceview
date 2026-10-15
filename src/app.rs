//! 应用主界面:状态机(空闲 / 扫描中 / 完成)+ 工具栏 + 扫描进度。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use std::time::Instant;

use crate::scanner::{self, ScanEvent, ScanResult};
use crate::tree::human_size;

/// 扫描目标来源:盘符 或 用户选择的任意文件夹
#[derive(Clone, PartialEq)]
pub enum Target {
    Drive(usize), // drives[drive] 的下标
    Folder(PathBuf),
}

/// 应用状态机
pub enum AppState {
    /// 未开始扫描
    Idle,
    /// 后台线程扫描中,持有事件接收端与取消标志
    Scanning {
        rx: Receiver<ScanEvent>,
        cancel: Arc<AtomicBool>,
    },
    /// 扫描完成,结果可用
    Done(Box<ScanResult>),
    /// 扫描失败(路径无效等)
    Failed(String),
}

/// 顶层应用结构体
pub struct AnalyseApp {
    /// 本机枚举出的盘符,如 ["C:\\", "D:\\"]
    drives: Vec<PathBuf>,
    /// 当前选中的扫描目标
    target: Target,
    /// rfd 文件夹对话框在工作线程的结果回传通道(对话框会阻塞,不能在主线程弹)
    folder_rx: Option<Receiver<Option<PathBuf>>>,
    /// 状态机
    state: AppState,
    /// 扫描进度快照(由 Tick 事件更新)
    tick_dirs: u64,
    tick_files: u64,
    tick_bytes: u64,
    tick_cur: String,
    scan_started: Option<Instant>,
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
            state: AppState::Idle,
            tick_dirs: 0,
            tick_files: 0,
            tick_bytes: 0,
            tick_cur: String::new(),
            scan_started: None,
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
            Target::Drive(i) => self
                .drives
                .get(*i)
                .cloned()
                .unwrap_or_else(|| PathBuf::from("C:\\")),
            Target::Folder(p) => p.clone(),
        }
    }

    /// 是否正在扫描(扫描中禁用重复开始/取消以外的操作)
    fn is_scanning(&self) -> bool {
        matches!(self.state, AppState::Scanning { .. })
    }

    /// 启动后台扫描线程
    fn start_scan(&mut self) {
        let root = self.target_path();
        let cancel = Arc::new(AtomicBool::new(false));
        let rx = scanner::spawn_scan(root, cancel.clone());
        self.state = AppState::Scanning { rx, cancel };
        self.tick_dirs = 0;
        self.tick_files = 0;
        self.tick_bytes = 0;
        self.tick_cur = String::new();
        self.scan_started = Some(Instant::now());
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

    /// 消费扫描线程事件:更新进度 / 完成 / 取消 / 失败
    fn poll_scan_events(&mut self, ctx: &egui::Context) {
        // 取出 rx(避免借用 self.state 与 &mut self 冲突)
        let taken = std::mem::replace(&mut self.state, AppState::Idle);
        let AppState::Scanning { rx, cancel } = taken else {
            self.state = taken;
            return;
        };
        loop {
            match rx.try_recv() {
                Ok(ScanEvent::Tick {
                    dirs,
                    files,
                    bytes,
                    cur,
                }) => {
                    self.tick_dirs = dirs;
                    self.tick_files = files;
                    self.tick_bytes = bytes;
                    self.tick_cur = cur;
                }
                Ok(ScanEvent::Done(result)) => {
                    self.state = AppState::Done(result);
                    return;
                }
                Ok(ScanEvent::Cancelled) => {
                    self.state = AppState::Idle;
                    ctx.request_repaint();
                    return;
                }
                Ok(ScanEvent::Failed(err)) => {
                    self.state = AppState::Failed(err);
                    return;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.state = AppState::Failed("扫描线程意外退出".into());
                    return;
                }
            }
        }
        // 仍在扫描:保持低频重绘以刷新进度
        self.state = AppState::Scanning { rx, cancel };
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }

    /// 工具栏:盘符下拉 + 浏览 + 目标预览 + 开始/取消扫描
    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        let scanning = self.is_scanning();
        let mut new_target: Option<Target> = None;
        let mut do_scan = false;
        let mut do_cancel = false;

        // 盘符下拉(扫描中禁用)
        let labels: Vec<String> = self
            .drives
            .iter()
            .map(|d| d.to_string_lossy().into_owned())
            .collect();
        if let Target::Drive(i) = &self.target {
            let current = labels.get(*i).cloned().unwrap_or_default();
            ui.add_enabled_ui(!scanning, |ui| {
                egui::ComboBox::from_id_salt("drive_combo")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (idx, label) in labels.iter().enumerate() {
                            if ui.selectable_label(idx == *i, label).clicked() {
                                new_target = Some(Target::Drive(idx));
                            }
                        }
                    });
            });
        } else {
            ui.label("自定义文件夹");
        }

        ui.separator();

        ui.add_enabled_ui(!scanning, |ui| {
            if ui.button("浏览…").clicked() {
                self.open_folder_picker();
            }
        });

        // 当前目标路径预览
        ui.label(
            egui::RichText::new(format!("扫描目标: {}", self.target_path().display()))
                .color(ui.visuals().weak_text_color()),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if scanning {
                if ui.add(egui::Button::new(egui::RichText::new("取消").color(egui::Color32::LIGHT_RED))).clicked() {
                    do_cancel = true;
                }
                ui.spinner();
            } else if ui.add(egui::Button::new("开始扫描")).clicked() {
                do_scan = true;
            }
        });

        if let Some(t) = new_target {
            self.target = t;
        }
        if do_scan {
            self.start_scan();
        }
        if do_cancel {
            if let AppState::Scanning { cancel, .. } = &self.state {
                cancel.store(true, Ordering::Relaxed);
            }
        }
    }

    /// 底部状态栏:扫描进度或结果摘要
    fn show_status_bar(&self, ui: &mut egui::Ui) {
        match &self.state {
            AppState::Idle => {
                ui.label("就绪:选择盘符或文件夹后点击“开始扫描”。");
            }
            AppState::Scanning { .. } => {
                let elapsed = self
                    .scan_started
                    .map(|t| t.elapsed().as_secs_f32())
                    .unwrap_or(0.0);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!(
                        "已扫描 {} 个目录 / {} 个文件,共 {} — {:?}s",
                        self.tick_dirs,
                        self.tick_files,
                        human_size(self.tick_bytes),
                        elapsed
                    ));
                });
                ui.label(
                    egui::RichText::new(format!("当前: {}", self.tick_cur))
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
            }
            AppState::Done(result) => {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "扫描完成:{} 个文件,合计 {}。受限目录 {} 项(已跳过)。",
                        result.total_files,
                        human_size(result.tree.nodes[crate::tree::ROOT as usize].size),
                        result.denied.len(),
                    ));
                    if result.denied.len() >= 200 {
                        ui.label("(样本上限 200)");
                    }
                });
            }
            AppState::Failed(err) => {
                ui.colored_label(egui::Color32::LIGHT_RED, format!("扫描失败:{err}"));
            }
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
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100))
                }
                Err(TryRecvError::Disconnected) => {
                    self.folder_rx = None;
                }
            }
        }
        // 轮询扫描事件
        if self.is_scanning() {
            self.poll_scan_events(ctx);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| self.show_toolbar(ui));
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            self.show_status_bar(ui);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            match &self.state {
                AppState::Scanning { .. } => {
                    ui.centered_and_justified(|ui| ui.heading("正在扫描,请稍候…"));
                }
                AppState::Done(_) => {
                    ui.centered_and_justified(|ui| {
                        ui.label("扫描完成。树形视图与 Treemap 将在后续步骤展示。")
                    });
                }
                _ => {
                    ui.heading("磁盘空间分析工具");
                    ui.label("选择扫描目标后点击“开始扫描”。");
                }
            }
        });
    }
}
