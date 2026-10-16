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
#   libgtk-4-media-gstreamer (in-app player), dpkg-dev (used by the GTK plugin), wget, file, patchelf,
#   gstreamer1.0-tools (gst-inspect-1.0 for the element check), ffmpeg (test AVIs for the decode check).
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
# Video thumbnails and playback: uridecodebin/playbin, typefind, demuxers, convert/scale, appsink,
# the libav (FFmpeg) decoders, and audio output (autodetect picks PipeWire or PulseAudio).
# Containers: MP4/MOV/3GP (isomp4), MKV/WebM (matroska), AVI (avi = avidemux), MTS/M2TS (mpegtsdemux),
# Ogg. Decoders: libav (H.264, HEVC, MPEG-4 Part 2/DivX/Xvid/MS-MPEG4, DV, VP8/VP9, MP3, AAC, AC-3 ...),
# jpeg (jpegdec for MJPEG AVIs), dv (dvdec), mpg123, vorbis, opus. check-appimage-gst.sh asserts the key ones.
for p in coreelements typefindfunctions playback app videoconvertscale videoconvert videoscale \
         isomp4 matroska avi mpegtsdemux ogg audioparsers videoparsersbad libav jpeg dv mpg123 \
         vorbis opus wavparse autodetect audioconvert audioresample volume pulseaudio pipewire; do
  for f in "$gst_dir/libgst$p.so"; do [ -e "$f" ] && ln -sf "$f" "$out/gst-plugins/"; done
done
export GSTREAMER_PLUGINS_DIR="$out/gst-plugins"
export GSTREAMER_HELPERS_DIR="$(pkg-config --variable=pluginscannerdir gstreamer-1.0)"

# The in-app player (gtk::Video) needs GTK's GStreamer media backend, a module the GTK plugin
# does not copy. Copy it in BEFORE linuxdeploy runs and let linuxdeploy deploy its dependencies
# (--deploy-deps-only). Its own RUNPATH is just $ORIGIN (the media folder), where none of its
# libgst* live, so without a fixed RUNPATH the loader takes libgstvideo/pbutils/gl/tag/allocators,
# and through them libgstaudio, from the HOST while libgstreamer/libgstbase come from the bundle.
# On newer hosts that fails: "libgstaudio-1.0.so.0: undefined symbol: gst_cpuid_supports_x86_sse2"
# and GTK reports the "gstreamer" media extension as not found (bug on Ubuntu 26.04, Oct 2026).
media_src="$(ls -d "$(pkg-config --variable=libdir gtk4)"/gtk-4.0/*/media 2>/dev/null | head -n1)"
if [ -z "$media_src" ] || [ ! -e "$media_src/libmedia-gstreamer.so" ]; then
  echo "GTK media module libmedia-gstreamer.so not found (install libgtk-4-media-gstreamer)" >&2; exit 1
fi
media_dst="$appdir/usr/lib/gtk-4.0/$(basename "$(dirname "$media_src")")/media" # e.g. 4.0.0/media
mkdir -p "$media_dst"
cp -L "$media_src/libmedia-gstreamer.so" "$media_dst/"

linuxdeploy-$ARCH.AppImage --appdir "$appdir" --plugin gtk --plugin gstreamer \
  --deploy-deps-only "$media_dst/libmedia-gstreamer.so" \
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
# GStreamer: only the bundled plugins and scanner, and a private registry. The gstreamer plugin's hook
# sets GST_PLUGIN_SYSTEM_PATH_1_0/GST_PLUGIN_PATH_1_0 and the scanner; also clear the unversioned
# GST_PLUGIN_PATH, and keep the registry out of ~/.cache/gstreamer-1.0 (shared with the host
# GStreamer, whose newer plugins would otherwise be listed in the same file).
if ! grep -q 'GST_REGISTRY_1_0' "$hook"; then
  cat >> "$hook" <<'HOOK'
