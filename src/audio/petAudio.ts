/**
 * 桌宠程序化音效引擎 —— 用 Web Audio 振荡器实时合成 chiptune，**不打包任何音频文件**。
 *
 * 设计拍板见 docs/prototypes/skin-system-audio-20260521.html（可试听）。
 *
 * 皮肤适配（CLAUDE.md「动画/陪伴必须适配所有皮肤」硬规则，应用到音效）：
 *   4 个物种各有"嗓音"（鼠=方波尖吱 / 猫=三角波带喵叫滑音 / 狐=锯齿 / 蛙=低频双脉冲呱呱），
 *   再叠每款皮肤的微调音（cents）。同一个事件在 9 款皮肤上音色都不同 —— 颜色驱动靠 palette，
 *   声音驱动靠 species + skinId，组件里**不** hardcode 频率。
 *
 * 纯模块（不 import tauri）—— 开关 / 音量由 usePetSounds 从 config 灌进来，方便单测。
 */
import { getSkin, type SkinId, type Species } from "../skins";

export type SfxEvent =
  | "task-done"  // jump · A 模式回复完成
  | "error"      // block
  | "feed"       // feed-digest · 吃下文件
  | "insert"     // paste · B 模式写入光标
  | "link"       // 续接 session
  | "levelup"    // 亲密度升级
  | "nudge"      // 主动提醒
  | "squeak"     // 点桌宠（直接交互，非每次桌面点击）
  | "hop"        // 打字结束蹦
  | "excited"    // 鼠标贴近（默认不自动触发，留 API）
  | "dizzy"      // 甩鼠标头晕
  | "bonk"       // 撞墙
  | "wake";      // 打哈欠醒来

interface Voice {
  wave: OscillatorType;
  pitch: number;   // 基频倍率（猫狐蛙比鼠低）
  bright: number;  // lowpass 亮度系数
  vibrato?: number;
  glide?: number;  // 默认音高滑移（猫的喵叫）
  croak?: boolean; // 蛙：每个音双脉冲
}

const VOICES: Record<Species, Voice> = {
  mouse: { wave: "square",   pitch: 1.0,  bright: 1.0 },
  cat:   { wave: "triangle", pitch: 0.74, bright: 0.85, vibrato: 13, glide: 1.04 },
  fox:   { wave: "sawtooth", pitch: 0.86, bright: 0.92 },
  frog:  { wave: "square",   pitch: 0.52, bright: 0.55, croak: true },
};

// 每款皮肤的微调音（cents）—— 让 9 只各有细微差别（cyber 偏高带电子味）。
const DETUNE: Partial<Record<SkinId, number>> = {
  classic: 0, lab: 30, field: -35, ninja: -12, cyber: 55, golden: -20,
};

// 音名 → 频率
const N = {
  C3: 130.81, G3: 196.0, A4: 440.0, C4: 261.63, Eb4: 311.13, E4: 329.63, G4: 392.0,
  C5: 523.25, E5: 659.25, G5: 783.99, A5: 880.0, B5: 987.77, C6: 1046.5,
};

let ctx: AudioContext | null = null;
let enabled = true;
let volume = 0.45;
let snoreTimer: number | null = null;
let snoreSpecies: Species | null = null;
const lastFire: Partial<Record<SfxEvent, number>> = {};

function ac(): AudioContext | null {
  if (typeof window === "undefined") return null;
  const AC = window.AudioContext || (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!AC) return null;
  if (!ctx) {
    try { ctx = new AC(); } catch { return null; }
  }
  if (ctx.state === "suspended") ctx.resume().catch(() => {});
  return ctx;
}

interface ToneOpts {
  wave?: OscillatorType; vol?: number; bright?: number;
  attack?: number; release?: number; detune?: number;
  vibrato?: number; bend?: number;
}

