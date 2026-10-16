#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Rob Ranslam
#
# Checks that the AppImage's GStreamer stack is self-contained: for the GTK media module, every bundled
# GStreamer plugin, the plugin scanner and every bundled libgst*, `ldd` (with an empty environment, so
# only RUNPATHs and the host's default paths apply, like at run time) must resolve every libgst* inside
# the AppDir. Also checks the AppRun hooks keep host plugins and the host registry out.
# Then checks the codecs: the plugin files for AVI and MJPEG are bundled, and gst-inspect-1.0 (from the
# build host, same GStreamer version as the bundle) run against ONLY the bundled plugins lists avidemux,
# jpegdec, avdec_mpeg4 and avdec_h264. With ffmpeg, it also decodes small test AVIs (MPEG-4 Part 2 + MP3,
# MJPEG) with the bundled plugins. SKIP_GST_INSPECT=1 skips the gst-inspect/decode part.
# Usage: check-appimage-gst.sh APPDIR   (an AppDir, or an extracted squashfs-root)
set -euo pipefail
appdir="$(cd "${1:?usage: $0 APPDIR}" && pwd -P)"
fail=0
mapfile -t files < <(
  find "$appdir/usr/lib/gtk-4.0" -name 'libmedia-gstreamer.so'
  find "$appdir/usr/lib/gstreamer-1.0" -name '*.so'
  find "$appdir/usr/lib" -type f -name gst-plugin-scanner
  find "$appdir/usr/lib" -maxdepth 1 -name 'libgst*.so*'
)
if ! printf '%s\n' "${files[@]}" | grep -q libmedia-gstreamer.so; then
  echo "FAIL: libmedia-gstreamer.so is not bundled" >&2; fail=1
fi
for f in "${files[@]}"; do
  [ -n "$f" ] || continue
  while read -r line; do
    name="${line%% *}"
    path="$(sed -n 's/.*=> \([^ ]*\) .*/\1/p' <<<"$line")"
    case "$name" in libgst*) ;; *) continue;; esac
    if [ -z "$path" ] || [ "$path" = "not" ]; then
      echo "FAIL: ${f#$appdir/}: $name not found" >&2; fail=1
    else
      real="$(readlink -f "$path")"
      case "$real" in "$appdir"/*) ;; *) echo "FAIL: ${f#$appdir/}: $name => $real (outside the AppDir)" >&2; fail=1;; esac
    fi
  done < <(env -i ldd "$f" 2>&1 | sed 's/^[[:space:]]*//')
done
hooks="$(cat "$appdir"/apprun-hooks/*.sh)"
for v in GST_PLUGIN_SYSTEM_PATH_1_0 GST_PLUGIN_PATH_1_0 GST_PLUGIN_SCANNER_1_0 GST_REGISTRY_1_0 GTK_MEDIA; do
  grep -q "$v=" <<<"$hooks" || { echo "FAIL: AppRun hooks don't set $v" >&2; fail=1; }
done
for p in avi jpeg libav isomp4 matroska audiofx playback; do
  [ -e "$appdir/usr/lib/gstreamer-1.0/libgst$p.so" ] || { echo "FAIL: GStreamer plugin libgst$p.so is not bundled" >&2; fail=1; }
done
if [ "${SKIP_GST_INSPECT:-0}" != 1 ]; then
  if ! command -v gst-inspect-1.0 >/dev/null; then
    echo "FAIL: gst-inspect-1.0 not found (install gstreamer1.0-tools, or set SKIP_GST_INSPECT=1)" >&2; fail=1
  else
    tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
    scanner="$(find "$appdir/usr/lib" -type f -name gst-plugin-scanner | head -n1)"
    bundled() {
      env -i HOME="$tmp" PATH=/usr/bin:/bin LD_LIBRARY_PATH="$appdir/usr/lib" \
        GST_PLUGIN_SYSTEM_PATH_1_0="$appdir/usr/lib/gstreamer-1.0" GST_PLUGIN_PATH_1_0= \
        GST_PLUGIN_SCANNER_1_0="$scanner" GST_REGISTRY_1_0="$tmp/registry.bin" "$@"
    }
    # Element lookup by name is unreliable when the host's gst tools are newer than the bundle, so
    # list everything once and grep (lines look like "libav:  avdec_mpeg4: libav MPEG-4 part 2 decoder").
    elements="$(bundled gst-inspect-1.0 2>/dev/null || true)"
    for e in avidemux jpegdec avdec_mpeg4 avdec_msmpeg4 avdec_h264 qtdemux matroskademux scaletempo playbin; do
      grep -q "  $e: " <<<"$elements" || { echo "FAIL: element $e missing from the bundled plugins" >&2; fail=1; }
    done
    if command -v ffmpeg >/dev/null && command -v gst-launch-1.0 >/dev/null; then
      ffmpeg -loglevel error -y -f lavfi -i testsrc=d=1:s=160x120:r=10 -f lavfi -i sine=d=1 \
        -c:v mpeg4 -vtag XVID -c:a libmp3lame "$tmp/xvid.avi" &&
      ffmpeg -loglevel error -y -f lavfi -i testsrc=d=1:s=160x120:r=10 -c:v mjpeg "$tmp/mjpeg.avi" || true
      for v in xvid mjpeg; do
        [ -s "$tmp/$v.avi" ] || { echo "note: ffmpeg could not make $v.avi; decode check skipped" >&2; continue; }
        if ! bundled timeout 60 gst-launch-1.0 -q uridecodebin uri="file://$tmp/$v.avi" ! videoconvert ! fakesink >"$tmp/$v.log" 2>&1; then
          echo "FAIL: the bundled GStreamer can't decode a $v AVI:" >&2; cat "$tmp/$v.log" >&2; fail=1
        fi
      done
    fi
  fi
fi
[ "$fail" = 0 ] && echo "GStreamer bundle OK: ${#files[@]} files, every libgst* resolves inside the AppDir; avidemux, jpegdec, avdec_mpeg4 present"
exit "$fail"
