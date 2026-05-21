/**
 * useAdaptiveOverlay · 自适应 overlay 窗口尺寸（v0.4+）
 *
 * 问题（2026-05-20 用户反馈，已经踩过 N 次了）：
 *   overlay 窗口物理尺寸固定（80 compact / 320 expanded 两档）→ React 渲染的菜单 /
 *   气泡只要超过 320 就被 macOS 窗口边界剪掉，每加一个菜单项 / 一行翻译就得手动
 *   调 EXPANDED_SIZE 常量。
 *
 * 解：内容驱动 —— 扫所有可见 UI 元素的 boundingRect，取**并集** → invoke
 *   `set_overlay_content_size` → Rust 改窗口尺寸 + 保持桌宠锚点不变。
 *   再也不靠"猜数值"。
 *
 * 为什么不直接 ResizeObserver(.stage)：
 *   .stage 是 width:100%/height:100% 填满整个 overlay 窗口 —— 它的 rect 永远 = 窗口大小，
 *   不告诉你"内部内容实际占多少"。所以必须遍历它的 children（PetMenu / ribbon / nudge
 *   / bubble / pet 都是 absolute 定位的）求并集。
 *
 * 采样策略：MutationObserver 监听 stage subtree 变化（菜单弹/收 / ribbon 进/出）+
 *   每个候选 element 用 ResizeObserver 监听自身尺寸变化（菜单文字增多）+ 每 250ms
 *   一次兜底 ping（防漏报）。三路触发都走同一个 push() 函数，里面 dedupe。
 */

import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

// v0.4 fix (2026-05-20)：padding 分两档 ——
//   · 桌宠本体（.mouse-wrap）只有小 drop-shadow → BASE_PADDING 够，idle 不必撑大窗口
//     （保住 f253762 的"idle 缩窗不遮挡"）。
//   · 气泡类（.bubble / .pet-menu / voice-confirm 等）带 --shadow-bubble
//     (0 12px 32px，外扩 ~44px) → 用 SHADOW_PADDING，否则阴影被窗口边界裁成"一圈黑边"。
const BASE_PADDING = 24;     // 只有桌宠时的透气边距
const SHADOW_PADDING = 96;   // 有阴影气泡时的边距（≈ 每边 48px 容下软阴影）
/** 这些元素带重 box-shadow（--shadow-bubble），命中任一就用 SHADOW_PADDING。
 *  注意：不含 .stage-bubble —— 它装的是 session chip（轻阴影），用 BASE 即可。 */
const SHADOWED_SELECTOR = ".bubble, .pet-menu, .nudge-bubble, .rx-ribbon, [data-adaptive-measure]";
const DEBOUNCE_MS = 16;      // 一帧 60fps
const POLL_FALLBACK_MS = 1000; // 兜底轮询（v0.4 fix · 250ms → 1s，亚像素抖动放大成飘移的元凶）
const CHANGE_THRESHOLD_PX = 4; // v0.4 fix · 变化 < 4px 不 invoke（ceil + 亚像素 → 1-2px 噪声）

/** 需要纳入测量的 UI 元素选择器（按类名 / data-testid 找）。新加的浮窗 UI 也加这里。 */
const VISIBLE_UI_SELECTORS = [
  ".mouse-wrap",
  ".pet-menu",
  ".nudge-bubble",
  ".rx-ribbon",
  ".reactive-panel",       // 旧 design 残留 (现在被 rx-ribbon 取代)，保留以防
  ".bubble",
  ".stage-bubble",
  "[data-adaptive-measure]", // 任何想加的元素打 data-adaptive-measure="" 即可
].join(",");

export interface UseAdaptiveOverlayOptions {
  /** 只在 enabled=true 时驱动；false 时不调 invoke（让其他通道管理尺寸） */
  enabled: boolean;
}

export function useAdaptiveOverlay(
  rootRef: React.RefObject<HTMLElement | null>,
  opts: UseAdaptiveOverlayOptions,
): void {
  // 用 ref 保留最后一次 push 的尺寸，跨 effect 重启也保留（避免冷启重复 invoke）
  const lastSizeRef = useRef<{ w: number; h: number }>({ w: -1, h: -1 });

  useEffect(() => {
    const root = rootRef.current;
    if (!root || !opts.enabled) return;

    let scheduled: number | undefined;
    let pollId: number | undefined;

    const measureAndPush = () => {
      const els = root.querySelectorAll<HTMLElement>(VISIBLE_UI_SELECTORS);
      if (els.length === 0) return;
      let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
      // 有「可见」阴影气泡在场 → 用大 padding 容下阴影；否则（只有桌宠 / 阴影元素是
      // 零尺寸隐藏态）用小 padding 不撑大窗口。
      // ⚠️ 必须只认**可见(非零尺寸)**的 shadowed 元素：之前用 querySelector(SHADOWED_SELECTOR)
      // 判存在，零尺寸/隐藏的 .rx-ribbon 等也会强制 96px padding，跟"测量忽略零尺寸元素"
      // 不一致 → 窗口被无谓撑大。改为在同一遍 measure 里、对实际计入的可见元素判 shadowed。
      let hasShadowed = false;
      els.forEach((el) => {
        const r = el.getBoundingClientRect();
        if (r.width === 0 || r.height === 0) return; // 不可见元素跳过
        if (el.matches(SHADOWED_SELECTOR)) hasShadowed = true;
        if (r.left < minX) minX = r.left;
        if (r.top < minY) minY = r.top;
        if (r.right > maxX) maxX = r.right;
        if (r.bottom > maxY) maxY = r.bottom;
      });
      if (minX === Infinity) return;
      const pad = hasShadowed ? SHADOW_PADDING : BASE_PADDING;
      // v0.4 fix (2026-05-20)：Math.ceil 永远向上取整 + 亚像素 getBoundingClientRect
      // 会被放大成单向累积飘移。改用 Math.round + 4px 容差阈值。
      const w = Math.round(maxX - minX + pad);
      const h = Math.round(maxY - minY + pad);
      const dw = Math.abs(w - lastSizeRef.current.w);
      const dh = Math.abs(h - lastSizeRef.current.h);
      if (dw < CHANGE_THRESHOLD_PX && dh < CHANGE_THRESHOLD_PX) return;
      lastSizeRef.current = { w, h };
      invoke("set_overlay_content_size", { width: w, height: h }).catch(() => {});
    };

    const schedule = () => {
      if (scheduled) window.clearTimeout(scheduled);
      scheduled = window.setTimeout(measureAndPush, DEBOUNCE_MS);
    };

    // 触发源 1: MutationObserver（菜单 / ribbon 添加/移除）
    const mo = new MutationObserver(schedule);
    mo.observe(root, { childList: true, subtree: true, attributes: true, attributeFilter: ["style", "class"] });

    // 触发源 2: ResizeObserver 监听 root 自身（窗口本身尺寸变了也要重新算）
    const ro = new ResizeObserver(schedule);
    ro.observe(root);

    // 触发源 3: 兜底定时 ping
    pollId = window.setInterval(measureAndPush, POLL_FALLBACK_MS);

    // 首次立即同步
    measureAndPush();

    return () => {
      mo.disconnect();
      ro.disconnect();
      if (scheduled) window.clearTimeout(scheduled);
      if (pollId) window.clearInterval(pollId);
    };
  }, [rootRef, opts.enabled]);
}
