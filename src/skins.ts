/**
 * Mouse skin definitions — single source of truth for the React side.
 * Mirrors src-tauri/src/skins.rs (kebab-case id ↔ Rust enum).
 *
 * 每款皮肤 = 8 色调色板 + 体型 variant。anatomy 仍 16×16，由 PixelMouse.tsx
 * 按 body variant 渲染对应的耳/身/尾形状。
 */

export type SkinId =
  | "classic"
  | "lab"
  | "field"
  | "ninja"
  | "cyber"
  | "golden"
  // v0.1.26 · 非啮齿目
  | "cat-gray"
  | "fox-red"
  | "frog-tree";

export type BodyVariant =
  | "standard" | "slim" | "chubby" | "ninja" | "robot" | "round"
  // v0.1.26
  | "cat"  | "fox"  | "frog";

/** v0.1.26 · 物种（决定耳/身/尾的"骨架"，picker 里也用来分组） */
export type Species = "mouse" | "cat" | "fox" | "frog";

export interface SkinPalette {
  body: string;
  belly: string;
  earIn: string;
  earOut: string;
  eye: string;
  nose: string;
  tail: string;
  paw: string;
}

export interface Skin {
  id: SkinId;
  name: string;     // 中文展示名
  desc: string;     // 一句描述（Onboarding / tray 用）
  tag?: string;     // 角标（"默认" / "限定" 等）
  body: BodyVariant;
  palette: SkinPalette;
  /** v0.1.26 · 物种 —— picker 分组用 */
  species?: Species;
}

export const SKINS: readonly Skin[] = [
  {
    id: "classic",
    name: "经典灰",
    desc: "DESIGN.md 锁定的原版 · 灰身白肚粉耳",
    tag: "默认",
    body: "standard",
    palette: { body:"#cfcfcf", belly:"#ffffff", earIn:"#ff9bb8", earOut:"#cfcfcf",
               eye:"#1a1a1a", nose:"#d63d6a", tail:"#8a8a8a", paw:"#ffffff" },
  },
  {
    id: "lab",
    name: "小白鼠",
    desc: "通体雪白 + 红宝石眼 · 身体瘦一圈",
    body: "slim",
    palette: { body:"#fafafa", belly:"#ffffff", earIn:"#ffb3c8", earOut:"#e8e3dc",
               eye:"#ff5f57", nose:"#ff6b9d", tail:"#ffb3c8", paw:"#ffffff" },
  },
  {
    id: "field",
    name: "田鼠",
    desc: "棕色毛皮 + 大耳朵 · 圆滚滚",
    body: "chubby",
    palette: { body:"#a47148", belly:"#e8d5b0", earIn:"#d6a07a", earOut:"#7a4e2e",
               eye:"#1a1a1a", nose:"#5a2e10", tail:"#7a4e2e", paw:"#d6a07a" },
  },
  {
    id: "ninja",
    name: "忍者鼠",
    desc: "深色身 + 黑色蒙面带 · 黄眼锐利",
    body: "ninja",
    palette: { body:"#3a3a42", belly:"#5a5a64", earIn:"#1a1a1a", earOut:"#2a2a30",
               eye:"#ffd166", nose:"#1a1a1a", tail:"#2a2a30", paw:"#1a1a1a" },
  },
  {
    id: "cyber",
    name: "机械鼠",
    desc: "钛银机身 + 青蓝发光眼 · 天线代替耳朵",
    tag: "酷",
    body: "robot",
    palette: { body:"#c0c8d0", belly:"#e8eef4", earIn:"#00e5ff", earOut:"#7a8290",
               eye:"#00e5ff", nose:"#00e5ff", tail:"#7a8290", paw:"#5a6270" },
  },
  {
    id: "golden",
    name: "金鼠",
    desc: "奶油金身 + 圆润体型 + 卷尾巴",
    tag: "限定",
    body: "round",
    palette: { body:"#e8c87a", belly:"#fff4d6", earIn:"#d68a40", earOut:"#c8a050",
               eye:"#3a2818", nose:"#a06028", tail:"#c8a050", paw:"#fff4d6" },
  },
  // v0.1.26 · 非啮齿目家族 ────────────────────────────────────────────
  {
    id: "cat-gray",
    name: "小灰猫",
    desc: "尖耳 + 长卷尾 + 胡须 · 性格高冷",
    tag: "新",
    body: "cat",
    species: "cat",
    palette: { body:"#a8a8a8", belly:"#f4f4f4", earIn:"#ff9bb8", earOut:"#7a7a7a",
               eye:"#3fa66a", nose:"#d63d6a", tail:"#7a7a7a", paw:"#a8a8a8" },
  },
  {
    id: "fox-red",
    name: "赤狐",
    desc: "橙红毛 + 白胸 + 蓬松大尾 · 雪地里跳",
    tag: "新",
    body: "fox",
    species: "fox",
    palette: { body:"#d96e2c", belly:"#fff4ec", earIn:"#1a1a1a", earOut:"#a04a18",
               eye:"#1a1a1a", nose:"#1a1a1a", tail:"#fff4ec", paw:"#1a1a1a" },
  },
  {
    id: "frog-tree",
    name: "树蛙",
    desc: "圆头 + 突眼 + 白肚 · 蹲在叶子上",
    tag: "新",
    body: "frog",
    species: "frog",
    palette: { body:"#7ab84a", belly:"#f4e890", earIn:"#ffffff", earOut:"#5a8a30",
               eye:"#d63d6a", nose:"#5a8a30", tail:"#5a8a30", paw:"#f4e890" },
  },
] as const;

export const DEFAULT_SKIN: SkinId = "classic";

export function getSkin(id: SkinId | undefined | null): Skin {
  return SKINS.find(s => s.id === id) ?? SKINS[0];
}
