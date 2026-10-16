#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Rob Ranslam
#
# Checks that the AppImage's GStreamer stack is self-contained: for the GTK media module, every bundled
# GStreamer plugin, the plugin scanner and every bundled libgst*, `ldd` (with an empty environment, so
# only RUNPATHs and the host's default paths apply, like at run time) must resolve every libgst* inside
# the AppDir. Also checks the AppRun hooks keep host plugins and the host registry out.
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
[ "$fail" = 0 ] && echo "GStreamer bundle OK: ${#files[@]} files, every libgst* resolves inside the AppDir"
exit "$fail"
