#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Builds Unswiper.app (release, universal if possible) with the bundled unswiper-blur helper
# (needs Rust/cargo), ad-hoc signs it,
# and packages dist/Unswiper-<version>.dmg.
# Usage: apps/macos/scripts/build-dmg.sh [version]
set -euo pipefail

APP_NAME="Unswiper"
BUNDLE_ID="io.github.rranslam.Unswiper"  # same as Linux; com.rranslam.unswiper up to 0.1.2
VERSION="${1:-${VERSION:-0.5.0}}"
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

echo "==> Building the blur checker helper (Rust unswiper-blur, no libheif: HEIC goes through sips)"
REPO="$(cd "$PKG_DIR/../.." && pwd)"
HELPER_OUT="$(mktemp -d)/unswiper-blur"
command -v cargo >/dev/null || { echo "!! cargo not found: install Rust from https://rustup.rs" >&2; exit 1; }
built=()
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  rustup target add "$target" >/dev/null 2>&1 || true
  if cargo build --manifest-path "$REPO/Cargo.toml" --release -p unswiper-blur --no-default-features --target "$target"; then
    built+=("$REPO/target/$target/release/unswiper-blur")
  else
    echo "!! unswiper-blur for $target failed"
  fi
done
[ "${#built[@]}" -gt 0 ] || { echo "!! no unswiper-blur build" >&2; exit 1; }
lipo -create -output "$HELPER_OUT" "${built[@]}"

echo "==> Assembling $APP"
rm -rf "$DIST"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN_DIR/$APP_NAME" "$APP/Contents/MacOS/$APP_NAME"
# The app finds the helper next to its executable (BlurHelper.locate).
cp "$HELPER_OUT" "$APP/Contents/MacOS/unswiper-blur"
chmod 755 "$APP/Contents/MacOS/unswiper-blur"
# Copy any SwiftPM resource bundles next to the binary.
find "$BIN_DIR" -maxdepth 1 -name '*.bundle' -exec cp -R {} "$APP/Contents/Resources/" \;
if [ ! -d "$APP/Contents/Resources/${APP_NAME}_${APP_NAME}.bundle" ]; then
  echo "!! Resource bundle ${APP_NAME}_${APP_NAME}.bundle missing (Help would be unavailable)" >&2
  exit 1
fi

# GeoNames (CC BY 4.0) and bundled crates (plan §20.3): shown in About Unswiper, also on the dmg.
cp "$PKG_DIR/../../THIRD_PARTY_NOTICES" "$APP/Contents/Resources/THIRD_PARTY_NOTICES.txt"

# App icon: built from the iconset with Apple's iconutil (Resources/README.md).
ICONSET="$PKG_DIR/Resources/AppIcon.iconset"
if [ ! -d "$ICONSET" ]; then
  echo "!! $ICONSET missing" >&2
  exit 1
fi
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
ICON_KEY="<key>CFBundleIconFile</key><string>AppIcon</string>"

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
cp "$PKG_DIR/../../THIRD_PARTY_NOTICES" "$STAGE/THIRD_PARTY_NOTICES.txt"
hdiutil create -volname "$APP_NAME $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
rm -rf "$STAGE"

echo "==> Done: $DMG"
lipo -info "$APP/Contents/MacOS/$APP_NAME" || true
lipo -info "$APP/Contents/MacOS/unswiper-blur" || true
"$APP/Contents/MacOS/unswiper-blur" --version
echo "==> Takeout helper self-test"
TAKEOUT_TMP="$(mktemp -d)"
[ "$("$APP/Contents/MacOS/unswiper-blur" takeout dates "$TAKEOUT_TMP")" = "[]" ] || { echo "unswiper-blur takeout dates failed"; exit 1; }
rmdir "$TAKEOUT_TMP"
echo "==> Third-party notices"
grep -q "GeoNames" "$APP/Contents/Resources/THIRD_PARTY_NOTICES.txt" || { echo "THIRD_PARTY_NOTICES missing from the app"; exit 1; }
echo "==> Help self-test"
"$APP/Contents/MacOS/$APP_NAME" --help-selftest