function tone(c: AudioContext, freq: number, t0: number, dur: number, o: ToneOpts = {}) {
  const wave = o.wave ?? "square";
  const vol = (o.vol ?? 0.2) * volume;
  const bright = o.bright ?? 1;
  const osc = c.createOscillator();
  const g = c.createGain();
  const lp = c.createBiquadFilter();
  osc.type = wave;
  osc.frequency.setValueAtTime(freq, t0);
  if (o.bend) osc.frequency.exponentialRampToValueAtTime(Math.max(20, freq * o.bend), t0 + dur);
  if (o.detune) osc.detune.setValueAtTime(o.detune, t0);
  lp.type = "lowpass";
  lp.frequency.value = Math.min(13000, freq * 6 * bright + 400);
  if (o.vibrato) {
    const lfo = c.createOscillator();
    const la = c.createGain();
    lfo.frequency.value = o.vibrato;
    la.gain.value = freq * 0.025;
    lfo.connect(la).connect(osc.frequency);
    lfo.start(t0); lfo.stop(t0 + dur + 0.05);
  }
  const a = o.attack ?? 0.005;
  const r = o.release ?? 0.06;
  g.gain.setValueAtTime(0.0001, t0);
  g.gain.exponentialRampToValueAtTime(Math.max(0.0002, vol), t0 + a);
  g.gain.exponentialRampToValueAtTime(0.0001, t0 + dur + r);
  osc.connect(lp).connect(g).connect(c.destination);
  osc.start(t0); osc.stop(t0 + dur + r + 0.02);
}

function noiseBurst(c: AudioContext, t0: number, dur: number, o: { vol?: number; lp?: number; hp?: number } = {}) {
  const buf = c.createBuffer(1, Math.max(1, Math.floor(c.sampleRate * dur)), c.sampleRate);
  const d = buf.getChannelData(0);
  for (let i = 0; i < d.length; i++) d[i] = Math.random() * 2 - 1;
  const n = c.createBufferSource();
  n.buffer = buf;
  const lp = c.createBiquadFilter(); lp.type = "lowpass"; lp.frequency.value = o.lp ?? 2000;
  const hp = c.createBiquadFilter(); hp.type = "highpass"; hp.frequency.value = o.hp ?? 200;
  const g = c.createGain();
  const vol = (o.vol ?? 0.15) * volume;
  g.gain.setValueAtTime(0.0001, t0);
  g.gain.exponentialRampToValueAtTime(Math.max(0.0002, vol), t0 + 0.01);
  g.gain.exponentialRampToValueAtTime(0.0001, t0 + dur);
  n.connect(hp).connect(lp).connect(g).connect(c.destination);
  n.start(t0); n.stop(t0 + dur + 0.02);
}

type VoiceTone = (freq: number, at: number, dur: number, o?: ToneOpts) => void;

function voiceFor(species: Species, skinId: SkinId, c: AudioContext): VoiceTone {
  const v = VOICES[species] ?? VOICES.mouse;
  const det = DETUNE[skinId] ?? 0;
  return (freq, at, dur, o = {}) => {
    const opt: ToneOpts = { wave: v.wave, bright: v.bright, detune: det, ...o };
    if (v.vibrato && opt.vibrato == null) opt.vibrato = v.vibrato;
    if (v.glide && opt.bend == null) opt.bend = v.glide;
    if (v.croak) {
      tone(c, freq * v.pitch, at, Math.min(dur, 0.05), opt);
      tone(c, freq * v.pitch * 0.97, at + 0.065, dur, opt);
      return;
    }
    tone(c, freq * v.pitch, at, dur, opt);
  };
}

