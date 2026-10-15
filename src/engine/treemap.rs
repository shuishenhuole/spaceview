//! Squarified Treemap 布局算法(纯计算,无 UI 依赖)。
//! 参考 Bruls, Huizing, van Wijk "Squarified Treemaps"。

use egui::{Pos2, Rect, Vec2};

/// 对 items(节点标识 + 权重)在 rect 内做 squarified 布局,返回各自的矩形。
/// 权重 <= 0 的项被丢弃;若总权重为 0 返回空。
pub fn squarify<T: Clone>(items: &[(T, u64)], rect: Rect) -> Vec<(T, Rect)> {
    let mut items: Vec<(T, u64)> = items.iter().filter(|(_, s)| *s > 0).cloned().collect();
    if items.is_empty() {
        return Vec::new();
    }
    // 降序排列是大矩形优先成行的关键
    items.sort_by(|a, b| b.1.cmp(&a.1));
    let mut out = Vec::with_capacity(items.len());
    layout(&items, rect, &mut out);
    out
}

fn layout<T: Clone>(items: &[(T, u64)], rect: Rect, out: &mut Vec<(T, Rect)>) {
    if items.is_empty() || rect.width() <= 0.5 || rect.height() <= 0.5 {
        // 剩余空间太小,整块给最后一个,避免零碎矩形
        if let Some((t, _)) = items.first() {
            out.push((t.clone(), rect));
        }
        return;
    }
    let total: u64 = items.iter().map(|(_, s)| *s).sum();
    if total == 0 {
        return;
    }
    // 每字节对应的像素面积
    let px2_per_byte = (rect.width() as f64 * rect.height() as f64) / total as f64;
    // 短边:当前行将沿短边铺开
    let horizontal = rect.height() <= rect.width(); // true=竖条沿左侧、高度方向铺开
    let shorter = if horizontal { rect.height() } else { rect.width() } as f64;

    // 贪心成行:逐行加入项,直到最差纵横比变差;最后一项无条件入行
    let mut row_len = 0usize;
    let mut row_sum: u64 = 0;
    let mut best_worst = f64::INFINITY;
    for (i, (_, s)) in items.iter().enumerate() {
        let trial: u64 = row_sum + s;
        if i > 0 {
            let w = worst_ratio(&items[..i + 1], shorter * shorter, trial);
            if w > best_worst {
                break;
            }
            best_worst = w;
        }
        row_len = i + 1;
        row_sum = trial;
    }
    if row_len == 0 {
        row_len = 1;
        row_sum = items[0].1;
    }

    // 行厚度 = 行面积 / 短边;行内每项沿短边按比例分配
    let thickness = (row_sum as f64 * px2_per_byte / shorter) as f32;
    let mut cursor = 0.0f32;
    for (t, s) in items.iter().take(row_len) {
        let extent = (*s as f64 * px2_per_byte / thickness as f64) as f32;
        let r = if horizontal {
            Rect::from_min_size(
                rect.min + Vec2::new(0.0, cursor),
                Vec2::new(thickness, extent),
            )
        } else {
            Rect::from_min_size(
                rect.min + Vec2::new(cursor, 0.0),
                Vec2::new(extent, thickness),
            )
        };
        out.push((t.clone(), r));
        cursor += extent;
    }

    // 剩余区域继续布局
    let rest = if horizontal {
        Rect::from_min_size(rect.min + Vec2::new(thickness, 0.0), Vec2::new((rect.width() - thickness).max(0.0), rect.height()))
    } else {
        Rect::from_min_size(rect.min + Vec2::new(0.0, thickness), Vec2::new(rect.width(), (rect.height() - thickness).max(0.0)))
    };
    layout(&items[row_len..], rest, out);
}

/// 一行的最差纵横比:row_sum 为行权重和,s2 为短边平方
fn worst_ratio(items: &[(impl Clone, u64)], s2: f64, row_sum: u64) -> f64 {
    let mx = items.iter().map(|(_, s)| *s).max().unwrap_or(1) as f64;
    let mn = items.iter().map(|(_, s)| *s).min().unwrap_or(1) as f64;
    let sum2 = row_sum as f64 * row_sum as f64;
    if sum2 <= 0.0 || s2 <= 0.0 || mn <= 0.0 {
        return f64::INFINITY;
    }
    (s2 * mx / sum2).max(sum2 / (s2 * mn))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_area_and_keeps_ratios() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 80.0));
        let items = vec![("a", 500u64), ("b", 300), ("c", 120), ("d", 60), ("e", 20)];
        let laid = squarify(&items, rect);
        assert_eq!(laid.len(), 5);
        // 全部在矩形内
        for (_, r) in &laid {
            assert!(r.min.x >= rect.min.x - 0.01 && r.max.x <= rect.max.x + 0.01);
            assert!(r.min.y >= rect.min.y - 0.01 && r.max.y <= rect.max.y + 0.01);
        }
        // 面积近似按权重分配
        let total_area: f32 = laid.iter().map(|(_, r)| r.width() * r.height()).sum();
        assert!((total_area - 8000.0).abs() < 100.0);
        let a = laid.iter().find(|(t, _)| *t == "a").unwrap().1;
        let e = laid.iter().find(|(t, _)| *t == "e").unwrap().1;
        assert!(a.width() * a.height() > e.width() * e.height());
    }

    #[test]
    fn single_item_gets_all() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(50.0, 40.0));
        let laid = squarify(&[("x", 7u64)], rect);
        assert_eq!(laid.len(), 1);
        assert!((laid[0].1.area() - 2000.0).abs() < 1.0);
    }

    #[test]
    fn drops_zero_weights() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(50.0, 40.0));
        let laid = squarify(&[("x", 7u64), ("z", 0)], rect);
        assert_eq!(laid.len(), 1);
    }
}
