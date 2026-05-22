/**
 * usePetSounds —— 把桌宠状态机接到 petAudio：状态切换 → 触发对应音效。
 *
 * 两层来源（与 PixelMouse 一致）：
 *   - mouseState（任务态）：jump/block/feed-digest/paste 进入时各发一声
 *   - companionState（环境态）：waking/hop/dizzy 进入时发声；sleep 时循环鼾声
 *   - 额外信号：续 session(link) / 亲密度升级(levelup) / 撞墙(bonk) / 主动提醒(nudge)
 *
 * 触发集是**精选**的（见 CLAUDE.md「会不会打扰」）：
 *   - 故意 **不** 自动触发 squeak（每次桌面点击都会进 companion=clicked，太吵）——
 *     桌宠被直接点击的吱声由 App.handleMouseClick 单独发，有边界。
 *   - 同理 excited（鼠标贴近）默认不自动触发，留 API 备用。
 * 单一开关 sfx_enabled 总闸；音量 sfx_volume。两者从 Rust config 灌进 petAudio。
 */
import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

import * as petAudio from "../audio/petAudio";
import type { SfxEvent } from "../audio/petAudio";
import type { SkinId } from "../skins";
import type { MouseState } from "../components/PixelMouse";
import type { CompanionState } from "./useCompanion";

const MOUSE_ON_ENTER: Partial<Record<MouseState, SfxEvent>> = {
  jump: "task-done",
  block: "error",
  "feed-digest": "feed",
  paste: "insert",
};

const COMPANION_ON_ENTER: Partial<Record<CompanionState, SfxEvent>> = {
  waking: "wake",
  hop: "hop",
  dizzy: "dizzy",
};

export interface PetSoundSnapshot {
  mouseState: MouseState;
  companionState?: CompanionState;
  continuing: boolean;
  intimacyLevel: number;
  bonkActive: boolean;
  nudgeActive: boolean;
}

/** 纯函数 —— 给定上一帧与当前帧，算出这一步要触发哪些一次性音效（不含循环鼾声）。 */
export function soundEventsForTransition(
  prev: PetSoundSnapshot,
  next: PetSoundSnapshot,
): SfxEvent[] {
  const out: SfxEvent[] = [];
  if (next.mouseState !== prev.mouseState) {
    const e = MOUSE_ON_ENTER[next.mouseState];
    if (e) out.push(e);
  }
  if (next.companionState !== prev.companionState && next.companionState) {
    const e = COMPANION_ON_ENTER[next.companionState];
    if (e) out.push(e);
  }
  if (next.continuing && !prev.continuing) out.push("link");
  if (next.intimacyLevel > prev.intimacyLevel) out.push("levelup");
  if (next.bonkActive && !prev.bonkActive) out.push("bonk");
  if (next.nudgeActive && !prev.nudgeActive) out.push("nudge");
  return out;
}

export interface UsePetSoundsInput {
  skin: SkinId;
  mouseState: MouseState;
  companionState?: CompanionState;
  continuing: boolean;
  intimacyLevel: number;
  bonkActive: boolean;
  nudgeActive: boolean;
}

export function usePetSounds(input: UsePetSoundsInput) {
  // 启动读 config + 监听托盘切换 + 首次手势解锁 AudioContext
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    invoke<{ enabled: boolean; volume: number }>("get_sfx_config")
      .then((c) => { petAudio.setEnabled(c.enabled); petAudio.setVolume(c.volume); })
      .catch(() => { /* 浏览器 dev 模式 */ });
    try {
      const p = listen<{ enabled: boolean; volume: number }>("sfx-changed", (e) => {
        petAudio.setEnabled(e.payload.enabled);
        petAudio.setVolume(e.payload.volume);
      });
      p.then((fn) => { unlisten = fn; }).catch(() => {});
    } catch { /* dev mode */ }
    const unlock = () => petAudio.resume();
    window.addEventListener("pointerdown", unlock);
    window.addEventListener("keydown", unlock);
    return () => {
      if (unlisten) unlisten();
      window.removeEventListener("pointerdown", unlock);
      window.removeEventListener("keydown", unlock);
      petAudio.stopSnore();
    };
  }, []);

  // 一次性音效：边沿检测
  const prevRef = useRef<PetSoundSnapshot>({
    mouseState: input.mouseState,
    companionState: input.companionState,
    continuing: input.continuing,
    intimacyLevel: input.intimacyLevel,
    bonkActive: input.bonkActive,
    nudgeActive: input.nudgeActive,
  });
  useEffect(() => {
    const next: PetSoundSnapshot = {
      mouseState: input.mouseState,
      companionState: input.companionState,
      continuing: input.continuing,
      intimacyLevel: input.intimacyLevel,
      bonkActive: input.bonkActive,
      nudgeActive: input.nudgeActive,
    };
    for (const ev of soundEventsForTransition(prevRef.current, next)) {
      petAudio.playEvent(ev, input.skin);
    }
    prevRef.current = next;
  }, [
    input.skin, input.mouseState, input.companionState, input.continuing,
    input.intimacyLevel, input.bonkActive, input.nudgeActive,
  ]);

  // 循环鼾声：真正睡着（companion=sleep）时打鼾，醒/换皮即停。
  useEffect(() => {
    if (input.companionState === "sleep") petAudio.startSnore(input.skin);
    else petAudio.stopSnore();
  }, [input.companionState, input.skin]);
}
