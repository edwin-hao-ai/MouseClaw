/**
 * v0.4+ · 桌宠亲密度系统
 *
 * 持久化（localStorage）互动累计 + 最后互动时间，输出：
 *   - level 0–3：互动越多越亲（眼睛微大 / 更常 glance / idle 更活泼）
 *   - neglected：冷落超过 NEGLECT_DAYS 天 → 委屈表情
 *
 * 隐私：纯本地 localStorage，不上报。计数只在"用户主动互动"时 +1（点击 / 打字 /
 * 贴近，由 useCompanion 的 onInteract 回调驱动），不记录任何内容。
 *
 * 为什么用 localStorage 而不是 Rust config：亲密度是纯视觉调味，丢了无所谓
 * （顶多桌宠"忘了"你们多熟），不值得占 config.json + IPC。webview 重启
 * localStorage 仍在（同 origin）。
 */

import { useCallback, useEffect, useRef, useState } from "react";

const KEY_COUNT = "mouseclaw.intimacy.count";
const KEY_LAST = "mouseclaw.intimacy.lastInteractMs";

/** 互动次数 → level 阈值 */
export const LEVEL_THRESHOLDS = [0, 50, 200, 600] as const; // L0 / L1 / L2 / L3
export const NEGLECT_DAYS = 3;
/** 同一连续互动期内最多每这么久才 +1（防止一次长打字刷爆计数） */
export const INTERACT_DEBOUNCE_MS = 3000;

export interface Intimacy {
  level: 0 | 1 | 2 | 3;
  count: number;
  neglected: boolean;
  /** 给 useCompanion 的 onInteract 用 —— 用户主动互动时调它 */
  bumpInteract: () => void;
}

function readNum(key: string, fallback: number): number {
  try {
    const v = localStorage.getItem(key);
    if (v == null) return fallback;
    const n = Number(v);
    return Number.isFinite(n) ? n : fallback;
  } catch { return fallback; }
}

export function levelFor(count: number): 0 | 1 | 2 | 3 {
  let lvl: 0 | 1 | 2 | 3 = 0;
  for (let i = 0; i < LEVEL_THRESHOLDS.length; i++) {
    if (count >= LEVEL_THRESHOLDS[i]) lvl = i as 0 | 1 | 2 | 3;
  }
  return lvl;
}

export function useIntimacy(): Intimacy {
  const [count, setCount] = useState(() => readNum(KEY_COUNT, 0));
  const [neglected, setNeglected] = useState(false);
  const lastBumpAtRef = useRef(0);

  // 启动 + 每分钟检查冷落
  useEffect(() => {
    const check = () => {
      const lastMs = readNum(KEY_LAST, Date.now());
      const days = (Date.now() - lastMs) / 86400_000;
      setNeglected(days >= NEGLECT_DAYS);
    };
    check();
    const id = window.setInterval(check, 60_000);
    return () => window.clearInterval(id);
  }, []);

  const bumpInteract = useCallback(() => {
    const now = Date.now();
    // 记录"最后互动" + 清委屈（无论是否到 debounce）
    try { localStorage.setItem(KEY_LAST, String(now)); } catch { /* private mode */ }
    setNeglected(false);
    // 计数去抖：连续互动期内每 INTERACT_DEBOUNCE_MS 才 +1
    if (now - lastBumpAtRef.current < INTERACT_DEBOUNCE_MS) return;
    lastBumpAtRef.current = now;
    setCount((c) => {
      const next = c + 1;
      try { localStorage.setItem(KEY_COUNT, String(next)); } catch { /* private mode */ }
      return next;
    });
  }, []);

  return { level: levelFor(count), count, neglected, bumpInteract };
}
