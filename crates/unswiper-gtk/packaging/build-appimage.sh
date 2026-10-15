#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Rob Ranslam
#
# Builds Unswiper-$ARCH.AppImage (ARCH = uname -m: x86_64 or aarch64; build natively on that arch) with linuxdeploy + its GTK and GStreamer plugins.
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
# Output: $OUT_DIR (default target/appimage)/Unswiper-<arch>.AppImage
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

ARCH="${ARCH:-$(uname -m)}"
case "$ARCH" in x86_64|aarch64) ;; *) echo "unsupported arch $ARCH" >&2; exit 1;; esac
fetch() { [ -s "$tools/$1" ] || { wget -q -O "$tools/$1" "$2"; chmod +x "$tools/$1"; }; }
fetch linuxdeploy-$ARCH.AppImage https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-$ARCH.AppImage
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

linuxdeploy-$ARCH.AppImage --appdir "$appdir" --plugin gtk --plugin gstreamer \
  --desktop-file "$appdir/usr/share/applications/$id.desktop" \
  --icon-file "$appdir/usr/share/icons/hicolor/256x256/apps/$id.png"

# The GTK plugin's AppRun hook forces GDK_BACKEND=x11 ("Crash with Wayland"). GTK 4 is fine on
# Wayland (Hyprland, GNOME), so drop that line and let GTK pick the backend.
sed -i '/GDK_BACKEND=x11/d' "$appdir"/apprun-hooks/*gtk*.sh
if grep -rn 'GDK_BACKEND' "$appdir/apprun-hooks/"; then echo "GDK_BACKEND still set" >&2; exit 1; fi
# It also exports GTK_THEME=Adwaita:<light|dark>. With libadwaita that loads GTK's own built-in
# Adwaita theme over libadwaita's stylesheet: dialogs lose their rounded sheet, shadow and backdrop
# dimming (they blend into the photo grid), and the system dark/light setting is ignored.
# libadwaita picks the colour scheme from the settings portal itself, so drop the export.
sed -i '/export GTK_THEME=/d' "$appdir"/apprun-hooks/*gtk*.sh
if grep -rn 'export GTK_THEME=' "$appdir/apprun-hooks/"; then echo "GTK_THEME still set" >&2; exit 1; fi

# Keep host modules out of the bundled GLib/GStreamer. The bundle carries GLib from the build distro
# (Ubuntu 24.04); newer hosts' GVFS GIO modules need newer GLib symbols ("libgvfscommon.so: undefined
# symbol: g_variant_builder_init_static"). Unswiper only needs local files (it uses the freedesktop
# Trash directly and portals over D-Bus), so load no GIO modules and use the local VFS.
# GStreamer and gdk-pixbuf already point at the bundle (plugin hooks); also pin the unversioned
# GStreamer path so a host GST_PLUGIN_SYSTEM_PATH can't add host plugins.
hook="$appdir/apprun-hooks/linuxdeploy-plugin-gtk.sh"
if ! grep -q 'GIO_MODULE_DIR' "$hook"; then
  mkdir -p "$appdir/usr/lib/gio/modules"
  cat >> "$hook" <<'HOOK'
export GIO_MODULE_DIR="$APPDIR/usr/lib/gio/modules" # bundled, empty: no host GVFS/dconf modules
export GIO_USE_VFS=local
export GST_PLUGIN_SYSTEM_PATH="$APPDIR/usr/lib/gstreamer-1.0"
HOOK
fi

rm -f "$out"/Unswiper*.AppImage
VERSION="$version" LDAI_OUTPUT="Unswiper-$ARCH.AppImage" \
  linuxdeploy-$ARCH.AppImage --appdir "$appdir" --output appimage
ls -l "$out/Unswiper-$ARCH.AppImage"
