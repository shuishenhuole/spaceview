//! 底部状态栏:扫描进度、完成摘要或失败信息。

use crate::app::AppState;
use crate::engine::tree::{human_size, ROOT};

/// 进度快照(app.rs 在 Tick 事件中更新)
pub struct Progress {
    pub dirs: u64,
    pub files: u64,
    pub bytes: u64,
    pub cur: String,
    pub started: Option<std::time::Instant>,
}

pub fn show(state: &AppState, progress: &Progress, ui: &mut egui::Ui) {
    match state {
        AppState::Idle => {
            ui.label("就绪:选择盘符或文件夹后点击“开始扫描”。");
        }
        AppState::Scanning { .. } => {
            let elapsed = progress.started.map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!(
                    "已扫描 {} 个目录 / {} 个文件,共 {} — {:.1}s",
                    progress.dirs,
                    progress.files,
                    human_size(progress.bytes),
                    elapsed
                ));
            });
            ui.label(
                egui::RichText::new(format!("当前: {}", progress.cur))
                    .small()
                    .color(ui.visuals().weak_text_color()),
            );
        }
        AppState::Done(result) => {
            ui.label(format!(
                "扫描完成:{} 个文件,合计 {}。受限目录 {} 项(已跳过)。",
                result.total_files,
                human_size(result.tree.nodes[ROOT as usize].size),
                result.denied.len(),
            ));
        }
        AppState::Failed(err) => {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("扫描失败:{err}"));
        }
    }
}
