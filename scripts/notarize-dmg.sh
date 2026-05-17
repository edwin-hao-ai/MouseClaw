#!/usr/bin/env bash
# Notarize an already-signed MouseClaw dmg.
#
# Prereqs (run once):
#   1. Get an Apple Developer Program account ($99/yr).
#   2. Create an app-specific password at appleid.apple.com → Sign-In and Security.
#   3. Find your Team ID at developer.apple.com → Membership.
#   4. Store the creds in Keychain so we never type them again:
#      xcrun notarytool store-credentials mouseclaw \
#          --apple-id "you@example.com" \
#          --team-id "ABCDE12345" \
#          --password "abcd-efgh-ijkl-mnop"
#   5. Verify it works: xcrun notarytool history --keychain-profile mouseclaw
#
# Then just run: bash scripts/notarize-dmg.sh [path/to/dmg]
# Defaults to project-root MouseClaw_0.1.0_aarch64.dmg.

set -euo pipefail

DMG="${1:-$(dirname "$0")/../MouseClaw_0.1.0_aarch64.dmg}"
DMG="$(realpath "$DMG")"
PROFILE="${MOUSECLAW_NOTARY_PROFILE:-mouseclaw}"

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
