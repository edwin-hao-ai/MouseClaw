/**
 * Day 1 占位 UI：纯像素老鼠（睡眠状态），透明背景。
 * 真正的 4 状态动画 + 气泡 + panel 在 Day 2+。
 */
import "./App.css";

const MOUSE_PALETTE = {
  body: "#cfcfcf",
  belly: "#ffffff",
  earIn: "#ff9bb8",
  eye: "#1a1a1a",
  nose: "#d63d6a",
  tail: "#8a8a8a",
  paw: "#ffffff",
};

function PixelMouse() {
  const p = MOUSE_PALETTE;
  return (
    <svg
      viewBox="0 0 16 16"
      width="128"
      height="128"
      style={{
        imageRendering: "pixelated",
        shapeRendering: "crispEdges",
        filter: "drop-shadow(0 4px 8px rgba(0,0,0,0.4))",
      }}
    >
      {/* ears outer */}
      <rect x="3" y="1" width="2" height="2" fill={p.body} />
      <rect x="2" y="2" width="3" height="2" fill={p.body} />
      <rect x="11" y="1" width="2" height="2" fill={p.body} />
      <rect x="11" y="2" width="3" height="2" fill={p.body} />
      {/* ears inner */}
      <rect x="3" y="2" width="1" height="1" fill={p.earIn} />
      <rect x="12" y="2" width="1" height="1" fill={p.earIn} />
      {/* body */}
      <rect x="3" y="3" width="10" height="1" fill={p.body} />
      <rect x="2" y="4" width="12" height="3" fill={p.body} />
      <rect x="3" y="7" width="10" height="1" fill={p.body} />
      <rect x="5" y="8" width="6" height="2" fill={p.belly} />
      <rect x="3" y="8" width="2" height="2" fill={p.body} />
      <rect x="11" y="8" width="2" height="2" fill={p.body} />
      <rect x="4" y="10" width="8" height="1" fill={p.body} />
      <rect x="4" y="11" width="2" height="1" fill={p.paw} />
      <rect x="10" y="11" width="2" height="1" fill={p.paw} />
      {/* sleeping eyes (small dots) */}
      <rect x="5" y="6" width="1" height="1" fill={p.eye} />
      <rect x="10" y="6" width="1" height="1" fill={p.eye} />
      {/* nose */}
      <rect x="7" y="7" width="2" height="1" fill={p.nose} />
      {/* tail */}
      <rect x="13" y="9" width="1" height="1" fill={p.tail} />
      <rect x="14" y="7" width="1" height="3" fill={p.tail} />
    </svg>
  );
}

function App() {
  return (
    <div className="mouse-stage">
      <PixelMouse />
    </div>
  );
}

export default App;
