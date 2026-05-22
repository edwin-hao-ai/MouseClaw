import { describe, it, expect } from "vitest";
import { soundEventsForTransition, type PetSoundSnapshot } from "./usePetSounds";

const base: PetSoundSnapshot = {
  mouseState: "sleep",
  companionState: "idle",
  continuing: false,
  intimacyLevel: 0,
  bonkActive: false,
  nudgeActive: false,
};

describe("soundEventsForTransition", () => {
  it("没有变化时不触发任何音效", () => {
    expect(soundEventsForTransition(base, base)).toEqual([]);
  });

  it("进入 jump → task-done；进入 block → error", () => {
    expect(soundEventsForTransition(base, { ...base, mouseState: "jump" })).toEqual(["task-done"]);
    expect(soundEventsForTransition(base, { ...base, mouseState: "block" })).toEqual(["error"]);
  });

  it("吃下文件 / 写入光标 各有声", () => {
    expect(soundEventsForTransition(base, { ...base, mouseState: "feed-digest" })).toEqual(["feed"]);
    expect(soundEventsForTransition(base, { ...base, mouseState: "paste" })).toEqual(["insert"]);
  });

  it("companion 进入 hop/dizzy 各触发对应音", () => {
    expect(soundEventsForTransition(base, { ...base, companionState: "hop" })).toEqual(["hop"]);
    expect(soundEventsForTransition(base, { ...base, companionState: "dizzy" })).toEqual(["dizzy"]);
  });

  it("waking 故意不发声（防回到桌面被起床音骚扰 · 2026-05-23）", () => {
    expect(soundEventsForTransition(base, { ...base, companionState: "waking" })).toEqual([]);
  });

  it("clicked / excited 故意不自动触发（防吵）", () => {
    expect(soundEventsForTransition(base, { ...base, companionState: "clicked" })).toEqual([]);
    expect(soundEventsForTransition(base, { ...base, companionState: "excited" })).toEqual([]);
  });

  it("续 session 上升沿 → link，只在 false→true 那一次", () => {
    expect(soundEventsForTransition(base, { ...base, continuing: true })).toEqual(["link"]);
    const onState = { ...base, continuing: true };
    expect(soundEventsForTransition(onState, onState)).toEqual([]);
  });

  it("亲密度升级 → levelup（只在升高时）", () => {
    expect(soundEventsForTransition(base, { ...base, intimacyLevel: 1 })).toEqual(["levelup"]);
    const l2 = { ...base, intimacyLevel: 2 };
    expect(soundEventsForTransition(l2, { ...l2, intimacyLevel: 1 })).toEqual([]); // 降级不响
  });

  it("撞墙 / 提醒 上升沿各触发一次", () => {
    expect(soundEventsForTransition(base, { ...base, bonkActive: true })).toEqual(["bonk"]);
    expect(soundEventsForTransition(base, { ...base, nudgeActive: true })).toEqual(["nudge"]);
  });

  it("一帧内多个信号同时变 → 全部触发", () => {
    const next = { ...base, mouseState: "jump" as const, continuing: true, bonkActive: true };
    expect(soundEventsForTransition(base, next).sort()).toEqual(["bonk", "link", "task-done"]);
  });
});
