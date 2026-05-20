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

export type MouseState =
  | "sleep" | "listen" | "think" | "write" | "jump" | "block"
  | "type"   // v0.1.14 · 语音 IME 中：盯光标，前爪举笔状
  | "hub"    // v0.1.14 · Hub 打开：坐下，弯眼 ︶
  | "paste"  // v0.1.14 · 粘贴动作：爪子拎剪贴板，闪一下
  | "feed-wait"     // v0.4 · 文件 hover 在桌宠上：张嘴等接收
  | "feed-digest"   // v0.4 · 已吞下文件：闭眼眯笑 + 肚子发光（CSS）
  | "talk";  // v0.3.11 · AI 流式输出时：嘴一直在动 + 身体节奏抖（CSS keyframe mc-talk）

interface PixelMouseProps {
  state: MouseState;
  /** Width/height in px. Common: 22 (tiny preview), 32 / 48 / 64 / 96 / 128. */
  size?: number;
  /** Skin id. 默认 classic（向后兼容老 config）。 */
  skin?: SkinId;
  /** Show the green chain icon above the head (session-continuation indicator). */
  continuing?: boolean;
  /** v0.4 · Reactive T1：剪贴板触发的一次性抖耳，由 CSS 动画自动消失。 */
  twitching?: boolean;
  /**
   * v0.4+ · 陪伴向动画 —— 来自 useCompanion hook 的当前帧。
   *
   * - companionState 控制 mouse-wrap 上的额外 class（typing 节奏点头 / alert 抬头 /
   *   excited 兴奋 / sleep 趴下闭眼）—— 与现有 MouseState 解耦：
   *   companion 是"环境感知"层，state 是"任务进行中"层。
   *   任何 state 非 idle 时（listening / thinking / write / …）忽略 companion class。
   * - eyeOffset 平移整个 `<g.mc-eyes>` 组，让 9 款皮肤的眼睛 rect 一起朝光标偏 ——
   *   palette/anchor 都不变，自动适配全皮肤（见 CLAUDE.md "动画/陪伴效果必须适配
   *   所有皮肤"硬规则）。
   */
  companionState?: "idle" | "typing" | "alert" | "excited" | "sleep"
                 | "clicked" | "worried" | "drowsy"
                 | "waking" | "dizzy" | "hop" | "glance";
  eyeOffset?: { x: number; y: number };
  /** v0.4+ · 亲密度 0–3 —— level 越高 idle 时眼睛微大；见 useIntimacy */
  intimacyLevel?: 0 | 1 | 2 | 3;
  /** v0.4+ · 冷落超 N 天 → 委屈表情 */
  neglected?: boolean;
}

// ── Body parts (按 body variant 决定形状) ───────────────────────────

/** Ears — 拆 L/R 两个独立 <g>，每个标 transform-origin 在自身基部，
 *  这样 reactive 抖耳 / 各种 hover 动画可以让左右独立摆动。
 *  CSS 走 .mc-ear-l / .mc-ear-r 类。
 *
 *  data-pivot 是 CSS 不便直接读取的；我们用 inline style 设 transform-origin。
 *  注意 SVG transform-origin 必须用像素值在 svg 坐标系下生效。 */
