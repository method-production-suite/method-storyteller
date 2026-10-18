#!/usr/bin/env bash
# Remakes every app icon in this folder from mark.svg, on a Mac (swift, sips, iconutil):
#   icon-1024.png  the source picture         icon.icns  macOS (.app bundle)
#   icon-512.png   Linux (.deb, AppImage)     icon.ico   Windows (the installer)
#   icon-256.png   Linux
# mark.svg is a copy of the provisional mark from the website's brand folder; when the brand
# changes, copy the new mark here and run this again.
set -euo pipefail
cd "$(dirname "$0")"
swift render-icon.swift mark.svg icon-1024.png
sips -z 512 512 icon-1024.png --out icon-512.png > /dev/null
sips -z 256 256 icon-1024.png --out icon-256.png > /dev/null
sips -s format ico icon-256.png --out icon.ico > /dev/null
set=$(mktemp -d)/icon.iconset
mkdir -p "$set"
for px in 16 32 128 256 512; do
  sips -z "$px" "$px" icon-1024.png --out "$set/icon_${px}x${px}.png" > /dev/null
  double=$((px * 2))
  sips -z "$double" "$double" icon-1024.png --out "$set/icon_${px}x${px}@2x.png" > /dev/null
done
iconutil -c icns "$set" -o icon.icns
rm -rf "$(dirname "$set")"
ls -l icon-1024.png icon-512.png icon-256.png icon.ico icon.icns
