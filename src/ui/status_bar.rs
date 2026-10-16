//! 底部状态栏:扫描进度、完成摘要或失败信息。

use crate::app::AppState;
use crate::engine::tree::human_size;
use crate::f;
use crate::i18n::b;

/// 进度快照(app.rs 在 Tick 事件中更新)
pub struct Progress {
    pub dirs: u64,
    pub files: u64,
    pub bytes: u64,
    pub cur: String,
    pub started: Option<std::time::Instant>,
    /// 历史累计暂停时长(秒),从耗时统计中扣除
    pub paused_secs: f32,
    /// 本次暂停起点,未暂停时为 None
    pub paused_at: Option<std::time::Instant>,
}

/// 有效扫描耗时:总流逝时间减去全部暂停区间
fn active_elapsed(progress: &Progress) -> f32 {
    let Some(t) = progress.started else { return 0.0 };
    let mut e = t.elapsed().as_secs_f32() - progress.paused_secs;
    if let Some(p) = progress.paused_at {
        e -= p.elapsed().as_secs_f32();
    }
    e.max(0.0)
}

pub fn show(state: &AppState, progress: &Progress, ui: &mut egui::Ui) {
    match state {
        AppState::Idle => {
            ui.label(b(
                "就绪:选择存储位置或文件夹后点击“开始扫描”。",
                "Ready: pick a location or folder, then click \u{201c}Start Scan\u{201d}.",
            ));
        }
        AppState::Scanning { paused, .. } => {
            // 有效耗时扣除暂停区间,暂停中数字保持不动
            let elapsed = active_elapsed(progress);
            ui.horizontal(|ui| {
                if paused.load(std::sync::atomic::Ordering::Relaxed) {
                    // 暂停中不转圈,提示可继续
                    ui.label(b("⏸ 已暂停", "⏸ Paused"));
                } else {
                    ui.spinner();
                }
                // 附扫描速度,观感上替代无法预知的总进度
                let secs = elapsed.max(0.5) as f64;
                ui.label(f!(
                    "已扫描 {} 个目录 / {} 个文件,共 {} — {:.1}s · {:.0} 文件/秒 · {:.1} MB/秒",
                    "Scanned {} dirs / {} files, {} total — {:.1}s · {:.0} files/s · {:.1} MB/s",
                    progress.dirs,
                    progress.files,
                    human_size(progress.bytes),
                    elapsed,
                    progress.files as f64 / secs,
                    progress.bytes as f64 / secs / 1_048_576.0
                ));
            });
            ui.label(
                egui::RichText::new(f!(
                    "当前: {}",
                    "Current: {}",
                    progress.cur
                ))
                .small()
                .color(ui.visuals().weak_text_color()),
            );
        }
        AppState::Done(result) => {
            ui.label(f!(
                "扫描完成:{} 个文件,合计 {}。受限目录 {} 项(已跳过)。",
                "Scan finished: {} files, {} total. {} restricted directories skipped.",
                result.total_files,
                human_size(result.total_bytes),
                result.denied.len(),
            ));
        }
        AppState::Failed(err) => {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                f!("扫描失败:{}", "Scan failed: {}", err),
            );
        }
    }
}
