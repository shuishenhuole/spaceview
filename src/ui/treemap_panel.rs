//! Treemap 面板:渲染当前目录的子块,左键点击返回下钻目标,
//! 右键弹出菜单(复制路径 / 打开文件夹 / 删除到回收站)。
//!
//! 动画以稳定 marker(节点索引)匹配矩形:布局结果缓存,目标变化时从当前
//! 姿态补间到新目标;新块淡入、离场块短暂淡出,下钻/返回整层交叉淡入淡出。
//! 动画与布局状态存于 egui 临时数据,仅在动画未结束时请求重绘。

use std::collections::HashMap;

use crate::engine::tree::{Tree, human_size};
use crate::engine::treemap::squarify;
use crate::f;
use crate::i18n::b;
use crate::ui::RowAction;
use crate::ui::files_panel::show_row_menu;
use crate::ui::motion;

/// 超过该块数时跳过布局动画,直接落位,保证大量节点下交互流畅
const MAX_ANIMATED_BLOCKS: usize = 800;
/// 视图切换时新块的分批入场间隔
const STAGGER: f32 = 0.015;
/// 分批入场最大延迟
const STAGGER_MAX: f32 = 0.12;

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

/// 两个矩形是否几乎重合(避免亚像素抖动触发补间)
fn rects_close(a: egui::Rect, b: egui::Rect) -> bool {
    (a.min - b.min).length() < 0.25 && (a.max - b.max).length() < 0.25
}

/// 单个块的补间状态
#[derive(Clone)]
struct BlockAnim {
    marker: Option<u32>,
    from: egui::Rect,
    to: egui::Rect,
    alpha_from: f32,
    alpha_to: f32,
    start: f32,
    duration: f32,
    /// 离场幽灵块:只在底层淡出,不参与命中与文字绘制
    ghost: bool,
    /// 离场时保留颜色,避免引用已切换的视图数据
    fill: egui::Color32,
}

impl BlockAnim {
    fn t(&self, now: f32) -> f32 {
        motion::ease_out_cubic(motion::progress(now, self.start, self.duration))
    }

    fn rect(&self, now: f32) -> egui::Rect {
        motion::lerp_rect(self.from, self.to, self.t(now))
    }

    fn alpha(&self, now: f32) -> f32 {
        motion::lerp(self.alpha_from, self.alpha_to, self.t(now))
    }

    fn done(&self, now: f32) -> bool {
        now >= self.start + self.duration
    }
}

/// 把已有块转成淡出幽灵(固定当前位置,只降不透明度)
fn ghost_of(block: BlockAnim, now: f32, duration: f32) -> BlockAnim {
    let rect = block.rect(now);
    BlockAnim {
        marker: block.marker,
        from: rect,
        to: rect,
        alpha_from: block.alpha(now),
        alpha_to: 0.0,
        start: now,
        duration,
        ghost: true,
        fill: block.fill,
    }
}

/// Treemap 跨帧动画状态(marker → 补间)
#[derive(Clone)]
struct AnimState {
    epoch: u64,
    current: u32,
    blocks: Vec<BlockAnim>,
}

/// 布局缓存:数据与可用矩形都没变时跳过 squarify
#[derive(Clone)]
struct LayoutCache {
    rect: egui::Rect,
    items: Vec<(Option<u32>, u64)>,
    laid: Vec<(Option<u32>, egui::Rect)>,
}

/// 读取布局缓存,必要时重新计算(扫描快照约每秒更新一次,避免每帧重排)
fn cached_layout(
    ui: &egui::Ui,
    id: egui::Id,
    rect: egui::Rect,
    items: &[(Option<u32>, u64)],
) -> Vec<(Option<u32>, egui::Rect)> {
    let mut laid = None;
    ui.ctx().data_mut(|d| {
        if let Some(cache) = d.get_temp::<LayoutCache>(id)
            && cache.rect == rect
            && cache.items.as_slice() == items
        {
            laid = Some(cache.laid);
        }
        if laid.is_none() {
            let fresh = squarify(items, rect);
            d.insert_temp(
                id,
                LayoutCache {
                    rect,
                    items: items.to_vec(),
                    laid: fresh.clone(),
                },
            );
            laid = Some(fresh);
        }
    });
    laid.unwrap_or_default()
}