const EVENTS: Record<SfxEvent, (vt: VoiceTone, t: number, c: AudioContext) => void> = {
  "task-done": (vt, t) => [N.C5, N.E5, N.G5, N.C6].forEach((f, i) => vt(f, t + i * 0.08, 0.13, { vol: 0.22 })),
  error: (vt, t) => [N.G4, N.Eb4, N.C4].forEach((f, i) => vt(f, t + i * 0.1, 0.17, { wave: "sawtooth", vol: 0.2, bright: 0.8 })),
  feed: (vt, t) => { vt(N.C4, t, 0.18, { vol: 0.18, bend: 0.65 }); vt(N.G5, t + 0.24, 0.15, { vol: 0.2 }); },
  insert: (vt, t) => { vt(N.A5, t, 0.04, { vol: 0.13, wave: "square" }); vt(N.C6, t + 0.1, 0.13, { vol: 0.2 }); },
  link: (vt, t) => { vt(N.C5, t, 0.12, { vol: 0.18 }); vt(N.G5, t + 0.13, 0.17, { vol: 0.2 }); },
  levelup: (vt, t) => [N.C5, N.E5, N.G5, N.B5, N.C6].forEach((f, i) => vt(f, t + i * 0.06, 0.12, { vol: 0.2 })),
  nudge: (vt, t) => { vt(N.E5, t, 0.18, { wave: "triangle", vol: 0.18 }); vt(N.C5, t + 0.17, 0.26, { wave: "triangle", vol: 0.16 }); },
  squeak: (vt, t) => vt(N.C6, t, 0.06, { vol: 0.16 }),
  hop: (vt, t) => vt(N.E5, t, 0.13, { vol: 0.2, bend: 1.6 }),
  excited: (vt, t) => { vt(N.G5, t, 0.07, { vol: 0.17 }); vt(N.C6, t + 0.09, 0.1, { vol: 0.2 }); },
  dizzy: (vt, t) => vt(N.A4, t, 0.5, { vol: 0.16, vibrato: 20, bend: 0.7 }),
  bonk: (vt, t, c) => { noiseBurst(c, t, 0.08, { vol: 0.18, lp: 1200 }); vt(N.G3, t, 0.14, { vol: 0.18, wave: "sine", bend: 0.6 }); },
  wake: (vt, t) => vt(N.C4, t, 0.5, { vol: 0.16, bend: 1.9, attack: 0.09, release: 0.22 }),
};

// 鼾声单次（吸→呼），基频随物种降低
function snoreOnce(species: Species, c: AudioContext, t: number) {
  const v = VOICES[species] ?? VOICES.mouse;
  const base = 110 * v.pitch;
  tone(c, base, t, 0.42, { wave: "sine", vol: 0.1, bend: 1.5, attack: 0.16, release: 0.1, bright: 0.5 });
  noiseBurst(c, t, 0.42, { vol: 0.035, lp: 560, hp: 110 });
  tone(c, base * 1.35, t + 0.58, 0.46, { wave: "sine", vol: 0.07, bend: 0.6, attack: 0.05, release: 0.22, bright: 0.5 });
}

const SNORE_INTERVAL_MS = 3500;

// ── 公共 API ──────────────────────────────────────────────────────────
export function setEnabled(on: boolean) {
  enabled = on;
  if (!on) stopSnore();
}
export function setVolume(v: number) {
  volume = Math.max(0, Math.min(1, v));
}
export function isEnabled(): boolean { return enabled; }

/** 解锁 AudioContext —— WKWebView 需用户手势后才出声，在首次 pointer/key 时调。 */
export function resume() {
  const c = ac();
  if (c && c.state === "suspended") c.resume().catch(() => {});
}

export function playEvent(ev: SfxEvent, skinId: SkinId) {
  if (!enabled) return;
  const c = ac();
  if (!c) return;
  const now = (typeof performance !== "undefined" ? performance.now() : Date.now());
  if (now - (lastFire[ev] ?? 0) < 400) return; // 节流：同事件 400ms 内不重复
  lastFire[ev] = now;
  const skin = getSkin(skinId);
  const sp = (skin.species ?? "mouse") as Species;
  EVENTS[ev](voiceFor(sp, skinId, c), c.currentTime + 0.001, c);
}

export function startSnore(skinId: SkinId) {
  if (!enabled) return;
  const sp = (getSkin(skinId).species ?? "mouse") as Species;
  if (snoreTimer != null && snoreSpecies === sp) return; // 已在为该物种打鼾
  stopSnore();
  snoreSpecies = sp;
  const tick = () => {
    if (!enabled) return;
    const c = ac();
    if (c) snoreOnce(sp, c, c.currentTime + 0.01);
  };
  tick();
  snoreTimer = window.setInterval(tick, SNORE_INTERVAL_MS);
}

export function stopSnore() {
  if (snoreTimer != null) {
    clearInterval(snoreTimer);
    snoreTimer = null;
  }
  snoreSpecies = null;
}
