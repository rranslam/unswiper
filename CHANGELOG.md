# Changelog

All notable changes to Unswiper are listed here. Downloads: https://github.com/rranslam/unswiper/releases

## [Unreleased]

### Added
- Geo grouping, core (plan §20, M1). Three grouping modes. `visit` is the default and works exactly as in v0.5. `geo` groups by place regardless of time: DBSCAN on great-circle distance, 1 km and at least 3 photos, with each place split into trips. `visit-geo` splits each visit by place: 2 km, after 3 photos or 30 minutes. Locations come from EXIF GPS (photos), QuickTime/MP4 (videos) or, as a fallback, Takeout `geoData`. `0,0` is ignored. Photos without a location take the place of a photo taken within 30 minutes, or else stay in their date-based set. Place names come offline from the new `unswiper-geo` crate (reverse_geocoder 4.1.1, GeoNames CC BY 4.0, see THIRD_PARTY_NOTICES). library.json schema 2 adds optional `location`, `place`, `parentVisitId` and `config.groupMode` (missing or unknown = `visit`). v1 files load unchanged. CLI: `--group-mode`, `--place-radius-km`, `--min-place-items` and `--join-same-place` apply to that run only. Also new: `config set|get group-mode` and `geo <lat> <lon>`. The Mac helper gained `unswiper-blur geo`. The GTK and Mac UI follow in M2.
- Geo grouping in the apps (plan §20, M2). A **Grouping** picker (By Visit (Time), By Place, By Visit and Place) saves `groupMode` with the library and regroups right away. Linux: a Grouping section in the main menu. Mac: View ▸ Grouping and a picker above the visits list. Place groups show a pin. The Mac app now reads and writes library.json schema 2 (v1 files load unchanged) and groups by place through the bundled helper's new `unswiper-blur group` command, so both apps use the same code and place names. THIRD_PARTY_NOTICES (GeoNames, CC BY 4.0) ships in the Flatpak, AppImage and dmg and is shown in About. Help explains the grouping modes, and the Mac Privacy page now says where locations are kept.

### Changed
- Minimum supported Rust version is now 1.92 (was 1.89). The GTK 4 bindings already needed 1.92. To stay on 1.92, the Linux app uses relm4 0.10.1 (with gtk4-rs 0.10, glib 0.21 and libadwaita 0.8) instead of 0.11, which needs 1.93. No app code changes were needed. `kstring` (pulled in by gstreamer) is pinned to 2.0.2, because 2.0.5 needs Rust 1.96. `fixed` (used by the offline geocoder) stays pinned to 1.30.0, because 1.31 needs Rust 1.93.

## [0.5.0] - 2026-10-25

