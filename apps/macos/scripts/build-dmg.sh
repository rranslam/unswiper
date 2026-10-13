#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Builds Unswiper.app (release, universal if possible), ad-hoc signs it,
# and packages dist/Unswiper-<version>.dmg.
# Usage: apps/macos/scripts/build-dmg.sh [version]
set -euo pipefail

APP_NAME="Unswiper"
BUNDLE_ID="com.rranslam.unswiper"
VERSION="${1:-${VERSION:-0.1.1}}"
VERSION="${VERSION#v}"
MIN_MACOS="14.0"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PKG_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
DIST="$PKG_DIR/dist"
APP="$DIST/$APP_NAME.app"
DMG="$DIST/$APP_NAME-$VERSION.dmg"

cd "$PKG_DIR"
echo "==> Building $APP_NAME $VERSION (release)"
if swift build -c release --arch arm64 --arch x86_64; then
  BIN_DIR="$(swift build -c release --arch arm64 --arch x86_64 --show-bin-path)"
else
  echo "!! Universal build failed; falling back to arm64 only"
  swift build -c release --arch arm64
  BIN_DIR="$(swift build -c release --arch arm64 --show-bin-path)"
fi

echo "==> Assembling $APP"
rm -rf "$DIST"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN_DIR/$APP_NAME" "$APP/Contents/MacOS/$APP_NAME"
# Copy any SwiftPM resource bundles next to the binary.
find "$BIN_DIR" -maxdepth 1 -name '*.bundle' -exec cp -R {} "$APP/Contents/Resources/" \;
if [ ! -d "$APP/Contents/Resources/${APP_NAME}_${APP_NAME}.bundle" ]; then
  echo "!! Resource bundle ${APP_NAME}_${APP_NAME}.bundle missing (Help would be unavailable)" >&2
  exit 1
fi

ICON_KEY=""
if [ -f "$PKG_DIR/Resources/AppIcon.icns" ]; then
  cp "$PKG_DIR/Resources/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
  ICON_KEY="<key>CFBundleIconFile</key><string>AppIcon</string>"
fi

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleExecutable</key><string>$APP_NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>LSMinimumSystemVersion</key><string>$MIN_MACOS</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.photography</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSPrincipalClass</key><string>NSApplication</string>
  <key>NSHumanReadableCopyright</key><string>Copyright (c) 2026 Rob Ranslam. MIT License.</string>
  $ICON_KEY
</dict>
</plist>
PLIST
plutil -lint "$APP/Contents/Info.plist"

echo "==> Ad-hoc signing"
codesign --force --deep -s - "$APP"
codesign --verify --deep --strict "$APP"

echo "==> Creating $DMG"
STAGE="$(mktemp -d)"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
cp "$PKG_DIR/../../LICENSE" "$STAGE/LICENSE.txt" 2>/dev/null || true
hdiutil create -volname "$APP_NAME $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
rm -rf "$STAGE"

echo "==> Done: $DMG"
lipo -info "$APP/Contents/MacOS/$APP_NAME" || true
echo "==> Help self-test"
"$APP/Contents/MacOS/$APP_NAME" --help-selftest
