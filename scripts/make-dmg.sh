#!/usr/bin/env bash
# Build a distributable, drag-to-install .dmg from the release .app.
#
# We build the DMG with hdiutil rather than Tauri's bundle_dmg.sh because the
# latter drives Finder via AppleScript to style the window, which fails in
# headless / CI / non-interactive sessions. This produces a clean compressed
# DMG with an /Applications symlink for drag-to-install.
#
# Usage:
#   npm run tauri build          # produces the .app
#   ./scripts/make-dmg.sh        # wraps it into a .dmg
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="$ROOT/src-tauri/target/release/bundle/macos/beta2clocks.app"
VERSION="$(grep -m1 '"version"' "$ROOT/src-tauri/tauri.conf.json" | sed -E 's/.*"version" *: *"([^"]+)".*/\1/')"
ARCH="$(uname -m)"   # arm64 / x86_64
OUT_DIR="$ROOT/src-tauri/target/release/bundle/dmg"
OUT="$OUT_DIR/beta2clocks_${VERSION}_${ARCH}.dmg"

if [[ ! -d "$APP" ]]; then
  echo "error: $APP not found — run 'npm run tauri build' first." >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

cp -R "$APP" "$STAGE/"

# Tauri's default ad-hoc signature can be inconsistent ("code has no resources
# but signature indicates they must be present"), which makes macOS report the
# downloaded app as "damaged". Clear stray xattrs and re-sign ad-hoc so the
# bundle has a valid, self-consistent signature. (Downloaded copies will still
# be quarantined — users right-click → Open, or run the documented xattr step —
# until the app is Developer-ID signed + notarized.)
xattr -cr "$STAGE/beta2clocks.app" 2>/dev/null || true
codesign --force --deep --sign - "$STAGE/beta2clocks.app"
codesign --verify --deep --strict "$STAGE/beta2clocks.app" || { echo "error: re-signed bundle failed verification" >&2; exit 1; }

ln -s /Applications "$STAGE/Applications"

rm -f "$OUT"
hdiutil create -volname "beta2clocks" -srcfolder "$STAGE" -ov -format UDZO "$OUT" >/dev/null

echo "Built $OUT"