### Added
- Google Takeout import, core (plan §19, M1): `unswiper-cli takeout import <folder|archives> --to <library>` safely unpacks the Google Photos part of Takeout `.zip`/`.tgz` exports (all parts, resumable, never writes outside the library folder), with a dry run (`--dry-run`), a free-space check, optional deletion of each archive after unpacking (`--delete-archives`) and incremental re-import of later exports. `unswiper-cli index <library> --takeout` uses Google's JSON files for dates and places when a photo has none. The app UI follows in M2.
- **Import from Google Photos** (plan §19, M2), Linux and Mac: a step-by-step assistant (Library menu, setup screen and Help) that explains exactly which Google Takeout options to pick (Google Photos only, .zip, 50 GB, every 2 months), opens Takeout with Google Photos already chosen, lets you choose the downloads folder and the library folder, shows the size, space and time needed before anything starts, unpacks with a progress bar (stop and continue later), can delete each archive after unpacking, skips album copies unless you ask for them, and then opens the library with Google's dates. New help page "Import from Google Photos". On the Mac the bundled helper gained `takeout import --json --progress-json` and `takeout dates`.
- Mac help: photos in Google Drive can be opened directly through Google Drive for desktop (no Takeout), and what to do with Google Drive's "Items moved" dialog after a commit (click OK, not Move back; rejects keep using Drive space until you delete `_Rejected`).
- Google Photos import: the free-space check is mandatory. The import doesn't start when the free space on the library disk can't be measured; the check says how to fix it (choose a folder in Pictures, or in the Flatpak the exact `flatpak override` line). In the Flatpak, portal folders are measured on the real disk (via the portal's host path when needed). `unswiper-cli takeout import --no-space-check` remains as the explicit opt-out.

## [0.4.0] - 2026-10-15

### Fixed
- Linux AppImage: videos didn't play on newer distributions such as Ubuntu 26.04 ("Media extension gstreamer … not found"). GTK's video module loaded the system's GStreamer libraries next to the bundled ones; it now uses only the bundled GStreamer, its own plugins and a private plugin registry. Also affected the 0.3.0 AppImages; Flatpak and Mac were not affected. ([#1](https://github.com/rranslam/unswiper/issues/1))
- Linux AppImage: **AVI** videos (DivX/Xvid, MS-MPEG4, Motion JPEG, DV) didn't play or get thumbnails ("missing codec"): the AVI demuxer wasn't bundled. The AppImage now also bundles the JPEG, DV, MPEG-TS (MTS/M2TS), Ogg, Vorbis, Opus and MP3 plugins, and every build checks that `avidemux`, `jpegdec` and `avdec_mpeg4` are present and that test AVIs decode. The Flatpak already played AVI. On the Mac, AVI still opens in your default player (Help explains why).
- Linux AppImage: bundles GStreamer's `scaletempo` (audiofx plugin), so the player no longer warns "scaletempo element not available".
- Linux AppImage: now includes a list of the bundled third-party libraries with their licenses (`usr/share/doc/unswiper/third-party`).

### Added
- **Photo Info (Linux and Mac).** See how a photo was taken to choose between similar shots: camera and lens, shutter, aperture, ISO, focal length (with the 35 mm equivalent), flash, exposure compensation, date with time zone, dimensions, file size (Live Photo motion included), format and folder, and the blur check in words (**Sharp**, **Soft** or **Blurry**). **Right-click** a tile (Mac: right-click ▸ **Show Info**) for a quick look, or press **I** (Linux **Ctrl+I**, Mac **⌘I**) for the full inspector, which follows the selection and the viewer. A one-line settings summary shows the selected item only (under the grid; on the Mac also under the clicked picture in Compare).
- **Location in Photo Info**, shown only on your computer and never stored or sent; **Open in Maps** opens OpenStreetMap only when you click it. On by default; hide it with **Show Location in Photo Info** (Linux main menu, Mac **View** menu). The setting is saved in library.json as `infoShowLocation: false` and shared by both apps.
- Videos: length, size, codec, camera and location are read from MOV/MP4 files (portrait iPhone clips show the right way round).
- `unswiper-blur info [--library library.json] FILE...` prints the same rows as JSON (read-only); the Mac app uses it through the bundled helper.
- New Help pages: **Photo Info** (Linux and Mac); Keyboard Shortcuts and Privacy updated.

## [0.3.0] - 2026-10-15

### Changed
- Blur thresholds calibrated on 85 real photos (`docs/blur-evaluation.md`). Sharp seascapes, horizons and panels are no longer marked as shaken, and false positives at Normal dropped from 14 % to 6 % of sharp photos.

### Fixed
- Linux Help: the Keyboard Shortcuts page now matches the app (F5 re-indexes, Ctrl+Shift+R restores).
- Linux: test and screenshot runs (`--quit-after`) no longer merge into an already-running Unswiper.

### Added
- **Blurry photo detection (Linux).** Unswiper checks photos in the background (the open visit first) and marks likely out-of-focus or shaken photos **Reject** with an orange **Blurry?** badge. Nothing moves until you commit the visit; your own Keep/Reject choices always win, and pressing **U** on a marked photo clears it for good. On by default; turn it off, change the sensitivity (Low / Normal / High), or clear or confirm all suggestions in a visit from the main menu under **Blurry Photos**. A **Blurry** filter shows only the marked photos, and the commit confirmation says how many rejects were suggested as blurry. New Help page: **Blurry Photos**.
- **Play videos while reviewing (Linux and Mac).** Double-click an item, or select it and press **Space**, to open a full-window viewer; **Space**/**Esc** close it, **←/→** step through the visit, and **K/R/U** decide on the item shown. Videos start muted (your mute choice is kept until you quit), clips under 10 s loop, and video tiles show a **▶ 0:07** badge. **Play Live Photo Motion** (off by default) plays a Live Photo's video part. Files that can't be played offer **Open in Default App**. New Help page: **Watching Videos and Large Previews**.
- AppImage now bundles GTK's GStreamer media module and audio output (PipeWire/PulseAudio).
- **Blurry photo detection on the Mac**, with the same rules, settings and library.json keys as Linux. The Mac app runs the same Rust detector (a small `unswiper-blur` helper inside Unswiper.app), the open visit first. You get the orange **Blurry?** badge, a **Blurry** filter, a **Blurry Photos** menu (on/off, sensitivity, check now, clear or reject all suggestions in a visit), **U** to clear a choice, and a note in Review & commit. Blur suggestions saved by the Linux app are kept when the Mac app saves. New Mac Help page: **Blurry Photos**.
- `unswiper-blur scan --only FILE` scores just the listed photos.
- **`unswiper-blur` command-line tool** (`unswiper-blur scan <folder|library.json>`), read-only, for checking big folders and tuning sensitivity.
- New crate `unswiper-blur` (pure Rust, offline, MIT). Scores are cached in `quality-cache.json` next to `library.json`; `library.json` only gets two optional keys (`decisionSource: "auto-blur"`, `blurDismissed`), so files without blur data are unchanged.

## [0.2.0] - 2026-10-13

### Added
- **Unswiper for Linux.** A native GNOME app (GTK 4 + libadwaita) with the same visits, Keep/Reject, Review & Commit and Restore as the Mac app. Works on GNOME, Hyprland/Omarchy and other Wayland or X11 desktops.
- **Two Linux downloads, each for x86_64 and Arm (aarch64, e.g. Apple Silicon Macs running Asahi or a Linux VM)** on the release page: `Unswiper-x86_64.AppImage` (one file, make it executable and run it) and `Unswiper-x86_64.flatpak` (sandboxed; install with `flatpak install --user`). See the README for step-by-step instructions.
- **Shared Rust core** (`unswiper-core`) for grouping, indexing, commit and restore, with a command-line tool (`unswiper`). It reads and writes the same `library.json` and commit manifests as the Mac app.
- **Video thumbnails on Linux via GStreamer** (H.264 and HEVC/iPhone clips). The Flatpak gets the codecs from the Freedesktop codecs extension; the AppImage bundles them. **HEIC photos** are supported through libheif.

### Changed
- **Commit applies only to the open visit.** The confirmation names the visit and how many files will be moved; Commit is disabled when the visit has no rejects. (The Mac app already worked this way.)
- **New app icon:** photos going into a funnel, with the kept one marked by a check.
- **New bundle ID: `io.github.rranslam.Unswiper`** (was `com.rranslam.unswiper`), the same as the Linux app. Your libraries, decisions and commit history stay where they are. Window sizes and other preferences are copied over on first launch. **macOS will ask again for permission to access your photo folders**; click Allow.
- The Mac app is still ad-hoc signed: on first launch use System Settings ▸ Privacy & Security ▸ **Open Anyway**.

## [0.1.2] - 2026-10-13

### Fixed
- **Saved decisions can no longer be wiped silently.** If `library.json` can't be read (damaged, or saved by a newer version), Unswiper now copies it to `library.json.corrupt-<date>` first, tells you in an alert, and only then starts empty. Before, it quietly started empty and the next save overwrote all your Keep/Reject choices. `library.json` now records a `schemaVersion`.
- **Interrupted commits can be restored.** The commit record (manifest) is now written *before* any file moves and updated after each one. If Unswiper quits mid-commit (crash, power loss, force quit), the next launch offers to put the moved files back.
- Restore now says clearly when a file can't be found in the Trash (for example after the Trash was emptied) instead of failing quietly.
- Commit refuses symbolic links, and validates every file before moving any.

### Changed
- **New rule for files without a capture date.** The file's modification date is no longer used for grouping (it changes whenever a file is copied). Instead Unswiper uses a date in the folder path (`2026/01/01`, `2026-01`, `2025-07-04 Beach`, `1999 Photos`), then the dated photos in the same folder, then a visit named after the folder. Only files directly in the library folder with no date at all go to **Undated**. Click **Re-index** after updating; some visits will regroup.
- Visits without dates show "No date" instead of a placeholder date.
- GitHub repository renamed from `visit-media-reduce` to `unswiper`: https://github.com/rranslam/unswiper. Old links redirect automatically. README, Help pages, and Help menu links now point to the new address.

### Added
- Automated tests (`swift test`, XCTest target `UnswiperTests`): grouping and stable visit IDs, capture/folder dates, Live Photo and sidecar pairing, commit safety checks, commit and restore round trips (quarantine and Trash), interrupted commits, dry run, manifests, old `library.json` formats, damaged-file recovery and debounced saving.
- Continuous integration: every push and pull request builds and runs the tests on GitHub (`.github/workflows/ci.yml`), with a coverage report.

## [0.1.1] - 2026-10-12

### Added
- Built-in **Help** window (Help ▸ Unswiper Help, or ⌘?) with a topic sidebar and search. Topics: Getting Started, Visits and Grouping, Reducing a Visit, Review & Commit, Restore Last Commit, Live Photos and Sidecar Files, Keyboard Shortcuts, Install/Update/Uninstall, Privacy, Troubleshooting and FAQ, About.
- Help menu items: Keyboard Shortcuts, Getting Started, Release Notes (GitHub Releases), Report an Issue.
- This changelog.

### Changed
- The DMG build script now bundles the help files into the app and checks they load.

## [0.1.0] - 2026-10-11

### Added
- First standalone release of Unswiper (formerly visit-media-reduce) as a downloadable `.dmg` for macOS 14+ (Apple Silicon and Intel).
- Group photos and videos into visits by time; reduce each visit with Keep/Reject, Compare and a per-visit keep target.
- Commit rejected files to the Trash or a quarantine folder (or dry run), with a confirmation dialog, file safety checks, and Restore Last Commit.
- Live Photos and sidecar files (.MOV, .AAE, .XMP) move together.
- MIT License.
