//! 编排层:应用状态机(空闲 / 扫描中 / 完成)+ 行为逻辑。
//! 渲染代码在 ui/ 目录各面板中,数据结构在 engine/ 目录中。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;

use crate::engine::scanner::{self, ScanEvent, ScanResult};
use crate::ui::{status_bar, toolbar, tree_panel};

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
    pub(crate) drives: Vec<PathBuf>,
    /// 当前选中的扫描目标
    pub(crate) target: Target,
    /// rfd 文件夹对话框在工作线程的结果回传通道(对话框会阻塞,不能在主线程弹)
    folder_rx: Option<Receiver<Option<PathBuf>>>,
    /// 状态机
    pub(crate) state: AppState,
    /// 扫描进度快照(由 Tick 事件更新,状态栏渲染用)
    pub(crate) progress: status_bar::Progress,
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
            progress: status_bar::Progress {
                dirs: 0,
                files: 0,
                bytes: 0,
                cur: String::new(),
                started: None,
            },
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
    pub(crate) fn target_path(&self) -> PathBuf {
        match &self.target {
            Target::Drive(i) => self
                .drives
                .get(*i)
                .cloned()
                .unwrap_or_else(|| PathBuf::from("C:\\")),
            Target::Folder(p) => p.clone(),
        }
    }

    /// 是否正在扫描(扫描中禁用目标修改)
    fn is_scanning(&self) -> bool {
        matches!(self.state, AppState::Scanning { .. })
    }

    /// 启动后台扫描线程
    fn start_scan(&mut self) {
        let root = self.target_path();
        let cancel = Arc::new(AtomicBool::new(false));
        let rx = scanner::spawn_scan(root, cancel.clone());
        self.state = AppState::Scanning { rx, cancel };
        self.progress = status_bar::Progress {
            dirs: 0,
            files: 0,
            bytes: 0,
            cur: String::new(),
            started: Some(std::time::Instant::now()),
        };
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
                    self.progress.dirs = dirs;
                    self.progress.files = files;
                    self.progress.bytes = bytes;
                    self.progress.cur = cur;
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

    /// 处理工具栏返回的动作
    fn handle_toolbar_action(&mut self, action: toolbar::ToolbarAction) {
        match action {
            toolbar::ToolbarAction::SelectDrive(i) => self.target = Target::Drive(i),
            toolbar::ToolbarAction::OpenFolderPicker => self.open_folder_picker(),
            toolbar::ToolbarAction::StartScan => self.start_scan(),
            toolbar::ToolbarAction::CancelScan => {
                if let AppState::Scanning { cancel, .. } = &self.state {
                    cancel.store(true, Ordering::Relaxed);
                }
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
        // 扫描完成时:左侧目录树面板
        if let AppState::Done(result) = &self.state {
            egui::Panel::left("tree_panel")
                .default_size(420.0)
                .show(ui, |ui| tree_panel::show(ui, &result.tree));
        }

        // 工具栏(渲染 → 动作 → 执行)
        let view = toolbar::ToolbarView {
            drives: &self.drives,
            target: &self.target,
            target_path: self.target_path(),
            scanning: self.is_scanning(),
        };
        let action = egui::Panel::top("toolbar")
            .show(ui, |ui| ui.horizontal(|ui| toolbar::show(&view, ui)).inner)
            .inner;
        if let Some(a) = action {
            self.handle_toolbar_action(a);
        }

        egui::Panel::bottom("status").show(ui, |ui| {
            status_bar::show(&self.state, &self.progress, ui)
        });
        egui::CentralPanel::default().show(ui, |ui| match &self.state {
            AppState::Scanning { .. } => {
                ui.centered_and_justified(|ui| ui.heading("正在扫描,请稍候…"));
            }
            AppState::Done(_) => {
                ui.centered_and_justified(|ui| {
                    ui.label("扫描完成。大文件列表与 Treemap 将在后续步骤展示。")
                });
            }
            AppState::Failed(_) => {
                ui.heading("磁盘空间分析工具");
            }
            _ => {
                ui.heading("磁盘空间分析工具");
                ui.label("选择扫描目标后点击“开始扫描”。");
            }
        });
    }
}
