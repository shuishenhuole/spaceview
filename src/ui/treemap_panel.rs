//! Treemap 面板:渲染当前目录的子块,左键点击返回下钻目标,
//! 右键弹出菜单(复制路径 / 打开文件夹 / 删除到回收站)。

use crate::engine::tree::{human_size, Tree};
use crate::engine::treemap::squarify;
use crate::f;
use crate::i18n::b;
use crate::ui::files_panel::show_row_menu;
use crate::ui::RowAction;

/// 按像素宽度估算截断文本:中文≈1em、ASCII≈0.55em,超出时以省略号结尾
fn fit_width(s: &str, max_px: f32, font_px: f32) -> String {
    let mut w = 0.0f32;
    let mut out = String::new();
    for ch in s.chars() {
        let cw = if ch.is_ascii() {
            font_px * 0.55
        } else {
            font_px * 1.05
        };
        if w + cw > max_px {
            out.push('…');
            break;
        }
        w += cw;
        out.push(ch);
    }
    out
}

/// 矩形按 t 在 a→b 间插值(逐分量线性)
fn lerp_rect(a: egui::Rect, b: egui::Rect, t: f32) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(
            egui::lerp(a.min.x..=b.min.x, t),
            egui::lerp(a.min.y..=b.min.y, t),
        ),
        egui::pos2(
            egui::lerp(a.max.x..=b.max.x, t),
            egui::lerp(a.max.y..=b.max.y, t),
        ),
    )
}

/// 向中心收缩:f=0.8 表示宽高缩到 80%
fn shrink_to(r: egui::Rect, f: f32) -> egui::Rect {
    let c = r.center();
    egui::Rect::from_min_max(c + (r.min - c) * f, c + (r.max - c) * f)
}

/// 颜色乘以不透明度因子(用于淡入)
fn fade(c: egui::Color32, a: f32) -> egui::Color32 {
    let [r, g, b, _] = c.to_array();
    egui::Color32::from_rgba_unmultiplied(r, g, b, (a.clamp(0.0, 1.0) * 255.0) as u8)
}

/// 动画状态经 egui 临时数据跨帧保存(marker → 当前矩形/不透明度)
#[derive(Clone, Default)]
struct AnimRects(Vec<(Option<u32>, egui::Rect, f32)>);
#[derive(Clone, Default)]
struct AnimClock(Option<std::time::Instant>);

