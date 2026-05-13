/**
 * Pixel mouse character — single 16×16 SVG, anatomy locked per DESIGN.md §3.1.
 * State changes are pure className swaps; never inline new shapes.
 *
 * Mandatory rendering:
 *   - image-rendering: pixelated
 *   - shape-rendering: crispEdges
 *   - size must be an integer multiple of 16 (16/32/48/64/96/128)
 */
import "./PixelMouse.css";

export type MouseState = "sleep" | "listen" | "think" | "write" | "jump" | "block";

interface PixelMouseProps {
  state: MouseState;
  size?: 32 | 48 | 64 | 96 | 128;
  /** Show the green chain icon above the head (session-continuation indicator). */
  continuing?: boolean;
}

/** Re-usable shared body shape (rows 1-12, same across all states). */
function MouseBody() {
  return (
    <>
      {/* ears outer (row 1-2) */}
      <rect x="3" y="1" width="2" height="2" fill="var(--m-body)" />
      <rect x="2" y="2" width="3" height="2" fill="var(--m-body)" />
      <rect x="11" y="1" width="2" height="2" fill="var(--m-body)" />
      <rect x="11" y="2" width="3" height="2" fill="var(--m-body)" />
      {/* ears inner pink (row 2) */}
      <rect x="3" y="2" width="1" height="1" fill="var(--m-ear-in)" />
      <rect x="12" y="2" width="1" height="1" fill="var(--m-ear-in)" />
      {/* head top */}
      <rect x="3" y="3" width="10" height="1" fill="var(--m-body)" />
      {/* head body (row 4-6) */}
      <rect x="2" y="4" width="12" height="3" fill="var(--m-body)" />
      {/* chin row 7 */}
      <rect x="3" y="7" width="10" height="1" fill="var(--m-body)" />
      {/* belly (row 8-9) */}
      <rect x="5" y="8" width="6" height="2" fill="var(--m-belly)" />
      <rect x="3" y="8" width="2" height="2" fill="var(--m-body)" />
      <rect x="11" y="8" width="2" height="2" fill="var(--m-body)" />
      {/* bottom row 10 */}
      <rect x="4" y="10" width="8" height="1" fill="var(--m-body)" />
      {/* paws row 11 */}
      <rect x="4" y="11" width="2" height="1" fill="var(--m-paw)" />
      <rect x="10" y="11" width="2" height="1" fill="var(--m-paw)" />
      {/* tail (right side) */}
      <rect x="13" y="9" width="1" height="1" fill="var(--m-tail)" />
      <rect x="14" y="7" width="1" height="3" fill="var(--m-tail)" />
      {/* nose row 7 */}
      <rect x="7" y="7" width="2" height="1" fill="var(--m-nose)" />
    </>
  );
}

function Eyes({ state }: { state: MouseState }) {
  if (state === "sleep" || state === "write") {
    // closed/squint — small 1-px dots lower row
    return (
      <>
        <rect x="5" y="6" width="1" height="1" fill="var(--m-eye)" />
        <rect x="10" y="6" width="1" height="1" fill="var(--m-eye)" />
      </>
    );
  }
  if (state === "jump") {
    // closed crescent (smiling eyes) — 2 stacked pixels
    return (
      <>
        <rect x="5" y="5" width="1" height="1" fill="var(--m-eye)" />
        <rect x="6" y="6" width="1" height="1" fill="var(--m-eye)" />
        <rect x="9" y="6" width="1" height="1" fill="var(--m-eye)" />
        <rect x="10" y="5" width="1" height="1" fill="var(--m-eye)" />
      </>
    );
  }
  // listen / think / block — alert 2-px eyes
  return (
    <>
      <rect x="5" y="5" width="1" height="2" fill="var(--m-eye)" />
      <rect x="10" y="5" width="1" height="2" fill="var(--m-eye)" />
    </>
  );
}

function Extras({ state }: { state: MouseState }) {
  if (state === "listen") {
    // mouth open dot under nose
    return <rect x="7" y="9" width="2" height="1" fill="var(--m-eye)" />;
  }
  if (state === "think") {
    // pink ? mark over right ear
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
    // yellow pencil in left paw (extends below)
    return (
      <>
        <rect x="3" y="12" width="1" height="3" fill="var(--warn)" />
        <rect x="3" y="15" width="1" height="1" fill="var(--m-eye)" />
      </>
    );
  }
  if (state === "block") {
    // red ! over head
    return (
      <>
        <rect x="7" y="-2" width="2" height="2" fill="var(--danger)" />
        <rect x="7" y="1" width="2" height="1" fill="var(--danger)" />
      </>
    );
  }
  return null;
}

export function PixelMouse({ state, size = 96, continuing = false }: PixelMouseProps) {
  return (
    <div className={`mouse-wrap mouse-${state}`} style={{ width: size, height: size }}>
      {continuing && <div className="mouse-chain" aria-hidden />}
      <svg
        viewBox="-1 -2 18 18"
        width={size}
        height={size}
        className="mouse-svg"
        role="img"
        aria-label={`MouseClaw, ${state} state`}
      >
        <MouseBody />
        <Eyes state={state} />
        <Extras state={state} />
      </svg>
    </div>
  );
}
