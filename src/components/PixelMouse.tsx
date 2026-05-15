/**
 * Pixel mouse character — 16×16 SVG, skin-aware.
 *
 * Anatomy 仍锁定在 16×16 网格上（DESIGN.md §3.1），但每款皮肤有：
 *   - 自己的 8 色调色板（见 skins.ts）
 *   - 一个 body variant —— 控制耳/身/尾的形状微调
 *
 * 渲染逻辑与 docs/prototypes/mouse-skins-20260515.html 完全一致 ——
 * prototype 拍板后 1:1 移植，视觉等价。
 *
 * Mandatory:
 *   - image-rendering: pixelated; shape-rendering: crispEdges
 *   - size 必须是 16 的整数倍
 */
import "./PixelMouse.css";
import { getSkin, type Skin, type SkinId } from "../skins";

export type MouseState = "sleep" | "listen" | "think" | "write" | "jump" | "block";

interface PixelMouseProps {
  state: MouseState;
  size?: 32 | 48 | 64 | 96 | 128;
  /** Skin id. 默认 classic（向后兼容老 config）。 */
  skin?: SkinId;
  /** Show the green chain icon above the head (session-continuation indicator). */
  continuing?: boolean;
}

// ── Body parts (按 body variant 决定形状) ───────────────────────────

function Ears({ skin }: { skin: Skin }) {
  const p = skin.palette;
  if (skin.body === "robot") {
    // 天线代替耳朵
    return (
      <>
        <rect x="4"  y="-1" width="1" height="3" fill={p.earOut} />
        <rect x="4"  y="-2" width="1" height="1" fill={p.earIn} />
        <rect x="11" y="-1" width="1" height="3" fill={p.earOut} />
        <rect x="11" y="-2" width="1" height="1" fill={p.earIn} />
      </>
    );
  }
  if (skin.body === "chubby") {
    // 大耳朵（外扩 1px）
    return (
      <>
        <rect x="2"  y="0" width="3" height="3" fill={p.earOut} />
        <rect x="1"  y="1" width="4" height="3" fill={p.earOut} />
        <rect x="11" y="0" width="3" height="3" fill={p.earOut} />
        <rect x="11" y="1" width="4" height="3" fill={p.earOut} />
        <rect x="3"  y="1" width="1" height="2" fill={p.earIn} />
        <rect x="12" y="1" width="1" height="2" fill={p.earIn} />
      </>
    );
  }
  // standard / slim / ninja / round
  return (
    <>
      <rect x="3"  y="1" width="2" height="2" fill={p.earOut} />
      <rect x="2"  y="2" width="3" height="2" fill={p.earOut} />
      <rect x="11" y="1" width="2" height="2" fill={p.earOut} />
      <rect x="11" y="2" width="3" height="2" fill={p.earOut} />
      <rect x="3"  y="2" width="1" height="1" fill={p.earIn} />
      <rect x="12" y="2" width="1" height="1" fill={p.earIn} />
    </>
  );
}

function Body({ skin }: { skin: Skin }) {
  const p = skin.palette;
  if (skin.body === "slim") {
    return (
      <>
        <rect x="4" y="3"  width="8"  height="1" fill={p.body} />
        <rect x="3" y="4"  width="10" height="3" fill={p.body} />
        <rect x="4" y="7"  width="8"  height="1" fill={p.body} />
        <rect x="6" y="8"  width="4"  height="2" fill={p.belly} />
        <rect x="4" y="8"  width="2"  height="2" fill={p.body} />
        <rect x="10" y="8" width="2"  height="2" fill={p.body} />
        <rect x="5" y="10" width="6"  height="1" fill={p.body} />
      </>
    );
  }
  if (skin.body === "round") {
    return (
      <>
        <rect x="3" y="3" width="10" height="1" fill={p.body} />
        <rect x="2" y="4" width="12" height="3" fill={p.body} />
        <rect x="3" y="7" width="10" height="1" fill={p.body} />
        <rect x="4" y="8" width="8"  height="3" fill={p.belly} />
        <rect x="3" y="8" width="1"  height="3" fill={p.body} />
        <rect x="12" y="8" width="1" height="3" fill={p.body} />
      </>
    );
  }
  // standard / chubby / ninja / robot
  return (
    <>
      <rect x="3" y="3"  width="10" height="1" fill={p.body} />
      <rect x="2" y="4"  width="12" height="3" fill={p.body} />
      <rect x="3" y="7"  width="10" height="1" fill={p.body} />
      <rect x="5" y="8"  width="6"  height="2" fill={p.belly} />
      <rect x="3" y="8"  width="2"  height="2" fill={p.body} />
      <rect x="11" y="8" width="2"  height="2" fill={p.body} />
      <rect x="4" y="10" width="8"  height="1" fill={p.body} />
    </>
  );
}

