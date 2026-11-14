#!/bin/sh
# Builds target/<profile>/Knotes.app around the knotes binary (no Xcode needed, ADR 0010),
# with the MCP server knotes-mcp beside it (ADR 0027) and the helper that runs the embedding
# model, knotes-embed, with the model in Resources (ADR 0028; scripts/setup.sh fetches it).
# Usage: scripts/bundle.sh [debug|release]
set -eu
profile="${1:-release}"
cd "$(dirname "$0")/.."
model=models/embeddinggemma-300M-Q8_0.gguf
if [ ! -f "$model" ]; then
  echo "bundle: no $model: run scripts/setup.sh" >&2
  exit 1
fi
packages="-p knotes -p knotes-mcp -p knotes-embed-helper"
if [ "$profile" = release ]; then cargo build -q --release $packages; else cargo build -q $packages; fi
app="target/$profile/Knotes.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "target/$profile/knotes" "$app/Contents/MacOS/knotes"
cp "target/$profile/knotes-mcp" "$app/Contents/MacOS/knotes-mcp"
cp "target/$profile/knotes-embed" "$app/Contents/MacOS/knotes-embed"
cp "$model" "$app/Contents/Resources/"
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
# A stable signature when the "Knotes Dev" certificate is in the keychain: macOS then knows each
# build as the same app (the Keychain keeps its "Always Allow"); else ad hoc, new every build.
identity=-
if security find-identity -p codesigning 2>/dev/null | grep -q '"Knotes Dev"'; then
  identity="Knotes Dev"
fi
codesign --force --sign "$identity" "$app/Contents/MacOS/knotes-mcp" 2>/dev/null || true
codesign --force --sign "$identity" "$app/Contents/MacOS/knotes-embed" 2>/dev/null || true
codesign --force --sign "$identity" "$app" 2>/dev/null || true
echo "$app"