/// 明亮柔和的马卡龙调色板轮换;直属文件块用灰色
fn block_fill(marker: Option<u32>, is_del: bool, n: usize, dark_mode: bool) -> egui::Color32 {
    match marker {
        Some(_) if is_del => egui::Color32::from_gray(if dark_mode { 78 } else { 205 }),
        Some(_) => {
            const PALETTE: [[u8; 3]; 8] = [
                [176, 200, 226],
                [182, 216, 182],
                [235, 197, 170],
                [213, 180, 213],
                [170, 214, 214],
                [226, 200, 160],
                [190, 175, 226],
                [180, 214, 180],
            ];
            let c = PALETTE[n % PALETTE.len()];
            egui::Color32::from_rgb(c[0], c[1], c[2])
        }
        // faint_bg_color 是加法混合色,直接作不透明填充会在浅色模式发黑,
        // 这里按主题取显式中性灰
        None => egui::Color32::from_gray(if dark_mode { 62 } else { 215 }),
    }
}

/// 文字颜色按块底色亮度自适应,避免深浅主题下出现过浅/过深文字
fn block_text_color(fill: egui::Color32) -> egui::Color32 {
    let [r, g, b, _] = fill.to_normalized_gamma_f32();
    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if lum > 0.55 {
        egui::Color32::from_gray(25)
    } else {
        egui::Color32::from_gray(240)
    }
}

