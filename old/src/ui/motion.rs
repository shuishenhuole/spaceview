//! 统一动效参数与工具函数。
//!
//! 所有面板共用同一套时长/曲线,避免各自动画节奏不一致;时间单位均为秒。
//! 动画一律按“当前时间 - 起始时间”求值,而不是逐帧累加,保证掉帧时进度依然正确。

/// 悬停等即时反馈
pub const FAST: f32 = 0.12;
/// 常规布局 / 状态变化
pub const NORMAL: f32 = 0.18;
/// 内容出现 / 切换
pub const ENTER: f32 = 0.24;
/// 下钻 / 返回整层视图切换
pub const DRILL: f32 = 0.32;
/// “减少动画”模式下的时长(接近即时,但保留一帧过渡避免突跳)
pub const REDUCED: f32 = 0.01;

/// 三次缓出:起步快、收尾慢,适合出现与位移动画
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// 线性插值
pub fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t.clamp(0.0, 1.0)
}

/// 时间进度:start 之前为 0,start + duration 之后为 1
pub fn progress(now: f32, start: f32, duration: f32) -> f32 {
    if duration <= 0.0 {
        1.0
    } else {
        ((now - start) / duration).clamp(0.0, 1.0)
    }
}

/// 矩形逐分量线性插值
pub fn lerp_rect(a: egui::Rect, b: egui::Rect, t: f32) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(lerp(a.min.x, b.min.x, t), lerp(a.min.y, b.min.y, t)),
        egui::pos2(lerp(a.max.x, b.max.x, t), lerp(a.max.y, b.max.y, t)),
    )
}

/// 颜色乘以不透明度因子
pub fn with_alpha(color: egui::Color32, alpha: f32) -> egui::Color32 {
    let [r, g, b, _] = color.to_array();
    egui::Color32::from_rgba_unmultiplied(r, g, b, (alpha.clamp(0.0, 1.0) * 255.0) as u8)
}
