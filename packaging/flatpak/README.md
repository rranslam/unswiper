<!-- SPDX-License-Identifier: MIT -->
# Flatpak

`io.github.rranslam.Unswiper.yml` builds the GTK app (`crates/unswiper-gtk`) on GNOME Platform/SDK 50
with the `rust-stable` SDK extension (25.08). It is not published to Flathub.

```sh
flatpak install --user flathub org.gnome.Sdk//50 org.freedesktop.Sdk.Extension.rust-stable//25.08
flatpak-builder --user --install --force-clean build-flatpak packaging/flatpak/io.github.rranslam.Unswiper.yml
flatpak run io.github.rranslam.Unswiper
# single-file bundle:
flatpak build-bundle ~/.local/share/flatpak/repo Unswiper.flatpak io.github.rranslam.Unswiper
```

CI (`.github/workflows/flatpak.yml`) builds the same manifest and uploads `Unswiper.flatpak` as an artifact.

## Offline crates

`cargo-sources.json` lists every crate from `Cargo.lock` for the offline build. **Regenerate it after
any `Cargo.lock` change:** `packaging/flatpak/update-cargo-sources.sh` (uses a pinned
`flatpak-cargo-generator.py`).

## Bundled libraries

- **libheif 1.20 + libde265** — HEIC/HEIF thumbnails (decode only; every other codec is off).
- **Video thumbnails: GStreamer from the GNOME runtime.** The app grabs one frame with
  `uridecodebin ! videoconvert ! videoscale ! appsink`. The runtime ships `gst-libav`, and the
  H.264/HEVC decoders behind it come from `org.freedesktop.Platform.codecs-extra`. Flatpak installs that
  runtime extension automatically with the 25.08 base (GNOME 50), so the manifest needs no extra
  extension. The old `ffmpeg-full` extension is not needed. Verified in the sandbox: iPhone-style
  H.264 (`avc1`) and HEVC (`hvc1`) `.mov` clips both get thumbnails. If the codecs extension is
  missing or a codec isn't covered, the clip shows the video placeholder and everything else works.
  The ffmpeg command is not used any more.
  **AVI** (checked 15 Oct 2026 with GNOME 50 + codecs-extra 25.08): `avidemux` and `jpegdec` come from the
  runtime's gst-plugins-good, `avdec_mpeg4`/`avdec_msmpeg4`/`avdec_h264`/`avdec_mp3` from gst-libav with
  codecs-extra. Test AVIs (MPEG-4 Part 2/Xvid, MS-MPEG4 v3, MJPEG, DV) all decode in the sandbox, so the
  Flatpak needs no change. `dvdec` (libdv) isn't in the runtime, but `avdec_dvvideo` decodes DV.

## Sandbox permissions

| Permission | Why |
|---|---|
| `--socket=wayland`, `--socket=fallback-x11`, `--share=ipc`, `--device=dri` | Standard GTK 4 display access. |
| `--filesystem=xdg-pictures`, `--filesystem=xdg-videos` | Direct access to the usual photo folders (see below). |
| `--filesystem=xdg-data/Trash` | **Move to Trash** writes the freedesktop home trash (`files/` + `info/*.trashinfo`), so the commit record knows where each file went and **Restore Last Commit** can put it back. Inside the sandbox the app uses `HOST_XDG_DATA_HOME` or `~/.local/share` for this, not the app's private data folder. |

No network, no `home`/`host` access. Unswiper's own data (library.json, commit records) goes to the
app's private `~/.var/app/io.github.rranslam.Unswiper/data/unswiper`, and thumbnails go to `…/cache/unswiper`.

### Folders outside Pictures/Videos: the document portal

Any other folder (another drive, `~/Documents/Trips`, …) is opened with the portal folder chooser, and
the app sees it as `/run/user/<uid>/doc/<id>/<name>`. What works there:

- **Indexing, thumbnails, Keep/Reject:** yes.
- **Quarantine commits and restore:** yes. Moves stay inside the granted folder (`_Rejected/Unswiper`), and the
  portal grant is remembered, so the same path works again after a restart.
- **Live folder watching:** the document-portal FUSE mount does not report changes made outside the
  app through inotify, so the app detects these paths and **polls every 5 s** instead.
- **Move to Trash:** a rename from the portal mount into `~/.local/share/Trash` crosses file systems and
  fails. Those files are reported as skipped, and the app's help already says to use **Quarantine** for
  folders on other drives. For portal folders, use Quarantine.

**Same drive, separate mounts:** inside the sandbox `~/Pictures` and `~/.local/share/Trash` are separate
bind mounts, so `rename` reports EXDEV even though they are on the same disk. The trash code checks
`st_dev`. When both are on the same file system it copies, verifies and removes the file instead of
refusing. The host's `~/.local/share/Trash` must already exist, which it does on any desktop that has
used the Trash.

### Verified on a Debian 13 box (GNOME 50 runtime, Xvfb, xdg-desktop-portal 1.20)

- Built with `flatpak-builder`, installed from the bundle, run headless.
- `~/Pictures/…`: indexing; a live inotify pickup of a new file; **Move to Trash** (file and `.trashinfo` in the host
  Trash); **Restore Last Commit** puts it back.
- Document-portal folder (`/run/flatpak/doc/<id>/Trips`): indexing; a polling pickup of a new file;
  **Quarantine** commit to `_Rejected/Unswiper/<stamp>/`; restore. A **Trash** commit is skipped with the
  "different drive" message, as documented.
- Not tested: the interactive portal folder chooser dialog (the grant was made with
  `flatpak document-export`, which is what the chooser does), a real Wayland session, and HEIC thumbnails
  inside the sandbox.

**Tradeoff:** the two `xdg-pictures`/`xdg-videos` grants make the common case (photos in `~/Pictures`)
behave exactly like the native build: inotify watching, and Trash. They also let the app read and
write those folders without asking first. Dropping them is safe: everything then goes through the
portal with the limits above. `--filesystem=home` was not used.

**Google Photos import, free space (v0.5 M2):** the import never starts without knowing the free
space on the library disk. A folder picked through the portal (`/run/user/…/doc/<id>/…`) reports the
real disk's free space (xdg-desktop-portal 1.14 and later pass `statvfs` through for documents). If a
portal ever reports no sizes, Unswiper reads the real path from the portal's
`user.document-portal.host-path` attribute (1.19 and later) and measures it there when the sandbox
can see it (Pictures, thanks to `xdg-pictures`). Otherwise the check blocks and tells the user to pick
a folder in Pictures or to run `flatpak override --user --filesystem='<folder>' io.github.rranslam.Unswiper`.
No permission was added for this.
