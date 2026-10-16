//! 程序入口:创建 eframe 窗口并注册中文字体。
#![windows_subsystem = "windows"] // release 构建时不弹出控制台黑窗

mod app;
mod engine;
mod i18n;
mod ui;

pub use app::AnalyseApp;

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_app_id("analyse") // Storage 持久化需要的应用标识
        .with_inner_size([1100.0, 720.0])
        .with_min_inner_size([800.0, 500.0])
        .with_title("磁盘空间分析");
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
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
            // 命令行传入目录路径时作为初始扫描目标(各平台通用)
            if let Some(arg) = std::env::args().nth(1) {
                let p = std::path::PathBuf::from(&arg);
                if p.is_dir() {
                    app.target = app::Target::Folder(p);
                }
            }
            Ok(Box::new(app))
        }),
    )
}

/// 解码内置 PNG(src/assist/icon.png)并盒式降采样到 128×128,作为窗口/任务栏图标
fn window_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../src/assist/icon.png");
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes)).read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    // 统一转 RGBA8
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut v = Vec::with_capacity(w * h * 4);
            for px in buf.chunks_exact(3) {
                v.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
            v
        }
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        _ => return None,
    };
    const TW: usize = 128;
    const TH: usize = 128;
    let mut out = vec![0u8; TW * TH * 4];
    for oy in 0..TH {
        for ox in 0..TW {
            let x0 = ox * w / TW;
            let x1 = ((ox + 1) * w / TW).max(x0 + 1);
            let y0 = oy * h / TH;
            let y1 = ((oy + 1) * h / TH).max(y0 + 1);
            let (mut acc, mut n) = ([0u32; 4], 0u32);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let i = (sy * w + sx) * 4;
                    for c in 0..4 {
                        acc[c] += rgba[i + c] as u32;
                    }
                    n += 1;
                }
            }
            let o = (oy * TW + ox) * 4;
            for c in 0..4 {
                out[o + c] = (acc[c] / n) as u8;
            }
        }
    }
    Some(egui::IconData { rgba: out, width: TW as u32, height: TH as u32 })
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