/// 渲染 current 目录的矩形树。返回 (被点击下钻的子目录索引, 右键菜单动作)。
pub fn show(ui: &mut egui::Ui, tree: &Tree, current: u32) -> (Option<u32>, Option<RowAction>) {
    let rect = {
        let (r, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        r
    };
    if !rect.is_positive() {
        return (None, None);
    }

    // 项 = 子目录 + 本目录直属文件(灰色块);已删除目录以固定小权重占位显示
    let cur_size = tree.nodes[current as usize].size;
    let tomb_weight = (cur_size / 40).max(1);
    let mut items: Vec<(Option<u32>, u64)> = tree.nodes[current as usize]
        .children
        .iter()
        .map(|&c| {
            let n = &tree.nodes[c as usize];
            (Some(c), if n.deleted { tomb_weight } else { n.size })
        })
        .collect();
    let children_sum: u64 = tree.nodes[current as usize]
        .children
        .iter()
        .map(|&c| tree.nodes[c as usize].size)
        .sum();
    let own_files = tree.nodes[current as usize]
        .size
        .saturating_sub(children_sum);
    if own_files > 0 {
        items.push((None, own_files));
    }
    if items.is_empty() {
        ui.label(
            egui::RichText::new(b("空目录", "Empty folder"))
                .weak(),
        );
        return (None, None);
    }

    let laid = squarify(&items, rect);
    let painter = ui.painter_at(
        // 稍放大避免裁剪边框
        rect.expand(1.0),
    );

    // 过渡动画:读取上一帧的块状态,指数收敛到本帧目标布局;
    // 新出现的块(首帧/下钻新目录)从 82% 尺寸淡入,已有块从旧矩形形变过来
    const SMOOTH: f32 = 16.0;
    let anim_id = ui.id().with("tm_anim");
    let prev: Vec<(Option<u32>, egui::Rect, f32)> = ui
        .ctx()
        .data(|d| d.get_temp::<AnimRects>(anim_id))
        .map(|v| v.0)
        .unwrap_or_default();
    let now = std::time::Instant::now();
    let dt = ui
        .ctx()
        .data(|d| d.get_temp::<AnimClock>(anim_id).and_then(|c| c.0))
        .and_then(|t| Some(t.elapsed().as_secs_f32()))
        .unwrap_or(0.016)
        .clamp(0.0, 0.05);
    let k = 1.0 - (-SMOOTH * dt).exp();
    let mut state_out: Vec<(Option<u32>, egui::Rect, f32)> = Vec::with_capacity(laid.len());
    let mut moving = false;

    let mut clicked = None;
    let mut row_action: Option<RowAction> = None;

    for (n, (marker, r)) in laid.iter().enumerate() {
        // 动画矩形与不透明度:按 marker 匹配旧状态
        let (anim_r, anim_a) = match prev.iter().find(|(m, _, _)| m == marker) {
            Some((_, r0, a0)) => (lerp_rect(*r0, *r, k), a0 + (1.0 - a0) * k),
            None => (
                lerp_rect(shrink_to(*r, 0.82), *r, k),
                k,
            ),
        };
        // 收敛后直接钉到目标值,避免永远在动
        let settled = (anim_r.min - r.min).length() < 0.5
            && (anim_r.max - r.max).length() < 0.5
            && anim_a > 0.995;
        let (dr, da) = if settled { (*r, 1.0) } else { (anim_r, anim_a) };
        if !settled {
            moving = true;
        }
        state_out.push((*marker, dr, da));
        if da < 0.02 {
            continue;
        }
        // 本次会话已删除到回收站的目录:占位显示,不可下钻/删除
        let is_del = marker.is_some_and(|idx| tree.nodes[idx as usize].deleted);
        let (label, size, count_tag) = match marker {
            Some(idx) => {
                let node = &tree.nodes[*idx as usize];
                let name = if node.deleted {
                    format!("{} · {}", node.name, b("已删除", "deleted"))
                } else {
                    node.name.clone()
                };
                (
                    name,
                    node.size,
                    f!("{} 个文件", "{} files", node.file_count),
                )
            }
            None => (
                b("本目录文件", "Files in this folder").to_owned(),
                own_files,
                f!(
                    "{} 个文件",
                    "{} files",
                    tree.nodes[current as usize].file_count
                ),
            ),
        };
        // 颜色:明亮柔和的马卡龙调色板轮换;直属文件块用灰色
        const PALETTE: [[u8; 3]; 8] = [
            [176, 200, 226], [182, 216, 182], [235, 197, 170], [213, 180, 213],
            [170, 214, 214], [226, 200, 160], [190, 175, 226], [180, 214, 180],
        ];
        let fill = match marker {
            Some(_) if is_del => egui::Color32::from_gray(if !ui.visuals().dark_mode {
                205
            } else {
                78
            }),
            Some(_) => {
                let c = PALETTE[n % PALETTE.len()];
                egui::Color32::from_rgb(c[0], c[1], c[2])
            }
            // 注意:faint_bg_color 是加法混合色(近黑),直接作不透明填充会在
            // 浅色模式下显示为深色块,这里按主题取显式中性灰
            None => egui::Color32::from_gray(if !ui.visuals().dark_mode {
                215
            } else {
                62
            }),
        };
        // 文字颜色按块底色亮度自适应:亮块用深色字、暗块用浅色字,
        // 避免固定 strong/text 色在深/浅主题下分别出现"过浅/过深"
        let [fr, fg, fb, _] = fill.to_normalized_gamma_f32();
        let lum = 0.2126 * fr + 0.7152 * fg + 0.0722 * fb;
        let text_col = if lum > 0.55 {
            egui::Color32::from_gray(25)
        } else {
            egui::Color32::from_gray(240)
        };
        painter.rect_filled(dr, 1.0, fade(fill, da));
        painter.rect_stroke(
            dr,
            1.0,
            egui::Stroke::new(1.0, fade(ui.visuals().window_fill(), da)),
            egui::StrokeKind::Inside,
        );
        // 已删除块:红色描边强调
        if is_del {
            painter.rect_stroke(
                dr,
                1.0,
                egui::Stroke::new(1.5, egui::Color32::from_rgb(205, 92, 92)),
                egui::StrokeKind::Inside,
            );
        }

        // 文字:仅在矩形够大时绘制;高度充足时名字与占用大小分两行,空间不够则合并为一行
        // 截断按像素宽度估算(中文≈1em、ASCII≈0.55em),避免中文名溢出
        if r.width() > 70.0 && r.height() > 18.0 {
            let size_str = human_size(size);
            let txt = fade(text_col, da);
            if r.height() > 36.0 {
                let short = fit_width(&label, r.width() - 8.0, 12.0);
                painter.text(
                    dr.center() - egui::vec2(0.0, 8.0),
                    egui::Align2::CENTER_CENTER,
                    &short,
                    egui::FontId::proportional(12.0),
                    txt,
                );
                painter.text(
                    dr.center() + egui::vec2(0.0, 9.0),
                    egui::Align2::CENTER_CENTER,
                    &size_str,
                    egui::FontId::proportional(11.0),
                    txt,
                );
            } else {
                let line = fit_width(
                    &format!("{label} · {size_str}"),
                    r.width() - 8.0,
                    12.0,
                );
                painter.text(
                    dr.center(),
                    egui::Align2::CENTER_CENTER,
                    line,
                    egui::FontId::proportional(12.0),
                    txt,
                );
            }
        }

        // 交互层:左键下钻 + 悬停详情 + 右键菜单
        // 灰色"本目录文件"块没有独立节点,菜单作用于当前目录本身
        let menu_path = match marker {
            Some(idx) => tree.dir_path(*idx),
            None => tree.dir_path(current),
        };
        let resp = ui.interact(*r, ui.id().with(("tm", n)), egui::Sense::click());
        if let Some(idx) = marker {
            if resp.clicked() && !is_del {
                clicked = Some(*idx);
            }
        }
        let pct = size as f32 / cur_size.max(1) as f32 * 100.0;
        let resp2 = resp.on_hover_text(f!(
            "{label}\n{}\n{}  {} · 占当前视图 {:.1}%",
            "{label}\n{}\n{}  {} · {:.1}% of current view",
            menu_path.display(),
            human_size(size),
            count_tag,
            pct
        ));
        // 悬停块加亮边框
        if resp2.hovered() {
            painter.rect_stroke(
                *r,
                1.0,
                egui::Stroke::new(2.0, ui.visuals().strong_text_color()),
                egui::StrokeKind::Inside,
            );
        }
        // 灰色块不提供删除(删目录会把直属文件一起删,语义留给具体目录块);已删除块无菜单
        if marker.is_some() && !is_del {
            resp2.context_menu(|ui| {
                if let Some(a) = show_row_menu(&menu_path, ui) {
                    row_action = Some(a);
                }
            });
        }
    }
    // 写回动画状态;若有块未收敛则请求下一帧重绘,动画自然停止
    ui.ctx().data_mut(|d| {
        d.insert_temp(anim_id, AnimRects(state_out));
        d.insert_temp(anim_id, AnimClock(Some(now)));
    });
    if moving {
        ui.ctx().request_repaint();
    }
    (clicked, row_action)
}