/// 渲染 current 目录的矩形树。返回 (被点击下钻的子目录索引, 右键菜单动作)。
/// `epoch` 为扫描会话版本:变化时丢弃旧动画,`reduce_motion` 关闭布局动效。
pub fn show(
    ui: &mut egui::Ui,
    tree: &Tree,
    current: u32,
    epoch: u64,
    reduce_motion: bool,
) -> (Option<u32>, Option<RowAction>) {
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
        ui.label(egui::RichText::new(b("空目录", "Empty folder")).weak());
        return (None, None);
    }

    let layout_id = ui.id().with("tm_layout");
    let anim_id = ui.id().with("tm_anim");
    let laid = cached_layout(ui, layout_id, rect, &items);
    let painter = ui.painter_at(
        // 稍放大避免裁剪边框
        rect.expand(1.0),
    );
    let now = ui.input(|i| i.time) as f32;

    // 节点过多或用户要求减少动画时:直接绘制最终布局,并清理旧动画状态
    let animate = !reduce_motion && laid.len() <= MAX_ANIMATED_BLOCKS;
    if !animate {
        ui.ctx().data_mut(|d| d.remove::<AnimState>(anim_id));
    }

    let mut clicked = None;
    let mut row_action: Option<RowAction> = None;
    let mut next_anims: Vec<BlockAnim> = Vec::with_capacity(laid.len());
    let mut ghosts: Vec<BlockAnim> = Vec::new();
    let mut moving = false;

    if animate {
        let prev = ui.ctx().data(|d| d.get_temp::<AnimState>(anim_id));
        let same_epoch = prev.as_ref().is_some_and(|s| s.epoch == epoch);
        let same_view = same_epoch && prev.as_ref().is_some_and(|s| s.current == current);
        // 同一次扫描内切换目录(下钻/返回)用更长的整层过渡;新扫描/首帧用入场时长
        let enter_time = if same_epoch {
            motion::DRILL
        } else {
            motion::ENTER
        };
        let mut prev_current: HashMap<Option<u32>, BlockAnim> = HashMap::new();

        if let Some(state) = prev {
            if same_view {
                for block in state.blocks {
                    if block.ghost {
                        if !block.done(now) {
                            ghosts.push(block);
                        }
                    } else {
                        prev_current.insert(block.marker, block);
                    }
                }
            } else if same_epoch {
                // 下钻/返回:旧视图整层淡出垫底,新视图分批淡入
                for block in state.blocks.into_iter().filter(|b| !b.ghost) {
                    let a = block.alpha(now);
                    if a > 0.01 {
                        ghosts.push(ghost_of(block, now, enter_time));
                    }
                }
            }
            // epoch 变化(重新扫描):旧结果动画直接丢弃
        }

        let fresh_view = !same_view;
        for (n, (marker, target)) in laid.iter().enumerate() {
            let anim = match prev_current.remove(marker) {
                Some(prev) if rects_close(prev.to, *target) => prev,
                Some(prev) => BlockAnim {
                    marker: *marker,
                    from: prev.rect(now),
                    to: *target,
                    alpha_from: prev.alpha(now),
                    alpha_to: 1.0,
                    start: now,
                    duration: motion::NORMAL,
                    ghost: false,
                    fill: prev.fill,
                },
                None => {
                    // 视图切换时按面积顺序轻微错峰,避免整屏同时闪烁
                    let delay = if fresh_view {
                        (n as f32 * STAGGER).min(STAGGER_MAX)
                    } else {
                        0.0
                    };
                    BlockAnim {
                        marker: *marker,
                        from: *target,
                        to: *target,
                        alpha_from: 0.0,
                        alpha_to: 1.0,
                        start: now + delay,
                        duration: if fresh_view {
                            enter_time
                        } else {
                            motion::ENTER
                        },
                        ghost: false,
                        fill: egui::Color32::TRANSPARENT,
                    }
                }
            };
            moving |= !anim.done(now);
            next_anims.push(anim);
        }
        // 本帧不再存在的块:短暂淡出,而不是瞬间消失
        for (_, block) in prev_current {
            let a = block.alpha(now);
            if a > 0.01 {
                ghosts.push(ghost_of(block, now, motion::FAST));
                moving = true;
            }
        }
        // 限制同时动画的块数;已完成的幽灵块不再保留
        ghosts.retain(|g| !g.done(now) && g.alpha(now) > 0.005);
        if ghosts.len() > MAX_ANIMATED_BLOCKS {
            ghosts.truncate(MAX_ANIMATED_BLOCKS);
        }
        moving |= !ghosts.is_empty();
    }

    // 离场幽灵块垫底绘制,不做文字与命中
    for ghost in &ghosts {
        let a = ghost.alpha(now);
        if a > 0.01 {
            painter.rect_filled(ghost.rect(now), 1.0, motion::with_alpha(ghost.fill, a));
        }
    }

    for (n, (marker, target)) in laid.iter().enumerate() {
        // 当前姿态:动画开启时读取补间,否则直接落位
        let (dr, da) = if animate {
            let anim = &next_anims[n];
            (anim.rect(now), anim.alpha(now))
        } else {
            (*target, 1.0)
        };
        if da < 0.02 {
            continue;
        }

        let marker_val = *marker;
        let is_del = marker_val.is_some_and(|idx| tree.nodes[idx as usize].deleted);
        let base_fill = block_fill(marker_val, is_del, n, ui.visuals().dark_mode);
        let text_col = block_text_color(base_fill);
        let (size, file_count) = match marker_val {
            Some(idx) => {
                let node = &tree.nodes[idx as usize];
                (node.size, node.file_count)
            }
            None => (own_files, tree.nodes[current as usize].file_count),
        };
        let show_text = dr.width() > 70.0 && dr.height() > 18.0;

        // 交互层:左键下钻 + 悬停详情 + 右键菜单
        // 灰色“本目录文件”块没有独立节点,菜单作用于当前目录本身
        let resp = ui.interact(
            *target,
            ui.id().with(("tm", marker_val)),
            egui::Sense::click(),
        );
        let hovered = resp.hovered();
        // 悬停时才构建文案/路径,避免大量小块的每帧字符串分配
        let resp2 = if hovered {
            let label = match marker_val {
                Some(idx) => {
                    let node = &tree.nodes[idx as usize];
                    if node.deleted {
                        format!("{} · {}", node.name, b("已删除", "deleted"))
                    } else {
                        node.name.clone()
                    }
                }
                None => b("本目录文件", "Files in this folder").to_owned(),
            };
            let menu_path = match marker_val {
                Some(idx) => tree.dir_path(idx),
                None => tree.dir_path(current),
            };
            let pct = size as f32 / cur_size.max(1) as f32 * 100.0;
            resp.on_hover_text(f!(
                "{label}\n{}\n{}  {}  · 占当前视图 {:.1}%",
                "{label}\n{}\n{}  {}  · {:.1}% of current view",
                menu_path.display(),
                human_size(size),
                f!("{} 个文件", "{} files", file_count),
                pct
            ))
        } else {
            resp
        };
        // 悬停:边框与填充色做短补间,不改变矩形面积
        let hover_t = ui.ctx().animate_bool_with_time_and_easing(
            ui.id().with(("tm_hover", marker_val)),
            resp2.hovered(),
            if reduce_motion {
                motion::REDUCED
            } else {
                motion::FAST
            },
            egui::emath::easing::cubic_out,
        );
        let fill = if hover_t > 0.01 {
            let tint = if ui.visuals().dark_mode {
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, (24.0 * hover_t) as u8)
            } else {
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, (16.0 * hover_t) as u8)
            };
            base_fill.blend(tint)
        } else {
            base_fill
        };
        // 记录基础色,供该块离场时作幽灵色
        if animate {
            next_anims[n].fill = base_fill;
        }

        painter.rect_filled(dr, 1.0, motion::with_alpha(fill, da));
        painter.rect_stroke(
            dr,
            1.0,
            egui::Stroke::new(1.0, motion::with_alpha(ui.visuals().window_fill(), da)),
            egui::StrokeKind::Inside,
        );
        // 已删除块:红色描边强调
        if is_del {
            painter.rect_stroke(
                dr,
                1.0,
                egui::Stroke::new(
                    1.5,
                    motion::with_alpha(egui::Color32::from_rgb(205, 92, 92), da),
                ),
                egui::StrokeKind::Inside,
            );
        }

        // 文字:仅在矩形够大时绘制;高度充足时分两行,空间不够则合并
        if show_text {
            let label = match marker_val {
                Some(idx) => {
                    let node = &tree.nodes[idx as usize];
                    if node.deleted {
                        format!("{} · {}", node.name, b("已删除", "deleted"))
                    } else {
                        node.name.clone()
                    }
                }
                None => b("本目录文件", "Files in this folder").to_owned(),
            };
            let size_str = human_size(size);
            let txt = motion::with_alpha(text_col, da);
            if dr.height() > 36.0 {
                let short = fit_width(&label, dr.width() - 8.0, 12.0);
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
                let line = fit_width(&format!("{label} · {size_str}"), dr.width() - 8.0, 12.0);
                painter.text(
                    dr.center(),
                    egui::Align2::CENTER_CENTER,
                    line,
                    egui::FontId::proportional(12.0),
                    txt,
                );
            }
        }

        if hover_t > 0.01 {
            painter.rect_stroke(
                dr,
                1.0,
                egui::Stroke::new(
                    2.0,
                    motion::with_alpha(ui.visuals().strong_text_color(), hover_t),
                ),
                egui::StrokeKind::Inside,
            );
        }

        if let Some(idx) = marker_val
            && resp2.clicked()
            && !is_del
        {
            clicked = Some(idx);
        }
        // 灰色块不提供删除(删目录会把直属文件一起删除,语义留给具体目录块);已删除块无菜单
        if marker_val.is_some() && !is_del {
            resp2.context_menu(|ui| {
                if let Some(idx) = marker_val {
                    let path = tree.dir_path(idx);
                    if let Some(a) = show_row_menu(&path, ui) {
                        row_action = Some(a);
                    }
                }
            });
        }
    }

    if animate {
        next_anims.extend(ghosts);
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                anim_id,
                AnimState {
                    epoch,
                    current,
                    blocks: next_anims,
                },
            );
        });
        if moving {
            ui.ctx().request_repaint();
        }
    }

    (clicked, row_action)
}
