/** GSAP 动效封装。
 *
 * 约定:
 *  - 所有函数统一读取 `.reduce-motion` 偏好,开启时直接跳过(不写内联样式);
 *  - 动画结束用 clearProps 清除内联 transform/opacity,避免与 CSS 布局过渡打架;
 *  - 位置/尺寸形变仍由 CSS transition 负责(Treemap 块),GSAP 只管入场/离场。 */
import { gsap } from "gsap";

type Targets =
  | Element
  | Element[]
  | NodeListOf<Element>
  | string
  | null
  | undefined;
type Tween = ReturnType<typeof gsap.fromTo>;
type TweenTarget = Parameters<typeof gsap.fromTo>[0];

/** 是否允许播放动效(用户勾选"减少动画"时关闭) */
export function motionEnabled(): boolean {
  return !document.documentElement.classList.contains("reduce-motion");
}

function isEmpty(targets: Targets): boolean {
  if (!targets) return true;
  if (typeof targets === "string") return targets.length === 0;
  if (targets instanceof NodeList) return targets.length === 0;
  if (Array.isArray(targets)) return targets.length === 0;
  return false;
}

export interface EnterOptions {
  y?: number;
  duration?: number;
  stagger?: number;
  delay?: number;
  scale?: number;
}

/** 多个元素依次淡入上移(工具栏 / 面板入场) */
export function enterStagger(
  targets: Targets,
  opts: EnterOptions = {},
): Tween | null {
  if (!motionEnabled() || isEmpty(targets)) return null;
  return gsap.fromTo(
    targets as TweenTarget,
    { autoAlpha: 0, y: opts.y ?? 14, scale: opts.scale ?? 1 },
    {
      autoAlpha: 1,
      y: 0,
      scale: 1,
      duration: opts.duration ?? 0.5,
      stagger: opts.stagger ?? 0.08,
      delay: opts.delay ?? 0,
      ease: "power3.out",
      clearProps: "transform,opacity,visibility",
    },
  );
}

/** 单个区域淡入上移(Tab 切换 / 结果首次出现) */
export function enterPanel(
  target: Targets,
  opts: EnterOptions = {},
): Tween | null {
  if (!motionEnabled() || isEmpty(target)) return null;
  return gsap.fromTo(
    target as TweenTarget,
    { autoAlpha: 0, y: opts.y ?? 8 },
    {
      autoAlpha: 1,
      y: 0,
      duration: opts.duration ?? 0.34,
      stagger: opts.stagger ?? 0,
      ease: "power2.out",
      clearProps: "transform,opacity,visibility",
    },
  );
}

/** Treemap 色块按面积顺序(布局即降序)错峰淡入 */
export function enterBlocks(targets: Targets): Tween | null {
  if (!motionEnabled() || isEmpty(targets)) return null;
  return gsap.fromTo(
    targets as TweenTarget,
    { autoAlpha: 0, scale: 0.94 },
    {
      autoAlpha: 1,
      scale: 1,
      duration: 0.42,
      stagger: 0.006,
      ease: "power2.out",
      clearProps: "transform,opacity,visibility",
    },
  );
}

/** Toast 入场:轻微回弹滑入 */
export function enterToast(el: Element, done: () => void): void {
  if (!motionEnabled()) {
    gsap.set(el, { autoAlpha: 1, y: 0, scale: 1 });
    done();
    return;
  }
  gsap.fromTo(
    el,
    { autoAlpha: 0, y: 16, scale: 0.96 },
    {
      autoAlpha: 1,
      y: 0,
      scale: 1,
      duration: 0.42,
      ease: "back.out(1.7)",
      onComplete: done,
    },
  );
}

/** Toast 离场:淡出下沉 */
export function leaveToast(el: Element, done: () => void): void {
  if (!motionEnabled()) {
    done();
    return;
  }
  gsap.to(el, {
    autoAlpha: 0,
    y: 10,
    duration: 0.24,
    ease: "power2.in",
    onComplete: done,
  });
}

/** 加载遮罩淡入(不阻塞内容渲染,只盖住切换瞬间的布局) */
export function fadeIn(el: Element, done: () => void): void {
  if (!motionEnabled()) {
    gsap.set(el, { autoAlpha: 1 });
    done();
    return;
  }
  gsap.fromTo(
    el,
    { autoAlpha: 0 },
    { autoAlpha: 1, duration: 0.18, ease: "power2.out", onComplete: done },
  );
}

/** 加载遮罩淡出 */
export function fadeOut(el: Element, done: () => void): void {
  if (!motionEnabled()) {
    done();
    return;
  }
  gsap.to(el, {
    autoAlpha: 0,
    duration: 0.26,
    ease: "power2.in",
    onComplete: done,
  });
}

/** 主题切换时的轻微亮度反馈 */
export function flashTheme(el: Element): void {
  if (!motionEnabled()) return;
  gsap.fromTo(
    el,
    { opacity: 0.86 },
    { opacity: 1, duration: 0.32, ease: "power2.out", clearProps: "opacity" },
  );
}