function Ears({ skin }: { skin: Skin }) {
  const p = skin.palette;
  if (skin.body === "robot") {
    // 天线代替耳朵 —— 基部在 (x=4, y=2) 和 (x=11, y=2)，抖起来像 LED 摆动
    return (
      <>
        <g className="mc-ear mc-ear-l" style={{ transformOrigin: "4.5px 2px" }}>
          <rect x="4"  y="-1" width="1" height="3" fill={p.earOut} />
          <rect x="4"  y="-2" width="1" height="1" fill={p.earIn} />
        </g>
        <g className="mc-ear mc-ear-r" style={{ transformOrigin: "11.5px 2px" }}>
          <rect x="11" y="-1" width="1" height="3" fill={p.earOut} />
          <rect x="11" y="-2" width="1" height="1" fill={p.earIn} />
        </g>
      </>
    );
  }
  if (skin.body === "cat") {
    // 高三角猫耳 —— 基部 (x=3, y=3) 和 (x=12, y=3)
    return (
      <>
        <g className="mc-ear mc-ear-l" style={{ transformOrigin: "3.5px 3px" }}>
          <rect x="2" y="0" width="1" height="3" fill={p.earOut} />
          <rect x="3" y="1" width="1" height="2" fill={p.earOut} />
          <rect x="4" y="2" width="1" height="1" fill={p.earOut} />
          <rect x="3" y="2" width="1" height="1" fill={p.earIn} />
        </g>
        <g className="mc-ear mc-ear-r" style={{ transformOrigin: "12.5px 3px" }}>
          <rect x="13" y="0" width="1" height="3" fill={p.earOut} />
          <rect x="12" y="1" width="1" height="2" fill={p.earOut} />
          <rect x="11" y="2" width="1" height="1" fill={p.earOut} />
          <rect x="12" y="2" width="1" height="1" fill={p.earIn} />
        </g>
      </>
    );
  }
  if (skin.body === "fox") {
    return (
      <>
        <g className="mc-ear mc-ear-l" style={{ transformOrigin: "3.5px 3px" }}>
          <rect x="2" y="-1" width="1" height="4" fill={p.earOut} />
          <rect x="3" y="0"  width="1" height="3" fill={p.earOut} />
          <rect x="4" y="1"  width="1" height="2" fill={p.earOut} />
          <rect x="3" y="1"  width="1" height="2" fill={p.earIn} />
        </g>
        <g className="mc-ear mc-ear-r" style={{ transformOrigin: "12.5px 3px" }}>
          <rect x="13" y="-1" width="1" height="4" fill={p.earOut} />
          <rect x="12" y="0"  width="1" height="3" fill={p.earOut} />
          <rect x="11" y="1"  width="1" height="2" fill={p.earOut} />
          <rect x="12" y="1"  width="1" height="2" fill={p.earIn} />
        </g>
      </>
    );
  }
  if (skin.body === "frog") {
    // 蛙：眼鼓包不"抖耳"而是"眼珠转一下"。CSS 用单独 class 走不同 keyframe。
    return (
      <>
        <g className="mc-ear mc-ear-l mc-ear-frog" style={{ transformOrigin: "4.5px 1.5px" }}>
          <rect x="3" y="0" width="3" height="3" fill={p.body} />
          <rect x="4" y="1" width="1" height="1" fill={p.earIn} />
          <rect x="4" y="0" width="1" height="1" fill={p.earOut} />
        </g>
        <g className="mc-ear mc-ear-r mc-ear-frog" style={{ transformOrigin: "11.5px 1.5px" }}>
          <rect x="10" y="0" width="3" height="3" fill={p.body} />
          <rect x="11" y="1" width="1" height="1" fill={p.earIn} />
          <rect x="11" y="0" width="1" height="1" fill={p.earOut} />
        </g>
      </>
    );
  }
  if (skin.body === "chubby") {
    return (
      <>
        <g className="mc-ear mc-ear-l" style={{ transformOrigin: "3px 3.5px" }}>
          <rect x="2"  y="0" width="3" height="3" fill={p.earOut} />
          <rect x="1"  y="1" width="4" height="3" fill={p.earOut} />
          <rect x="3"  y="1" width="1" height="2" fill={p.earIn} />
        </g>
        <g className="mc-ear mc-ear-r" style={{ transformOrigin: "13px 3.5px" }}>
          <rect x="11" y="0" width="3" height="3" fill={p.earOut} />
          <rect x="11" y="1" width="4" height="3" fill={p.earOut} />
          <rect x="12" y="1" width="1" height="2" fill={p.earIn} />
        </g>
      </>
    );
  }
  // standard / slim / ninja / round —— 基部 (x=4, y=3) / (x=12, y=3)
  return (
    <>
      <g className="mc-ear mc-ear-l" style={{ transformOrigin: "4px 3px" }}>
        <rect x="3"  y="1" width="2" height="2" fill={p.earOut} />
        <rect x="2"  y="2" width="3" height="2" fill={p.earOut} />
        <rect x="3"  y="2" width="1" height="1" fill={p.earIn} />
      </g>
      <g className="mc-ear mc-ear-r" style={{ transformOrigin: "12px 3px" }}>
        <rect x="11" y="1" width="2" height="2" fill={p.earOut} />
        <rect x="11" y="2" width="3" height="2" fill={p.earOut} />
        <rect x="12" y="2" width="1" height="1" fill={p.earIn} />
      </g>
    </>
  );
}

