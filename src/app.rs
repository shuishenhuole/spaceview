//! 编排层:应用状态机(空闲 / 扫描中 / 完成)+ 行为逻辑。
//! 渲染代码在 ui/ 目录各面板中,数据结构在 engine/ 目录中。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;

use crate::engine::scanner::{self, ScanEvent, ScanResult};
use crate::engine::tree::ROOT;
use crate::ui::{
    extension_panel, files_panel, status_bar, toolbar, tree_panel, treemap_panel, Filter,
    RowAction,
};

/// 中央面板的 Tab 页
#[derive(PartialEq, Clone, Copy)]
pub enum Tab {
    TopFiles,
    Extensions,
    Treemap,
}

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
    /// 中央面板当前 Tab
    tab: Tab,
    /// 大文件列表排序方式
    file_sort: files_panel::SortBy,
    /// Treemap 下钻路径(节点索引栈,首元素为 ROOT)
    drill: Vec<u32>,
    /// 大文件列表中勾选待删除的文件(以路径为键)
    selected: Vec<PathBuf>,
    /// 是否弹出删除确认框
    confirm_delete: bool,
    /// 搜索关键词(不区分大小写,匹配目录名/文件完整路径)
    filter_text: String,
    /// 大小阈值,仅显示不小于该值的条目;0=不限
    filter_min_bytes: u64,
    /// 大文件列表键盘光标所在行(↑↓ 移动,空格勾选)
    focus_row: Option<usize>,
    /// 最近一次操作反馈(如删除结果),显示在状态栏,约 5 秒后自动消失
    pub(crate) message: Option<(String, std::time::Instant)>,
    /// 深色主题开关(工具栏按钮切换,持久化到 eframe Storage)
    pub(crate) dark: bool,
    /// 已应用到 Context 的主题,变化时才 set_visuals,避免每帧重建
    applied_dark: Option<bool>,
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
            tab: Tab::TopFiles,
            file_sort: files_panel::SortBy::Size,
            drill: vec![ROOT],
            selected: Vec::new(),
            confirm_delete: false,
            filter_text: String::new(),
            filter_min_bytes: 0,
            focus_row: None,
            message: None,
            dark: true,
            applied_dark: None,
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

