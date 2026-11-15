#!/bin/sh
# Builds target/<profile>/Knotes.app around the knotes binary (no Xcode needed, ADR 0010),
# with the MCP server knotes-mcp beside it (ADR 0027) and the helper that runs the embedding
# models, knotes-embed, with the models in Resources (ADR 0028, 0029; scripts/setup.sh fetches
# them), the
# licenses passing it on requires in Resources/Licenses, and Credits.rtf for the About window.
# Usage: scripts/bundle.sh [debug|release]
set -eu
profile="${1:-release}"
cd "$(dirname "$0")/.."
models="models/multilingual-e5-large-instruct-q8_0.gguf models/embeddinggemma-2-Q8_0.gguf"
for model in $models; do
  if [ ! -f "$model" ]; then
    echo "bundle: no $model: run scripts/setup.sh" >&2
    exit 1
  fi
done
packages="-p knotes -p knotes-mcp"
if [ "$profile" = release ]; then cargo build -q --release $packages; else cargo build -q $packages; fi
# The embedding helper is always built optimized: in a debug build llama.cpp's own code is too.
cargo build -q --release -p knotes-embed-helper
app="target/$profile/Knotes.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "target/$profile/knotes" "$app/Contents/MacOS/knotes"
cp "target/$profile/knotes-mcp" "$app/Contents/MacOS/knotes-mcp"
cp "target/release/knotes-embed" "$app/Contents/MacOS/knotes-embed"
for model in $models; do cp "$model" "$app/Contents/Resources/"; done
mkdir -p "$app/Contents/Resources/Licenses"
cp licenses/*.txt "$app/Contents/Resources/Licenses/"
cp licenses/Credits.rtf "$app/Contents/Resources/"
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
