#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Rob Ranslam
#
# Builds Unswiper-x86_64.AppImage with linuxdeploy + its GTK and GStreamer plugins.
# Build on the oldest distro you support (CI: Ubuntu 24.04 = GTK 4.14, libadwaita 1.5).
#
# Bundled: GTK 4, libadwaita, libheif + libde265, GStreamer core and the plugins needed for video
# thumbnails (base, good, libav for H.264/HEVC). The host only needs glibc, FUSE 2 (libfuse2) and a
# display server. Wayland is used natively (GDK_BACKEND is NOT forced to x11).
#
# Needs (Ubuntu 24.04): cargo, libgtk-4-dev, libadwaita-1-dev, libheif-dev, libgstreamer1.0-dev,
#   libgstreamer-plugins-base1.0-dev, gstreamer1.0-plugins-base, gstreamer1.0-plugins-good,
#   gstreamer1.0-plugins-bad (parsers), gstreamer1.0-libav, librsvg2-common (SVG icon loader),
#   dpkg-dev (used by the GTK plugin), wget, file, patchelf.
# Output: $OUT_DIR (default target/appimage)/Unswiper-x86_64.AppImage
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
root="$(cd "$here/../.." && pwd)"
id=io.github.rranslam.Unswiper
out="${OUT_DIR:-$root/target/appimage}"
tools="$out/tools"
mkdir -p "$tools"

cargo build --release --locked -p unswiper-gtk --manifest-path "$root/Cargo.toml"
target_dir="${CARGO_TARGET_DIR:-$root/target}"
bin="$target_dir/release/unswiper"
version="$("$bin" --version | cut -d' ' -f2)"

fetch() { [ -s "$tools/$1" ] || { wget -q -O "$tools/$1" "$2"; chmod +x "$tools/$1"; }; }
fetch linuxdeploy-x86_64.AppImage https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage
fetch linuxdeploy-plugin-gtk.sh https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh
fetch linuxdeploy-plugin-gstreamer.sh https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gstreamer/master/linuxdeploy-plugin-gstreamer.sh

appdir="$out/AppDir"
rm -rf "$appdir"
install -Dm755 "$bin" "$appdir/usr/bin/unswiper"
install -Dm644 "$here/data/$id.desktop" "$appdir/usr/share/applications/$id.desktop"
install -Dm644 "$here/data/$id.metainfo.xml" "$appdir/usr/share/metainfo/$id.metainfo.xml"
# App icon: scalable, symbolic and PNG sizes 16-512.
(cd "$here/data/icons" && find hicolor -type f) | while read -r f; do
  install -Dm644 "$here/data/icons/$f" "$appdir/usr/share/icons/$f"
done

cd "$out"
export PATH="$tools:$PATH" DEPLOY_GTK_VERSION=4 APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=1
gst_dir="$(pkg-config --variable=pluginsdir gstreamer-1.0)"
mkdir -p "$out/gst-plugins"
rm -f "$out/gst-plugins"/*
# Only what video thumbnails need: uridecodebin/playback, typefind, demuxers, convert/scale,
# appsink, and the libav (FFmpeg) decoders.
for p in coreelements typefindfunctions playback app videoconvertscale videoconvert videoscale \
         isomp4 matroska audioparsers videoparsersbad libav; do
  for f in "$gst_dir/libgst$p.so"; do [ -e "$f" ] && ln -sf "$f" "$out/gst-plugins/"; done
done
export GSTREAMER_PLUGINS_DIR="$out/gst-plugins"
export GSTREAMER_HELPERS_DIR="$(pkg-config --variable=pluginscannerdir gstreamer-1.0)"

linuxdeploy-x86_64.AppImage --appdir "$appdir" --plugin gtk --plugin gstreamer \
  --desktop-file "$appdir/usr/share/applications/$id.desktop" \
  --icon-file "$appdir/usr/share/icons/hicolor/256x256/apps/$id.png"

# The GTK plugin's AppRun hook forces GDK_BACKEND=x11 ("Crash with Wayland"). GTK 4 is fine on
# Wayland (Hyprland, GNOME), so drop that line and let GTK pick the backend.
sed -i '/GDK_BACKEND=x11/d' "$appdir"/apprun-hooks/*gtk*.sh
! grep -rn 'GDK_BACKEND' "$appdir/apprun-hooks/"

rm -f "$out"/Unswiper*.AppImage
VERSION="$version" LDAI_OUTPUT="Unswiper-x86_64.AppImage" \
  linuxdeploy-x86_64.AppImage --appdir "$appdir" --output appimage
ls -l "$out/Unswiper-x86_64.AppImage"
