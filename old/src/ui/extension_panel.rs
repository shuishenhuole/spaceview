//! 扩展名统计面板:按占用字节排序的占比条(前 40 类)。

use crate::engine::scanner::ScanResult;
use crate::engine::tree::human_size;
use crate::f;
use crate::i18n::b;

const ROW_LIMIT: usize = 40;

pub fn show(result: &ScanResult, ui: &mut egui::Ui) {
    let root_size = result.tree.nodes[crate::engine::tree::ROOT as usize].size.max(1);
    ui.label(
        egui::RichText::new(b(
            "按扩展名占用排序(相对扫描到的文件总大小)",
            "Sorted by extension usage (vs total scanned bytes)",
        ))
        .weak()
        .small(),
    );
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for stat in result.ext_stats.iter().take(ROW_LIMIT) {
                let pct = stat.bytes as f32 / root_size as f32;
                // "(无扩展名)" 是内部统计键,展示时翻译
                let ext_disp = if stat.ext == "(无扩展名)" {
                    b("(无扩展名)", "(no ext)").to_owned()
                } else {
                    format!(".{}", stat.ext.trim_start_matches('.'))
                };
                ui.add(
                    egui::ProgressBar::new(pct)
                        .text(f!(
                            "{}  {}  {} 个文件  {:.1}%",
                            "{}  {}  {} files  {:.1}%",
                            ext_disp,
                            human_size(stat.bytes),
                            stat.count,
                            pct * 100.0
                        ))
                        .desired_width(ui.available_width().min(420.0)),
                );
            }
            if result.ext_stats.len() > ROW_LIMIT {
                ui.label(
                    egui::RichText::new(f!(
                        "… 其余 {} 种扩展名合计占比不足尾部",
                        "… {} more extensions share the remaining tail",
                        result.ext_stats.len() - ROW_LIMIT
                    ))
                    .weak(),
                );
            }
        });
}