function Body({ skin }: { skin: Skin }) {
  const p = skin.palette;
  if (skin.body === "cat") {
    // 猫：身体微长，肚子用 belly 色
    return (
      <>
        <rect x="3" y="3" width="10" height="1" fill={p.body} />
        <rect x="2" y="4" width="12" height="3" fill={p.body} />
        <rect x="3" y="7" width="10" height="1" fill={p.body} />
        <rect x="5" y="8" width="6"  height="2" fill={p.belly} />
        <rect x="3" y="8" width="2"  height="2" fill={p.body} />
        <rect x="11" y="8" width="2" height="2" fill={p.body} />
        <rect x="4" y="10" width="8" height="1" fill={p.body} />
        {/* 胡须 —— 左右各 2 根 */}
        <rect x="0" y="6" width="2" height="1" fill={p.body} opacity="0.7" />
        <rect x="0" y="7" width="2" height="1" fill={p.body} opacity="0.5" />
        <rect x="14" y="6" width="2" height="1" fill={p.body} opacity="0.7" />
        <rect x="14" y="7" width="2" height="1" fill={p.body} opacity="0.5" />
      </>
    );
  }
  if (skin.body === "fox") {
    // 狐：身体加宽，胸前白色 V 形
    return (
      <>
        <rect x="3" y="3" width="10" height="1" fill={p.body} />
        <rect x="2" y="4" width="12" height="3" fill={p.body} />
        <rect x="3" y="7" width="10" height="1" fill={p.body} />
        {/* 白胸毛 V */}
        <rect x="6" y="7" width="4" height="1" fill={p.belly} />
        <rect x="5" y="8" width="6" height="3" fill={p.belly} />
        <rect x="3" y="8" width="2" height="2" fill={p.body} />
        <rect x="11" y="8" width="2" height="2" fill={p.body} />
        <rect x="4" y="10" width="8" height="1" fill={p.body} />
      </>
    );
  }
  if (skin.body === "frog") {
    // 蛙：圆胖短身，无颈
    return (
      <>
        <rect x="2" y="3" width="12" height="1" fill={p.body} />
        <rect x="1" y="4" width="14" height="4" fill={p.body} />
        <rect x="2" y="8" width="12" height="2" fill={p.body} />
        {/* 米色大肚子 */}
        <rect x="4" y="6" width="8"  height="4" fill={p.belly} />
        {/* 嘴角的微笑 */}
        <rect x="6" y="9" width="4" height="1" fill={p.body} />
        <rect x="5" y="10" width="6" height="1" fill={p.body} />
      </>
    );
  }
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
  if (skin.body === "cat") {
    // 长猫尾 —— 弯起来
    return (
      <>
        <rect x="13" y="9"  width="1" height="1" fill={p.tail} />
        <rect x="14" y="8"  width="1" height="1" fill={p.tail} />
        <rect x="15" y="7"  width="1" height="2" fill={p.tail} />
        <rect x="15" y="5"  width="1" height="2" fill={p.tail} />
        <rect x="14" y="4"  width="1" height="1" fill={p.tail} />
      </>
    );
  }
  if (skin.body === "fox") {
    // 蓬松大尾 —— 3 宽 + 白尾尖
    return (
      <>
        <rect x="13" y="6"  width="3" height="4" fill={p.body} />
        <rect x="14" y="5"  width="2" height="1" fill={p.body} />
        <rect x="15" y="9"  width="1" height="2" fill={p.tail} />
        <rect x="14" y="10" width="2" height="1" fill={p.tail} />
      </>
    );
  }
  if (skin.body === "frog") {
    // 蛙没有尾
    return null;
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
  // v0.1.14 · type 状态：眼睛盯右下角（光标在那）
  if (state === "type") {
    return (
      <>
        <rect x="6"  y="5" width="1" height="2" fill={e} />
        <rect x="11" y="5" width="1" height="2" fill={e} />
      </>
    );
  }
  // v0.4 · feed-digest：闭眼眯笑（吃饱满足）
  if (state === "feed-digest") {
    return (
      <>
        <rect x="5"  y="6" width="2" height="1" fill={e} />
        <rect x="9"  y="6" width="2" height="1" fill={e} />
      </>
    );
  }
  // v0.4 · feed-wait：圆瞪眼盯着文件
  if (state === "feed-wait") {
    return (
      <>
        <rect x="5"  y="5" width="2" height="2" fill={e} />
        <rect x="9"  y="5" width="2" height="2" fill={e} />
      </>
    );
  }
  // v0.1.14 · hub 状态：弯眼 ︶（开心闭眼）
  if (state === "hub") {
    return (
      <>
        <rect x="5"  y="6" width="1" height="1" fill={e} />
        <rect x="6"  y="5" width="1" height="1" fill={e} />
        <rect x="9"  y="5" width="1" height="1" fill={e} />
        <rect x="10" y="6" width="1" height="1" fill={e} />
      </>
    );
  }
  // listen / think / block / paste — alert 2-px
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
    // v0.1.34 · 三个像素点上下错峰跳 —— 经典 typing indicator 焦虑缓解
    // 放在桌宠头顶右上角（远离眼睛，不挡脸）
    return (
      <>
        <rect className="mc-dot mc-dot-1" x="11" y="-2" width="1" height="1" fill="var(--accent-primary)" />
        <rect className="mc-dot mc-dot-2" x="13" y="-2" width="1" height="1" fill="var(--accent-primary)" />
        <rect className="mc-dot mc-dot-3" x="15" y="-2" width="1" height="1" fill="var(--accent-primary)" />
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
  // v0.1.14 · type 状态：前爪举起，像握笔（左边外伸 + 笔尖一点高亮）
  if (state === "type") {
    return (
      <>
        <rect x="2" y="9"  width="1" height="3" fill={p.body} />
        <rect x="1" y="8"  width="1" height="2" fill={p.body} />
        <rect x="0" y="7"  width="1" height="2" fill={p.paw} />
        <rect x="-1" y="6" width="1" height="1" fill={p.nose} />
      </>
    );
  }
  // v0.4 · feed-wait：张大嘴等接收文件（粉色嘴 + 一点黑色喉咙）
  if (state === "feed-wait") {
    return (
      <>
        <rect x="7" y="8" width="2" height="2" fill={p.eye} />
        <rect x="7" y="9" width="2" height="1" fill={p.nose} />
        {/* 头顶冒一颗"❓" */}
        <rect className="mc-dot mc-dot-1" x="14" y="-2" width="1" height="1" fill="var(--accent-primary)" />
      </>
    );
  }
  // v0.4 · feed-digest：肚子小亮点（CSS belly-glow 用类做脉冲）
  if (state === "feed-digest") {
    return (
      <>
        <rect x="6" y="9" width="4" height="1" fill={p.nose} opacity="0.5" />
        <rect x="7" y="8" width="2" height="2" fill={p.nose} opacity="0.7" />
      </>
    );
  }
  // v0.1.14 · paste 状态：右爪拎一个白色「剪贴板」小方块
  if (state === "paste") {
    return (
      <>
        {/* 剪贴板矩形 */}
        <rect x="13" y="10" width="3" height="3" fill="#ffffff" />
        <rect x="13" y="10" width="3" height="1" fill={p.body} />
        {/* 中间两条线表示纸上的字 */}
        <rect x="14" y="11" width="1" height="1" fill={p.eye} />
        {/* 爪子伸出 */}
        <rect x="12" y="10" width="1" height="2" fill={p.paw} />
      </>
    );
  }
  return null;
}

export function PixelMouse({
  state, size = 96, skin = "classic", continuing = false, twitching = false,
  companionState, eyeOffset, intimacyLevel = 0, neglected = false,
}: PixelMouseProps) {
  const s = getSkin(skin);
  // companion 在"非任务态"挂（idle 状态下 mouseStateFor 返回 "sleep"；onboarding/listening
  // 返回 "listen"）。任务进行中的 think/write/talk/feed-*/jump/block/paste/type 有自己的
  // 动画 / 表情，不被陪伴层覆盖。
  const isDefaultState = state === "sleep" || state === "listen";
  const companionActive = isDefaultState && companionState && companionState !== "idle";
  const companionClass = companionActive ? ` companion-${companionState}` : "";
  // 亲密度：只在非任务态生效（任务态不分心）。level>0 加 idle-intimacy-N，neglected 加委屈类。
  const intimacyClass = isDefaultState
    ? `${intimacyLevel > 0 ? ` intimacy-${intimacyLevel}` : ""}${neglected ? " companion-neglected" : ""}`
    : "";
  // 眼睛 anatomy：companion 非 sleep/drowsy/neglected 时，把 sleep 闭眼覆盖成 listen 睁眼
  // （眼球追鼠标 / 点头才看得见）。sleep/drowsy 保持闭眼线条，CSS 做半闭效果。
  const eyesOpen = companionActive && companionState !== "sleep" && companionState !== "drowsy";
  const eyesState = state === "sleep" && eyesOpen ? ("listen" as MouseState) : state;
  // 眼球 transform —— **全部折进 SVG attribute**（translate 追鼠标 + scale 亲密度）。
  // 关键（2026-05-20 修回归）：亲密度放大之前用 CSS `.intimacy-N .mc-eyes{transform:scale}`，
  // 但 CSS transform property 会**覆盖** SVG presentation attribute → 一旦亲密度到 level≥1，
  // 追鼠标的 translate 被 scale 盖掉，眼睛就不跟随了（用户报"之前 work 现在不 work"）。
  // 改成同一个 attribute 里串联，两者共存。
  //   - sleep/drowsy/dizzy/neglected：attr 留空，眼睛表情由各自 CSS（含 !important）接管
  //   - 否则：translate(追鼠标) + scale-around-center(亲密度)。
  //     SVG scale 默认绕 viewBox 原点(0,0)，会把眼睛推向左上 → 必须 translate 到眼睛中心
  //     (≈8,6.5) 再 scale 再 translate 回来，才是"原地放大"。
  const noOffset = companionState === "sleep" || companionState === "drowsy"
    || companionState === "dizzy" || neglected;
  const intimacyScale = (isDefaultState && !neglected) ? [1, 1.05, 1.10, 1.16][intimacyLevel] : 1;
  let eyesTransformAttr: string | undefined;
  if (!noOffset) {
    const t = eyeOffset ? `translate(${eyeOffset.x.toFixed(3)} ${eyeOffset.y.toFixed(3)})` : "";
    const s = intimacyScale !== 1
      ? ` translate(8 6.5) scale(${intimacyScale}) translate(-8 -6.5)` : "";
    eyesTransformAttr = (t + s).trim() || undefined;
  }
  return (
    <div className={`mouse-wrap mouse-${state} mouse-skin-${s.id}${twitching ? " mouse-twitch" : ""}${companionClass}${intimacyClass}`} style={{ width: size, height: size }}>
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
        <g className="mc-tail" style={{ transformOrigin: "13px 9px" }}>
          <Tail skin={s} />
        </g>
        <g className="mc-eyes" transform={eyesTransformAttr}>
          <Eyes state={eyesState} skin={s} />
        </g>
        <Extras state={state} skin={s} />
        <rect x="4" y="11" width="2" height="1" fill={s.palette.paw} />
        <rect x="10" y="11" width="2" height="1" fill={s.palette.paw} />
        <rect className="mc-nose" x="7" y="7" width="2" height="1" fill={s.palette.nose} style={{ transformOrigin: "8px 7.5px" }} />
      </svg>
    </div>
  );
}
