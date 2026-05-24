/**
 * DrawOverlay (v0.1.21) —— AI 召唤期间的「实时画板」
 *
 * 用户反馈：之前轨迹只烘到截图上，过程中看不到自己画到哪。
 * 现在加全屏透明窗口，跟着 Rust 采样事件实时画粉色线 + 灰色辅助轨迹。
 *
 * **不抢鼠标焦点**（ignore_cursor_events = true）—— 用户的点击照样传给底层 app，
 * 我们只是叠加视觉反馈。Rust 那边用 NSEvent.pressedMouseButtons 判左键状态，跟
 * 不抢焦点不冲突。
 *
 * 性能：requestAnimationFrame 节流，最多 60fps 重绘
 */
import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

interface Pt { x: number; y: number; t: number; drawing: boolean; }

export default function DrawOverlay() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const pointsRef = useRef<Pt[]>([]);
  const dirtyRef = useRef(false);
  const [, setRedraw] = useState(0);

  // 监听 Rust 采样事件
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let unlistenClear: (() => void) | null = null;
    try {
      listen<Pt>("trail-point", (e) => {
        pointsRef.current.push(e.payload);
        dirtyRef.current = true;
      }).then(fn => { unlisten = fn; }).catch(() => {});
      listen("trail-clear", () => {
        pointsRef.current = [];
        dirtyRef.current = true;
        setRedraw(x => x + 1);
      }).then(fn => { unlistenClear = fn; }).catch(() => {});
    } catch {/* dev mode */}
    return () => {
      if (unlisten) unlisten();
      if (unlistenClear) unlistenClear();
    };
  }, []);

  // 重绘循环（rAF，最多 60fps）
  useEffect(() => {
    let raf = 0;
    const draw = () => {
      const c = canvasRef.current;
      if (c && dirtyRef.current) {
        dirtyRef.current = false;
        renderCanvas(c, pointsRef.current);
      }
      raf = requestAnimationFrame(draw);
    };
    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, []);

  // 适配 retina + 窗口 resize
  useEffect(() => {
    const fit = () => {
      const c = canvasRef.current;
      if (!c) return;
      const dpr = window.devicePixelRatio || 1;
      c.width = window.innerWidth * dpr;
      c.height = window.innerHeight * dpr;
      c.style.width = `${window.innerWidth}px`;
      c.style.height = `${window.innerHeight}px`;
      const ctx = c.getContext("2d");
      if (ctx) ctx.scale(dpr, dpr);
      dirtyRef.current = true;
    };
    fit();
    window.addEventListener("resize", fit);
    return () => window.removeEventListener("resize", fit);
  }, []);

  return (
    <canvas
      ref={canvasRef}
      style={{
        position: "fixed",
        inset: 0,
        width: "100vw",
        height: "100vh",
        pointerEvents: "none", // 必须 —— 让点击穿透到底层 app
        background: "transparent",
      }}
      aria-hidden
    />
  );
}

function renderCanvas(canvas: HTMLCanvasElement, points: Pt[]) {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const w = canvas.width / (window.devicePixelRatio || 1);
  const h = canvas.height / (window.devicePixelRatio || 1);
  ctx.clearRect(0, 0, w, h);
  if (points.length === 0) return;

  // 屏幕坐标 → 窗口坐标（窗口本身全屏，几乎是 1:1）
  // 我们采到的 points.x/y 是 NSEvent logical points (top-left)
  // 窗口也是全屏所以可以直接用
  let prev: Pt | null = null;
  for (const p of points) {
    if (prev) {
      const drawing = p.drawing || prev.drawing;
      // v0.5.x · 用户只要「圈选」(按住左键拖动的粉色标注)，不要移动时的灰线。
      //   所以只在 drawing 时画粉色；移动段(else)直接跳过，不再画灰线。
      if (drawing) {
        // 粉红主线 + halo
        ctx.strokeStyle = "rgba(232, 99, 140, 0.25)";
        ctx.lineWidth = 10;
        ctx.lineCap = "round";
        ctx.beginPath();
        ctx.moveTo(prev.x, prev.y);
        ctx.lineTo(p.x, p.y);
        ctx.stroke();

        ctx.strokeStyle = "rgba(232, 99, 140, 1)";
        ctx.lineWidth = 4;
        ctx.beginPath();
        ctx.moveTo(prev.x, prev.y);
        ctx.lineTo(p.x, p.y);
        ctx.stroke();
      }
      // else: 移动段不画（去掉灰色轨迹线）
    }
    if (p.drawing) {
      // 端点圆
      ctx.fillStyle = "rgba(232, 99, 140, 1)";
      ctx.beginPath();
      ctx.arc(p.x, p.y, 4.5, 0, Math.PI * 2);
      ctx.fill();
    }
    prev = p;
  }
}
