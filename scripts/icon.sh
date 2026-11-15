#!/bin/sh
# Makes the app icon, assets/icon/Knotes.icns, from assets/icon/source.jpeg (scripts/icon.swift
# fits it to Apple's grid at every size; iconutil packs them). Run it after changing the
# picture; bundle.sh copies the .icns into the app.
# Usage: scripts/icon.sh
set -eu
cd "$(dirname "$0")/.."
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
swift scripts/icon.swift assets/icon/source.jpeg "$work/Knotes.iconset"
iconutil -c icns "$work/Knotes.iconset" -o assets/icon/Knotes.icns
echo "assets/icon/Knotes.icns"