function NinjaMask({ skin }: { skin: Skin }) {
  if (skin.body !== "ninja") return null;
  const p = skin.palette;
  return (
    <>
      <rect x="2"  y="5" width="12" height="1" fill="#1a1a1a" />
      <rect x="0"  y="5" width="2"  height="1" fill={p.earIn} />
      <rect x="14" y="5" width="2"  height="1" fill={p.earIn} />
    </>
  );
}

function Tail({ skin }: { skin: Skin }) {
  const p = skin.palette;
  if (skin.body === "round") {
    // 卷尾巴
    return (
      <>
        <rect x="13" y="9" width="1" height="1" fill={p.tail} />
        <rect x="14" y="8" width="1" height="2" fill={p.tail} />
        <rect x="15" y="7" width="1" height="1" fill={p.tail} />
        <rect x="14" y="6" width="2" height="1" fill={p.tail} />
      </>
    );
  }
  if (skin.body === "robot") {
    // 短金属尾
    return (
      <>
        <rect x="13" y="9" width="2" height="1" fill={p.tail} />
        <rect x="15" y="8" width="1" height="1" fill={p.tail} />
      </>
    );
  }
  // 标准
  return (
    <>
      <rect x="13" y="9" width="1" height="1" fill={p.tail} />
      <rect x="14" y="7" width="1" height="3" fill={p.tail} />
    </>
  );
}

function Eyes({ state, skin }: { state: MouseState; skin: Skin }) {
  const e = skin.palette.eye;
  if (state === "sleep" || state === "write") {
    return (
      <>
        <rect x="5"  y="6" width="1" height="1" fill={e} />
        <rect x="10" y="6" width="1" height="1" fill={e} />
      </>
    );
  }
  if (state === "jump") {
    return (
      <>
        <rect x="5"  y="5" width="1" height="1" fill={e} />
        <rect x="6"  y="6" width="1" height="1" fill={e} />
        <rect x="9"  y="6" width="1" height="1" fill={e} />
        <rect x="10" y="5" width="1" height="1" fill={e} />
      </>
    );
  }
  // listen / think / block — alert 2-px
  return (
    <>
      <rect x="5"  y="5" width="1" height="2" fill={e} />
      <rect x="10" y="5" width="1" height="2" fill={e} />
    </>
  );
}

function Extras({ state, skin }: { state: MouseState; skin: Skin }) {
  const p = skin.palette;
  if (state === "listen") {
    return <rect x="7" y="9" width="2" height="1" fill={p.eye} />;
  }
  if (state === "think") {
    return (
      <>
        <rect x="13" y="0" width="1" height="1" fill="var(--accent-primary)" />
        <rect x="14" y="1" width="1" height="1" fill="var(--accent-primary)" />
        <rect x="13" y="2" width="1" height="1" fill="var(--accent-primary)" />
        <rect x="13" y="4" width="1" height="1" fill="var(--accent-primary)" />
      </>
    );
  }
  if (state === "write") {
    return (
      <>
        <rect x="3" y="12" width="1" height="3" fill="var(--warn)" />
        <rect x="3" y="15" width="1" height="1" fill={p.eye} />
      </>
    );
  }
  if (state === "block") {
    return (
      <>
        <rect x="7" y="-2" width="2" height="2" fill="var(--danger)" />
        <rect x="7" y="1"  width="2" height="1" fill="var(--danger)" />
      </>
    );
  }
  return null;
}

export function PixelMouse({ state, size = 96, skin = "classic", continuing = false }: PixelMouseProps) {
  const s = getSkin(skin);
  return (
    <div className={`mouse-wrap mouse-${state} mouse-skin-${s.id}`} style={{ width: size, height: size }}>
      {continuing && <div className="mouse-chain" aria-hidden />}
      <svg
        viewBox="-1 -3 18 18"
        width={size}
        height={size}
        className="mouse-svg"
        role="img"
        aria-label={`MouseClaw, ${s.name}, ${state} state`}
      >
        <Ears skin={s} />
        <NinjaMask skin={s} />
        <Body skin={s} />
        <Tail skin={s} />
        <Eyes state={state} skin={s} />
        <Extras state={state} skin={s} />
        <rect x="4" y="11" width="2" height="1" fill={s.palette.paw} />
        <rect x="10" y="11" width="2" height="1" fill={s.palette.paw} />
        <rect x="7" y="7" width="2" height="1" fill={s.palette.nose} />
      </svg>
    </div>
  );
}
