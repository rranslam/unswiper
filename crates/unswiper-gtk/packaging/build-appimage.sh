#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Rob Ranslam
#
# Builds Unswiper-<version>-x86_64.AppImage with linuxdeploy + its GTK plugin.
# Build on the oldest distro you support (Ubuntu 24.04 baseline: GTK 4.14, libadwaita 1.5).
# Needs: cargo, libgtk-4-dev, libadwaita-1-dev, libheif-dev, wget, file.
# ffmpeg is not bundled; video thumbnails use the system ffmpeg when present.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
id=io.github.rranslam.Unswiper
out="${OUT_DIR:-$root/target/appimage}"
tools="$out/tools"
mkdir -p "$tools"

cargo build --release -p unswiper-gtk --manifest-path "$root/Cargo.toml"
version="$("$root/target/release/unswiper" --version | cut -d' ' -f2)"

fetch() { [ -x "$tools/$1" ] || { wget -q -O "$tools/$1" "$2"; chmod +x "$tools/$1"; }; }
fetch linuxdeploy-x86_64.AppImage https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage
fetch linuxdeploy-plugin-gtk.sh https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh

appdir="$out/AppDir"
rm -rf "$appdir"
install -Dm755 "$root/target/release/unswiper" "$appdir/usr/bin/unswiper"
install -Dm644 "$here/data/$id.desktop" "$appdir/usr/share/applications/$id.desktop"
install -Dm644 "$here/data/$id.metainfo.xml" "$appdir/usr/share/metainfo/$id.metainfo.xml"
install -Dm644 "$here/data/icons/hicolor/scalable/apps/$id.svg" "$appdir/usr/share/icons/hicolor/scalable/apps/$id.svg"

cd "$out"
export DEPLOY_GTK_VERSION=4 VERSION="$version" APPIMAGE_EXTRACT_AND_RUN=1
"$tools/linuxdeploy-x86_64.AppImage" --appdir "$appdir" --plugin gtk \
  --desktop-file "$appdir/usr/share/applications/$id.desktop" \
  --icon-file "$appdir/usr/share/icons/hicolor/scalable/apps/$id.svg" \
  --output appimage
ls -l "$out"/*.AppImage
