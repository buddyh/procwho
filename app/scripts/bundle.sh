#!/bin/bash
# Builds ProcwhoBar in release mode and assembles app/build/Procwho.app (menu bar only,
# LSUIElement), ad-hoc signed. Copies ../target/release/procwho into Contents/Resources when
# it exists, so the app finds the CLI without PROCWHO_BIN.
#
# Usage: app/scripts/bundle.sh [--verify] (from anywhere)
# --verify writes a separate bundle without replacing the running/development app.
set -euo pipefail

APP_DIR="$(cd "$(dirname "$0")/.." && pwd)"
REPO_DIR="$(cd "$APP_DIR/.." && pwd)"
VERSION="$(tr -d '[:space:]' < "$APP_DIR/VERSION")"
BUNDLE_ID="dev.procwho.app"
OUT="$APP_DIR/build/Procwho.app"
if [[ "${1:-}" == "--verify" && "$#" == 1 ]]; then
    OUT="$APP_DIR/build/verification/Procwho.app"
elif [[ "$#" != 0 ]]; then
    echo "Usage: $0 [--verify]" >&2
    exit 2
fi

echo "building ProcwhoBar $VERSION (release)"
swift build -c release --package-path "$APP_DIR"
BIN_DIR="$(swift build -c release --package-path "$APP_DIR" --show-bin-path)"

rm -rf "$OUT"
mkdir -p "$OUT/Contents/MacOS" "$OUT/Contents/Resources"
cp "$BIN_DIR/ProcwhoBar" "$OUT/Contents/MacOS/ProcwhoBar"

CLI="$REPO_DIR/target/release/procwho"
if [[ -x "$CLI" ]]; then
    cp "$CLI" "$OUT/Contents/Resources/procwho"
    echo "bundled CLI: $CLI"
else
    echo "note: $CLI not found; the app will look for PROCWHO_BIN or ~/.local/bin/procwho"
fi

cat > "$OUT/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleExecutable</key><string>ProcwhoBar</string>
    <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>Procwho</string>
    <key>CFBundleDisplayName</key><string>Procwho</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>LSUIElement</key><true/>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# Sign the nested CLI first, then the bundle (ad hoc, no identity).
if [[ -f "$OUT/Contents/Resources/procwho" ]]; then
    codesign --force -s - "$OUT/Contents/Resources/procwho"
fi
codesign --force -s - "$OUT"
codesign --verify --strict "$OUT"
echo "built $OUT"
