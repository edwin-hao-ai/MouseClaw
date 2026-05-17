#!/usr/bin/env bash
# Notarize an already-signed MouseClaw dmg.
#
# Default uses the **OCTAgentNotary** keychain profile (shared with our other
# projects — Awareness / OCT). Override via $MOUSECLAW_NOTARY_PROFILE.
#
# One-time setup (Apple Developer Program account required, $99/yr):
#   1. Generate an app-specific password at appleid.apple.com → Sign-In
#      and Security → App-Specific Passwords. Format: xxxx-xxxx-xxxx-xxxx
#   2. Team ID lives at developer.apple.com → Membership (10-char string).
#   3. Store in Keychain (so xcrun reads it without prompting):
#        xcrun notarytool store-credentials "OCTAgentNotary" \
#            --apple-id "120298858@qq.com" \
#            --team-id  "5XNDF727Y6" \
#            --password "<app-specific-password>"
#   4. Verify it works:  xcrun notarytool history --keychain-profile OCTAgentNotary
#
# Then just run: bash scripts/notarize-dmg.sh [path/to/dmg]
# Defaults to the most recently-built dmg in project root.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Auto-pick newest MouseClaw_*.dmg if no arg
DEFAULT_DMG="$(ls -t "$ROOT"/MouseClaw_*_aarch64.dmg 2>/dev/null | head -1 || true)"
DMG="${1:-$DEFAULT_DMG}"
if [ -z "${DMG:-}" ]; then
  echo "❌ No dmg found in $ROOT. Run \`bun tauri build\` or pass a path."
  exit 1
fi
DMG="$(realpath "$DMG")"
# Profile priority (override via $MOUSECLAW_NOTARY_PROFILE):
#   1. OCTAgentNotary       (Beijing VGO Co;Ltd / 5XNDF727Y6, 120298858@qq.com)
#   2. AwarenessClawNotary  (same team, set up earlier for the Awareness project)
PROFILE="${MOUSECLAW_NOTARY_PROFILE:-}"
if [ -z "$PROFILE" ]; then
  for candidate in OCTAgentNotary AwarenessClawNotary mouseclaw; do
    if xcrun notarytool history --keychain-profile "$candidate" >/dev/null 2>&1; then
      PROFILE="$candidate"; break
    fi
  done
fi
if [ -z "$PROFILE" ]; then
  cat >&2 <<'HINT'
❌ No notarization keychain profile found. Set one up:

   xcrun notarytool store-credentials "OCTAgentNotary" \
     --apple-id "120298858@qq.com" \
     --team-id  "5XNDF727Y6" \
     --password "<app-specific-password from appleid.apple.com>"

Then re-run this script.
HINT
  exit 1
fi

if [ ! -f "$DMG" ]; then
  echo "❌ dmg not found: $DMG"
  echo "   Run \`bun tauri build\` first, or pass a path."
  exit 1
fi

echo "🦞 Notarizing: $DMG"
echo "   Keychain profile: $PROFILE"
echo ""

# 1. Verify it's signed first (notarize bounces unsigned binaries)
echo "── 1/4 · Verify Developer ID signature ──"
codesign -dv --verbose=2 "$DMG" 2>&1 | grep -E "Authority|Identifier" | head -3 || {
  echo "❌ Not signed with Developer ID. Did tauri.conf.json have signingIdentity?"
  exit 1
}
echo ""

# 2. Submit to Apple notarization service
echo "── 2/4 · Submit to Apple notary (this takes 1–10 min) ──"
SUBMIT_OUT=$(xcrun notarytool submit "$DMG" \
  --keychain-profile "$PROFILE" \
  --wait \
  --output-format json 2>&1)
echo "$SUBMIT_OUT" | head -30

# 3. Extract submission ID + status
STATUS=$(echo "$SUBMIT_OUT" | grep -oE '"status": *"[^"]*"' | head -1 | sed 's/.*"\([^"]*\)"$/\1/')
SUBMIT_ID=$(echo "$SUBMIT_OUT" | grep -oE '"id": *"[^"]*"' | head -1 | sed 's/.*"\([^"]*\)"$/\1/')

if [ "$STATUS" != "Accepted" ]; then
  echo ""
  echo "❌ Notarization failed (status: $STATUS)"
  echo "   Submission id: $SUBMIT_ID"
  echo "   Get details: xcrun notarytool log $SUBMIT_ID --keychain-profile $PROFILE"
  exit 1
fi
echo "✅ Notarization Accepted (id: $SUBMIT_ID)"
echo ""

# 4. Staple the notarization ticket to the dmg
echo "── 3/4 · Staple ticket to dmg ──"
xcrun stapler staple "$DMG"
echo ""

echo "── 4/4 · Verify ──"
xcrun stapler validate "$DMG"
spctl --assess --type install -vv "$DMG" 2>&1 | head -3
echo ""
echo "✅ All set. $DMG is now notarized + stapled."
echo "   Users won't see the Gatekeeper warning anymore."
