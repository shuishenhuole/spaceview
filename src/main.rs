//! 程序入口:创建 eframe 窗口并注册中文字体。
#![windows_subsystem = "windows"] // release 构建时不弹出控制台黑窗

mod app;
mod engine;
mod i18n;
mod ui;

pub use app::AnalyseApp;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("analyse") // Storage 持久化需要的应用标识
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("磁盘空间分析"),
        ..Default::default()
    };
    eframe::run_native(
        "磁盘空间分析",
        options,
        Box::new(|cc| {
            // egui 默认字体不含 CJK,中文会显示为方块,启动时先注册系统字体
            setup_cjk_fonts(&cc.egui_ctx);
            let mut app = AnalyseApp::default();
            // 读取上次保存的主题与语言偏好
            if let Some(storage) = cc.storage {
                if let Some(saved) = storage.get_string("theme") {
                    app.dark = saved != "light";
                }
                i18n::set_en(storage.get_string("lang").as_deref() == Some("en"));
            }
            Ok(Box::new(app))
        }),
    )
}

/// 尝试加载 Windows 系统中文字体(微软雅黑优先,依次回退)
fn setup_cjk_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    const CANDIDATES: [&str; 3] = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\msyh.ttf",
        "C:\\Windows\\Fonts\\simhei.ttf",
    ];
    for path in CANDIDATES {
        if let Ok(data) = std::fs::read(path) {
            fonts.font_data.insert(
                "win_cjk".to_owned(),
                egui::FontData::from_owned(data).into(),
            );
            // 追加到比例与等宽字体族末尾,作为中文缺字的回退
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                if let Some(list) = fonts.families.get_mut(&family) {
                    list.push("win_cjk".to_owned());
                }
            }
            break;
        }
    }
    ctx.set_fonts(fonts);
}
