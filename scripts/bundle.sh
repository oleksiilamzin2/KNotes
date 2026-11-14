#!/bin/sh
# Builds target/<profile>/Knotes.app around the knotes binary (no Xcode needed, ADR 0010),
# with the MCP server knotes-mcp beside it (ADR 0027).
# Usage: scripts/bundle.sh [debug|release]
set -eu
profile="${1:-release}"
cd "$(dirname "$0")/.."
if [ "$profile" = release ]; then cargo build -q --release -p knotes -p knotes-mcp; else cargo build -q -p knotes -p knotes-mcp; fi
app="target/$profile/Knotes.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "target/$profile/knotes" "$app/Contents/MacOS/knotes"
cp "target/$profile/knotes-mcp" "$app/Contents/MacOS/knotes-mcp"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Knotes</string>
  <key>CFBundleDisplayName</key><string>Knotes</string>
  <key>CFBundleIdentifier</key><string>app.knotes.Knotes</string>
  <key>CFBundleExecutable</key><string>knotes</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>27.0</string>
  <key>NSPrincipalClass</key><string>NSApplication</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticTermination</key><false/>
</dict>
</plist>
PLIST
codesign --force --sign - "$app" 2>/dev/null || true
echo "$app"
