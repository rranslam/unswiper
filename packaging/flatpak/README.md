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
- **ffmpeg: not included.** Video thumbnails call the `ffmpeg` *command*. The
  `org.freedesktop.Platform.ffmpeg-full` extension ships only libraries, not the CLI, so it would not
  help. Inside the Flatpak, videos show the generic video placeholder; dates, grouping, keep/reject
  and commit work the same. Bundling an ffmpeg CLI module (or decoding frames through GStreamer, which
  the GNOME runtime has) is a possible follow-up.

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
