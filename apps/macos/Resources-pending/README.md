<!-- SPDX-License-Identifier: MIT -->
# Pending macOS app icon (not wired in)

Generated from the Linux icon (`crates/unswiper-gtk/data/icons/hicolor/scalable/apps/io.github.rranslam.Unswiper.svg`)
placed on a macOS-style rounded square: `AppIcon-macos.svg`.

- `AppIcon.iconset/` — 16, 32, 128, 256, 512 px at @1x and @2x (16 to 1024 px), Apple naming.
- `AppIcon.icns` — built from the iconset on Linux (PNG chunks `icp4`…`ic14`).
  On a Mac you can rebuild it with `iconutil -c icns AppIcon.iconset`.

**Not used by the Mac app yet.** The Xcode/SwiftPM project, Info.plist and bundle ID are unchanged.
To adopt it: add `AppIcon.icns` to the bundle's Resources and set `CFBundleIconFile` to `AppIcon`
(or import the iconset into an asset catalog as `AppIcon`).