/// 大小阈值下拉的显示文案
fn size_threshold_label(v: u64) -> String {
    match v {
        0 => "不限大小".to_owned(),
        10_485_760 => "> 10MB".to_owned(),
        104_857_600 => "> 100MB".to_owned(),
        _ => "> 1GB".to_owned(),
    }
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
                    self.drill = vec![ROOT]; // 新结果重置下钻路径
                    self.focus_row = None;
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

    /// 将勾选文件删除到回收站,并同步回减目录树大小、TopN 与扩展名统计
    fn perform_delete(&mut self) {
        let AppState::Done(result) = &mut self.state else {
            return;
        };
        let result = result.as_mut();
        let mut ok = 0usize;
        let mut errs: Vec<String> = Vec::new();
        for path in std::mem::take(&mut self.selected) {
            match trash::delete(&path) {
                Ok(()) => {
                    if let Some(pos) = result.top_files.iter().position(|h| h.path == path) {
                        let (dir_idx, size) = {
                            let hit = &result.top_files[pos];
                            (hit.dir_idx, hit.size)
                        };
                        result.tree.subtract(dir_idx, size, 1);
                        let ext = path
                            .extension()
                            .map(|e| e.to_string_lossy().to_lowercase())
                            .unwrap_or_else(|| "(无扩展名)".to_string());
                        if let Some(st) = result.ext_stats.iter_mut().find(|e| e.ext == ext) {
                            st.count = st.count.saturating_sub(1);
                            st.bytes = st.bytes.saturating_sub(size);
                        }
                        result.top_files.remove(pos);
                        result.total_bytes = result.total_bytes.saturating_sub(size);
                        result.total_files = result.total_files.saturating_sub(1);
                    } else {
                        // 目录删除:按路径找到节点,子树清零并从树上摘除
                        let dir_idx = (0..result.tree.nodes.len() as u32)
                            .find(|&i| result.tree.dir_path(i) == path);
                        if let Some(idx) = dir_idx {
                            let (sz, fc) = (
                                result.tree.nodes[idx as usize].size,
                                result.tree.nodes[idx as usize].file_count,
                            );
                            result.tree.remove_subtree(idx);
                            result.total_bytes = result.total_bytes.saturating_sub(sz);
                            result.total_files = result.total_files.saturating_sub(fc);
                            // 被删目录下的文件若出现在 TopN,一并剔除
                            result.top_files.retain(|h| !h.path.starts_with(&path));
                        }
                    }
                    ok += 1;
                }
                Err(e) => errs.push(format!("{} — {e}", path.display())),
            }
        }
        self.message = Some((
            if errs.is_empty() {
                format!("已删除 {ok} 项到回收站")
            } else {
                format!("已删除 {ok} 项;{} 项失败:{}", errs.len(), errs[0])
            },
            std::time::Instant::now(),
        ));
        self.confirm_delete = false;
    }

    /// 处理工具栏返回的动作
    fn handle_toolbar_action(&mut self, action: toolbar::ToolbarAction) {        match action {
            toolbar::ToolbarAction::SelectDrive(i) => self.target = Target::Drive(i),
            toolbar::ToolbarAction::OpenFolderPicker => self.open_folder_picker(),
            toolbar::ToolbarAction::StartScan => self.start_scan(),
            toolbar::ToolbarAction::CancelScan => {
                if let AppState::Scanning { cancel, .. } = &self.state {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            toolbar::ToolbarAction::ToggleTheme => self.dark = !self.dark,
        }
    }

    /// 设置状态栏反馈消息(5 秒自动消失,见 logic)
    fn set_message(&mut self, msg: String) {
        self.message = Some((msg, std::time::Instant::now()));
    }

    /// 沿 parent 链重建下钻栈,把 Treemap 定位到指定目录并切换 Tab
    fn navigate_treemap(&mut self, idx: u32) {
        let stack: Vec<u32> = if let AppState::Done(result) = &self.state {
            let mut s = Vec::new();
            let mut cur = idx;
            loop {
                s.push(cur);
                match result.tree.nodes[cur as usize].parent {
                    Some(p) => cur = p,
                    None => break,
                }
            }
            s.reverse(); // 首元素为 ROOT
            s
        } else {
            return;
        };
        self.drill = stack;
        self.tab = Tab::Treemap;
    }

    /// 处理各面板右键菜单上报的动作
    fn handle_row_action(&mut self, action: RowAction, ctx: &egui::Context) {
        match action {
            RowAction::Copy(p) => {
                ctx.copy_text(p.display().to_string());
                self.set_message(format!("已复制路径:{}", p.display()));
            }
            RowAction::Reveal(p) => {
                match reveal_in_explorer(&p) {
                    Ok(()) => {}
                    Err(e) => self.set_message(format!("打开资源管理器失败:{e}")),
                }
            }
            RowAction::Delete(p) => {
                // 复用现有“勾选 → 确认对话框 → perform_delete”流程
                self.selected = vec![p];
                self.confirm_delete = true;
            }
        }
    }
}

/// 在 Windows 资源管理器中打开:path 为目录则打开自身,为文件则打开所在目录并选中
fn reveal_in_explorer(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let is_dir = std::fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false);
        let mut cmd = std::process::Command::new("explorer");
        if is_dir {
            cmd.arg(path.as_os_str());
        } else {
            cmd.arg(format!("/select,{}", path.display()));
        }
        // explorer.exe 成功时退出码也常为 1,spawn 即视为打开,不 wait 输出
        cmd.spawn().map(|_| ())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(std::io::Error::other("仅 Windows 支持打开资源管理器"))
    }
}

