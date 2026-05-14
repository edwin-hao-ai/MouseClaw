#!/usr/bin/env bash
# Generate the MouseClaw app icon (pixel mouse character on pink gradient
# rounded-square background) at all macOS-required sizes, then bundle into
# icon.icns via iconutil.
#
# 一次性脚本——更新视觉时改 mouse-source.svg 里的 SVG 然后再跑一次。

set -euo pipefail
cd "$(dirname "$0")/.."
ICONS_DIR=src-tauri/icons
SVG=/tmp/mouseclaw-icon-master.svg
MASTER_PNG=/tmp/mouseclaw-icon-1024.png

cat > "$SVG" <<'EOF'
<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#ffd6e3"/>
      <stop offset="55%" stop-color="#ff9bb8"/>
      <stop offset="100%" stop-color="#ff6b9d"/>
    </linearGradient>
    <radialGradient id="glow" cx="50%" cy="35%" r="60%">
      <stop offset="0%" stop-color="#ffffff" stop-opacity="0.35"/>
      <stop offset="60%" stop-color="#ffffff" stop-opacity="0.05"/>
      <stop offset="100%" stop-color="#ffffff" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <!-- Rounded-square background (macOS Big Sur app icon ratio ~18%) -->
  <rect width="1024" height="1024" rx="180" ry="180" fill="url(#bg)"/>
  <rect width="1024" height="1024" rx="180" ry="180" fill="url(#glow)"/>

  <!-- The pixel mouse (16x16 pixel art, scaled 48x = 768px) -->
  <g transform="translate(128, 128) scale(48)" shape-rendering="crispEdges">
    <!-- ears outer -->
    <rect x="3" y="1" width="2" height="2" fill="#cfcfcf"/>
    <rect x="2" y="2" width="3" height="2" fill="#cfcfcf"/>
    <rect x="11" y="1" width="2" height="2" fill="#cfcfcf"/>
    <rect x="11" y="2" width="3" height="2" fill="#cfcfcf"/>
    <!-- ears inner pink -->
    <rect x="3" y="2" width="1" height="1" fill="#ff9bb8"/>
    <rect x="12" y="2" width="1" height="1" fill="#ff9bb8"/>
    <!-- head -->
    <rect x="3" y="3" width="10" height="1" fill="#cfcfcf"/>
    <rect x="2" y="4" width="12" height="3" fill="#cfcfcf"/>
    <rect x="3" y="7" width="10" height="1" fill="#cfcfcf"/>
    <!-- belly (white) + body sides -->
    <rect x="5" y="8" width="6" height="2" fill="#ffffff"/>
    <rect x="3" y="8" width="2" height="2" fill="#cfcfcf"/>
    <rect x="11" y="8" width="2" height="2" fill="#cfcfcf"/>
    <!-- bottom row -->
    <rect x="4" y="10" width="8" height="1" fill="#cfcfcf"/>
    <!-- paws -->
    <rect x="4" y="11" width="2" height="1" fill="#ffffff"/>
    <rect x="10" y="11" width="2" height="1" fill="#ffffff"/>
    <!-- Cute happy eyes (closed crescent — smiling) -->
    <rect x="5" y="5" width="1" height="1" fill="#1a1a1a"/>
    <rect x="6" y="6" width="1" height="1" fill="#1a1a1a"/>
    <rect x="9" y="6" width="1" height="1" fill="#1a1a1a"/>
    <rect x="10" y="5" width="1" height="1" fill="#1a1a1a"/>
    <!-- nose -->
    <rect x="7" y="7" width="2" height="1" fill="#d63d6a"/>
    <!-- tail -->
    <rect x="13" y="9" width="1" height="1" fill="#8a8a8a"/>
    <rect x="14" y="7" width="1" height="3" fill="#8a8a8a"/>
  </g>
</svg>
EOF

echo "[icon] rendering 1024×1024 master from SVG..."
if command -v rsvg-convert >/dev/null 2>&1; then
    rsvg-convert -w 1024 -h 1024 "$SVG" -o "$MASTER_PNG"
elif command -v convert >/dev/null 2>&1; then
    convert -background none -density 300 "$SVG" -resize 1024x1024 "$MASTER_PNG"
else
    echo "需要 rsvg-convert 或 ImageMagick convert：brew install librsvg" >&2
    exit 1
fi

echo "[icon] generating Tauri-required PNG sizes → $ICONS_DIR"
sips -z 32 32  "$MASTER_PNG" --out "$ICONS_DIR/32x32.png"     >/dev/null
sips -z 128 128 "$MASTER_PNG" --out "$ICONS_DIR/128x128.png"   >/dev/null
sips -z 256 256 "$MASTER_PNG" --out "$ICONS_DIR/128x128@2x.png" >/dev/null

# Windows Square*x* sizes (cross-platform compat)
for sz in 30 44 71 89 107 142 150 284 310; do
    sips -z $sz $sz "$MASTER_PNG" --out "$ICONS_DIR/Square${sz}x${sz}Logo.png" >/dev/null
done
sips -z 50 50 "$MASTER_PNG" --out "$ICONS_DIR/StoreLogo.png" >/dev/null

echo "[icon] composing icon.icns..."
ICONSET=/tmp/mouseclaw.iconset
rm -rf "$ICONSET"
mkdir -p "$ICONSET"
sips -z 16 16    "$MASTER_PNG" --out "$ICONSET/icon_16x16.png"        >/dev/null
sips -z 32 32    "$MASTER_PNG" --out "$ICONSET/icon_16x16@2x.png"     >/dev/null
sips -z 32 32    "$MASTER_PNG" --out "$ICONSET/icon_32x32.png"        >/dev/null
sips -z 64 64    "$MASTER_PNG" --out "$ICONSET/icon_32x32@2x.png"     >/dev/null
sips -z 128 128  "$MASTER_PNG" --out "$ICONSET/icon_128x128.png"      >/dev/null
sips -z 256 256  "$MASTER_PNG" --out "$ICONSET/icon_128x128@2x.png"   >/dev/null
sips -z 256 256  "$MASTER_PNG" --out "$ICONSET/icon_256x256.png"      >/dev/null
sips -z 512 512  "$MASTER_PNG" --out "$ICONSET/icon_256x256@2x.png"   >/dev/null
sips -z 512 512  "$MASTER_PNG" --out "$ICONSET/icon_512x512.png"      >/dev/null
cp "$MASTER_PNG" "$ICONSET/icon_512x512@2x.png"
iconutil -c icns "$ICONSET" -o "$ICONS_DIR/icon.icns"

# Windows .ico (Tauri 2 also wants this for cross-platform stub even on macOS)
if command -v convert >/dev/null 2>&1; then
    convert "$MASTER_PNG" -define icon:auto-resize=256,128,64,48,32,16 "$ICONS_DIR/icon.ico"
fi

# Master also kept for next regen
cp "$MASTER_PNG" "$ICONS_DIR/icon.png"

echo "[icon] ✓ done — see $ICONS_DIR/"
ls -lh "$ICONS_DIR/" | grep -E "(icon\.|^total)"
