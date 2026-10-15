//! 应用主界面:持有全局状态机(空闲 / 扫描中 / 完成)。

/// 顶层应用结构体,后续步骤在此挂载扫描结果与 UI 状态
#[derive(Default)]
pub struct AnalyseApp {}

impl eframe::App for AnalyseApp {
    /// eframe 0.36 新 API:每帧先 logic(可改状态、收发事件,不允许画 UI),再 ui(构建界面)
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}

    /// 根 viewport 的 UI 回调,传入的 Ui 无 margin / 背景,需自行包 CentralPanel
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("磁盘空间分析工具");
            ui.label("初始化完成,等待功能接入。");
        });
    }
}
