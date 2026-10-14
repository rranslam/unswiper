<!-- SPDX-License-Identifier: MIT -->
# macOS app icon

- `AppIcon-macos.svg` is the source: the Linux funnel icon
  (`crates/unswiper-gtk/data/icons/hicolor/scalable/apps/io.github.rranslam.Unswiper.svg`) on a macOS-style
  rounded square.
- `AppIcon.iconset/` holds the PNGs at 16, 32, 128, 256 and 512 px, each at @1x and @2x (16 to 1024 px), with
  Apple's names.
- `scripts/build-dmg.sh` turns the iconset into `AppIcon.icns` with `iconutil` when it builds the app, and
  sets `CFBundleIconFile` to `AppIcon`. CI checks that `iconutil` accepts the iconset.

To change the icon, edit the SVG and re-render the PNGs, e.g. with `rsvg-convert -w <px> -h <px>`.
