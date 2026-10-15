# Changelog

All notable changes to Unswiper are listed here. Downloads: https://github.com/rranslam/unswiper/releases

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