impl eframe::App for AnalyseApp {
    /// 每帧先 logic(改状态、收线程事件,不画 UI),再 ui(构建界面)
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 主题变化时才重建 Visuals
        if self.applied_dark != Some(self.dark) {
            ctx.set_visuals(if self.dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            self.applied_dark = Some(self.dark);
        }
        // 反馈消息 5 秒后自动消失
        if let Some((_, at)) = &self.message {
            if at.elapsed() > std::time::Duration::from_secs(5) {
                self.message = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
        }
        // Backspace 在 Treemap 视图返回上一层(输入框获得焦点时不拦截)
        if self.tab == Tab::Treemap
            && matches!(self.state, AppState::Done(_))
            && self.drill.len() > 1
            && ctx.memory(|m| m.focused().is_none())
            && ctx.input(|i| i.key_pressed(egui::Key::Backspace))
        {
            self.drill.pop();
        }
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
        // 每帧构建过滤条件,树面板与文件列表共用
        let filter = Filter {
            needle: self.filter_text.trim().to_lowercase(),
            min_bytes: self.filter_min_bytes,
        };
        // 扫描完成时:左侧目录树面板(动作与联动导航经 left_out 回传)
        let mut left_out = tree_panel::TreePanelOut {
            action: None,
            nav: None,
        };
        if let AppState::Done(result) = &self.state {
            egui::Panel::left("tree_panel")
                .default_size(420.0)
                .show(ui, |ui| {
                    left_out = tree_panel::show(ui, &result.tree, &filter);
                });
        }

        // 工具栏(渲染 → 动作 → 执行)
        let view = toolbar::ToolbarView {
            drives: &self.drives,
            target: &self.target,
            target_path: self.target_path(),
            scanning: self.is_scanning(),
            dark: self.dark,
        };
        let action = egui::Panel::top("toolbar")
            .show(ui, |ui| ui.horizontal(|ui| toolbar::show(&view, ui)).inner)
            .inner;
        if let Some(a) = action {
            self.handle_toolbar_action(a);
        }

        // 过滤栏:仅在有扫描结果时显示(搜索框 + 大小阈值下拉)
        if matches!(self.state, AppState::Done(_)) {
            egui::Panel::top("filter_bar").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("过滤:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter_text)
                            .hint_text("搜索目录/文件名")
                            .desired_width(220.0),
                    );
                    if !self.filter_text.is_empty() && ui.button("✕").clicked() {
                        self.filter_text.clear();
                    }
                    let mut min = self.filter_min_bytes;
                    egui::ComboBox::from_id_salt("size_threshold")
                        .selected_text(size_threshold_label(min))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut min, 0, "不限大小");
                            ui.selectable_value(&mut min, 10 * 1024 * 1024, "> 10MB");
                            ui.selectable_value(&mut min, 100 * 1024 * 1024, "> 100MB");
                            ui.selectable_value(&mut min, 1024 * 1024 * 1024, "> 1GB");
                        });
                    self.filter_min_bytes = min;
                });
            });
        }

        egui::Panel::bottom("status").show(ui, |ui| {
            status_bar::show(&self.state, &self.progress, ui);
        });
        // 中央面板:Tab 状态先在局部复制渲染,结束后写回,避免借用冲突
        let mut tab = self.tab;
        let mut file_sort = self.file_sort;
        let mut selected = self.selected.clone();
        let mut focus_row = self.focus_row;
        let mut drill = std::mem::take(&mut self.drill);
        if drill.is_empty() {
            drill.push(ROOT);
        }
        let mut panel_action: Option<RowAction> = None;
        egui::CentralPanel::default().show(ui, |ui| match &self.state {
            AppState::Scanning { .. } => {
                ui.centered_and_justified(|ui| ui.heading("正在扫描,请稍候…"));
            }
            AppState::Done(result) => {
                // Tab:大文件 TopN / 扩展名统计 / Treemap
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut tab, Tab::TopFiles, "大文件 TopN");
                    ui.selectable_value(&mut tab, Tab::Extensions, "按类型统计");
                    ui.selectable_value(&mut tab, Tab::Treemap, "Treemap");
                });
                ui.separator();
                match tab {
                    Tab::TopFiles => {
                        panel_action = files_panel::show(
                            result,
                            &mut file_sort,
                            &mut selected,
                            &mut focus_row,
                            &filter,
                            ui,
                        )
                    }
                    Tab::Extensions => extension_panel::show(result, ui),
                    Tab::Treemap => {
                        // 面包屑:点击任一层回退到该层
                        let mut crumb_pop: Option<usize> = None;
                        egui::ScrollArea::horizontal().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for (lvl, &idx) in drill.iter().enumerate() {
                                    let name = if idx == ROOT {
                                        result.tree.root_path.to_string_lossy().into_owned()
                                    } else {
                                        result.tree.nodes[idx as usize].name.clone()
                                    };
                                    let last = lvl + 1 == drill.len();
                                    if ui
                                        .add_enabled(!last, egui::Button::new(name))
                                        .clicked()
                                    {
                                        crumb_pop = Some(lvl);
                                    }
                                    if !last {
                                        ui.label("›");
                                    }
                                }
                            });
                        });
                        if let Some(lvl) = crumb_pop {
                            drill.truncate(lvl + 1);
                        }
                        let current = *drill.last().unwrap();
                        let node = &result.tree.nodes[current as usize];
                        ui.label(
                            egui::RichText::new(format!(
                                "当前视图:{} — {}",
                                node.name,
                                crate::engine::tree::human_size(node.size)
                            ))
                            .weak()
                            .small(),
                        );
                        let (clicked, tm_action) = treemap_panel::show(ui, &result.tree, current);
                        if let Some(idx) = clicked {
                            drill.push(idx);
                        }
                        panel_action = tm_action;
                    }
                }
            }
            AppState::Failed(_) => {
                ui.heading("磁盘空间分析工具");
            }
            _ => {
                ui.heading("磁盘空间分析工具");
                ui.label("选择扫描目标后点击“开始扫描”。");
            }
        });
        self.tab = tab;
        self.file_sort = file_sort;
        self.selected = selected;
        self.focus_row = focus_row;
        self.drill = drill;

        // 树面板单击导航:重建 drill 栈并切到 Treemap;随后消费右键菜单动作
        if let Some(idx) = left_out.nav {
            self.navigate_treemap(idx);
        }
        for a in [left_out.action, panel_action].into_iter().flatten() {
            self.handle_row_action(a, ui.ctx());
        }

        // 右下角浮动反馈气泡:最后 1 秒线性淡出,5 秒后由 logic() 清除
        if let Some((m, at)) = self.message.clone() {
            let alpha = ((5.0f32 - at.elapsed().as_secs_f32()) / 1.0).clamp(0.0, 1.0);
            let win_fill = ui.visuals().window_fill();
            egui::Area::new(egui::Id::new("toast"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -56.0])
                .show(ui.ctx(), |ui| {
                    egui::Frame::new()
                        .fill(win_fill.linear_multiply(0.1 + 0.85 * alpha))
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::LIGHT_GREEN.linear_multiply(alpha),
                        ))
                        .corner_radius(8.0)
                        .inner_margin(egui::Margin::same(10))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(m)
                                    .color(egui::Color32::LIGHT_GREEN.linear_multiply(alpha)),
                            );
                        });
                });
        }

        // Delete 键触发确认对话框(仅在有勾选时)
        if ui.input(|i| i.key_pressed(egui::Key::Delete)) && !self.selected.is_empty() {
            self.confirm_delete = true;
        }
        if self.confirm_delete {
            let n = self.selected.len();
            let list = self.selected.clone();
            let mut do_it = false;
            let mut cancel = false;
            let _ = egui::Window::new("确认删除到回收站")
                .id(egui::Id::new("del_confirm"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui, |ui| {
                    for p in list.iter().take(5) {
                        ui.label(egui::RichText::new(p.display().to_string()).small());
                    }
                    if n > 5 {
                        ui.label(egui::RichText::new(format!("… 共 {n} 项")).weak());
                    }
                    ui.separator();
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(
                            egui::RichText::new("删除到回收站").color(egui::Color32::LIGHT_RED),
                        ))
                        .clicked()
                        {
                            do_it = true;
                        }
                        if ui.button("取消").clicked() {
                            cancel = true;
                        }
                    });
                });
            if do_it {
                self.perform_delete();
            }
            if cancel {
                self.confirm_delete = false;
            }
        }
    }

    /// 周期性保存(eframe 自动调用):目前仅持久化主题选择
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("theme", if self.dark { "dark" } else { "light" }.to_owned());
    }
}
