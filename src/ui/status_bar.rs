//! 底部状态栏:扫描进度、完成摘要或失败信息。

use crate::app::AppState;
use crate::engine::tree::human_size;

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
        AppState::Scanning { paused, .. } => {
            let elapsed = progress.started.map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0);
            ui.horizontal(|ui| {
                if paused.load(std::sync::atomic::Ordering::Relaxed) {
                    // 暂停中不转圈,提示可继续
                    ui.label("⏸ 已暂停");
                } else {
                    ui.spinner();
                }
                // 附扫描速度,观感上替代无法预知的总进度
                let secs = elapsed.max(0.5) as f64;
                ui.label(format!(
                    "已扫描 {} 个目录 / {} 个文件,共 {} — {:.1}s · {:.0} 文件/秒 · {:.1} MB/秒",
                    progress.dirs,
                    progress.files,
                    human_size(progress.bytes),
                    elapsed,
                    progress.files as f64 / secs,
                    progress.bytes as f64 / secs / 1_048_576.0
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
                human_size(result.total_bytes),
                result.denied.len(),
            ));
        }
        AppState::Failed(err) => {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("扫描失败:{err}"));
        }
    }
}