export GST_PLUGIN_PATH="$APPDIR/usr/lib/gstreamer-1.0"
export GST_PLUGIN_SYSTEM_PATH_1_0="$APPDIR/usr/lib/gstreamer-1.0" GST_PLUGIN_PATH_1_0="$APPDIR/usr/lib/gstreamer-1.0"
export GST_REGISTRY_1_0="${XDG_CACHE_HOME:-$HOME/.cache}/unswiper/gstreamer-1.0/registry-$(uname -m).bin"
export GST_REGISTRY="$GST_REGISTRY_1_0"
HOOK
fi
if ! grep -q 'GTK_MEDIA' "$hook"; then
  echo 'export GTK_PATH="$APPDIR/usr/lib/gtk-4.0" GTK_MEDIA=gstreamer' >> "$hook"
fi
# Find the bundled libgst* from the media module (usr/lib/gtk-4.0/4.0.0/media -> usr/lib).
patchelf --set-rpath '$ORIGIN/../../..' "$media_dst/libmedia-gstreamer.so"

# Third-party notices: the AppImage bundles LGPL/GPL libraries (GTK, GStreamer, and Ubuntu's FFmpeg,
# which is built with --enable-gpl, so libavcodec & co. are GPL-2.0-or-later). List each bundled
# library's Debian package and version and copy its copyright file; the matching source is on
# https://launchpad.net/ubuntu/+source/<source package> (see THIRD-PARTY-NOTICES.md in the AppImage).
notices="$appdir/usr/share/doc/unswiper/third-party"
rm -rf "$notices"; mkdir -p "$notices"
{
  echo "# Third-party software in Unswiper-$ARCH.AppImage"
  echo
  echo "Unswiper itself is MIT-licensed. The AppImage also contains the shared libraries below, unmodified,"
  echo "from Ubuntu packages, each under its own license (see the copyright files in this folder). Some, such"
  echo "as FFmpeg (libav*, GPL-2.0-or-later as built by Ubuntu) and GStreamer (LGPL-2.1-or-later), are copyleft;"
  echo "their source code is available from Ubuntu at https://launchpad.net/ubuntu/+source/<source package>/<version>"
  echo "and from the Unswiper maintainer on request. Some bundled decoders (H.264, HEVC, MPEG-4 Part 2, AAC)"
  echo "implement patented formats; check whether you need a patent license where you live."
  echo
  echo "| Package | Version | Source package |"
  echo "|---|---|---|"
  find "$appdir/usr/lib" -name '*.so*' \( -type f -o -type l \) -printf '%f\n' | sort -u | while read -r f; do
    { dpkg -S "*/$f" 2>/dev/null || true; } | head -n1 | cut -d: -f1
  done | sort -u | while read -r pkg; do
    [ -n "$pkg" ] || continue
    ver="$(dpkg-query -W -f='${Version}' "$pkg" 2>/dev/null)"
    src="$(dpkg-query -W -f='${source:Package}' "$pkg" 2>/dev/null)"
    echo "| $pkg | $ver | ${src:-$pkg} |"
    if [ -e "/usr/share/doc/$pkg/copyright" ]; then cp -L "/usr/share/doc/$pkg/copyright" "$notices/$pkg.copyright"; fi
  done
} > "$notices/THIRD-PARTY-NOTICES.md"
install -Dm644 "$root/LICENSE" "$appdir/usr/share/doc/unswiper/LICENSE" 2>/dev/null || true

rm -f "$out"/Unswiper*.AppImage
VERSION="$version" LDAI_OUTPUT="Unswiper-$ARCH.AppImage" \
  linuxdeploy-$ARCH.AppImage --appdir "$appdir" --output appimage
# Every libgst* the media module, plugins, scanner and bundled GStreamer libraries load must come
# from the AppDir (no host GStreamer mixed with the bundled one).
"$here/packaging/check-appimage-gst.sh" "$appdir"
ls -l "$out/Unswiper-$ARCH.AppImage"
