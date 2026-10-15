# Unswiper for Linux (and a shared Rust core): Port Plan, rev 3

*Revised 13 Oct 2026 (rev 3). Targets: Ubuntu 24.04 LTS and newer (26.04 LTS) / Debian 13+, current Arch / Omarchy (Hyprland, Wayland), and macOS 14+ through the shared core. Checked against the Swift source on `main` of `rranslam/unswiper` (formerly `visit-media-reduce`, renamed 13 Oct 2026).*

**What changed in rev 3:** Rob approved the plan and resolved most §14 decisions (§14.1): minimum Linux is **Ubuntu 24.04 LTS**, **libheif is required**, **ffmpeg stays optional**, **live folder watching is in v1**, the **80% / 85% coverage gate** is approved, Apple Developer signing is **pending** (Rob is setting it up), and the repo rename to `unswiper` is **done**. Effort rises by about 3–4 days (folder watching, 24.04 compatibility).

**What changed since rev 1:** I checked every claim against the actual Swift code and corrected it (§2 lists the corrections). I added a full automated-testing plan (§10) and put tests into every phase. Rob's decisions are now built in: rename the repo to `unswiper`, the folder-context Undated rule, a Rust core for **both** Linux and macOS, Apple signing and notarization, and geo grouping plus Google Photos moved to the roadmap.

## 1. Summary and recommendation

**Rewrite the logic in Rust as a `unswiper-core` crate, used by both platforms.** Linux gets a GTK4 + libadwaita UI built with relm4. macOS **keeps its SwiftUI UI** and calls the core through UniFFI (§9). Linux ships as Flatpak (primary), .deb and AUR, with AppImage last. macOS keeps shipping a DMG, which is now signed and notarized.

Why:
- **The current app is ~2,070 lines of Swift in 16 files** (counted on `main`): App 396 (AppState 311, UnswiperApp 85), Help 239 (HelpView 164, HelpLibrary 75), Models 146, Services 561 (LocalTreeIndexer 292, CommitService 174, LibraryStore 95), and Views 729 (ReduceView 217, GroupsView 159, CommitView 131, CompareView 65, SetupView 55, ThumbnailView 53, RootView 49). There are also 11 Help Markdown pages (200 lines). About **707 lines (34%) are UI-free logic** (Models + Services), and even that code imports ImageIO, AVFoundation and AppKit (`trashItem`). None of those exist on Linux.
- **Swift on Linux has no SwiftUI**, so the UI must be rewritten on Linux whatever language we choose. Given that, Rust costs little extra. It brings memory and thread safety to file-moving code and a mature crate ecosystem, and it gives **one tested implementation of grouping, validation, commit and restore for both OSes**.
- **GTK4/libadwaita** is native on GNOME, works well on KDE, and handles Wayland and Hyprland properly. Portals, GIO Trash and the thumbnail cache are built in.
- Estimated size: about 3.5–4.5k lines of Rust plus about 2k lines of tests.

## 2. Code-check corrections (rev 1 → rev 2)

| Area | Rev 1 said | The Swift code actually does | Plan impact |
|---|---|---|---|
| Size | ~1.7k Swift lines, 30–40% logic | ~2,070 lines, 707 (34%) logic | Estimates updated |
| `library.json` dates | Swift default (seconds since 2001), needs migration | `JSONEncoder` with `.iso8601`, **no fractional seconds**, sorted keys, pretty-printed, atomic write | No date migration. Rust uses RFC 3339 seconds (`Z`) for compatibility |
| `library.json` shape | Unspecified, "has schemaVersion" | `LibrarySnapshot { config, items, groups, lastIndexedAt }`, **no version field**, **absolute paths** | v1 = today's format. v2 adds `schemaVersion` + root-relative paths (§5) |
| Load failure | Not covered | `load()` **silently falls back to an empty default** on any decode error, so the next save overwrites the user's decisions | Fix in both apps: back up the unreadable file and show an error (also an XCTest) |
| Manifest | Written *before* moving, with status | Written **after** all moves, to `…/Unswiper/commits/<stamp>.json` (+ `manifest.json` in the quarantine folder). Entries `{originalPath, resultPath, sizeBytes, movedAt}`. Header `{id, createdAt, mode, libraryRoot}` | A crash mid-commit currently leaves no record. Core writes a "pending" journal first (§6) |
| Restore | Generic | Only the **latest** manifest (newest by filename). Skips if the target exists. Renames the manifest to `.restored` only if every file succeeded, then re-indexes | Same semantics, plus "restore any commit" later |
| Trash `resultPath` | GIO lacks it | macOS gets it from `trashItem(resultingItemURL:)`. It is **empty string** if unknown, and restore then fails for that entry | Core must record the trash path itself (§3.1) |
| Quarantine | `_Rejected/Unswiper/<ts>` | `<root>/_Rejected/Unswiper/<ISO stamp, ':'→'-'>/`, flat, with an 8-char UUID prefix on collision. Falls back to `~/Library/Application Support/Unswiper/Quarantine` | Same layout on Linux |
| Validation | Plus inode check | realpath prefix check against root, regular file, exact size, mtime within **±1 s** (because the JSON has no fractional seconds). A missing `modifiedAt` means refuse. **Every file in the set** (sidecars too) is validated before any move | Same rules. Add `dev`+`ino` (new, Linux/macOS) |
| Pairing | Case-insensitive stem match | Key is the **lowercased full path minus extension** (same directory). Stills `heic/heif/jpg/jpeg` absorb a same-stem **`.mov` only** (not `.mp4`). `.aae/.xmp` attach to the anchor. **Orphan sidecars are ignored**. Other same-stem primaries stay separate items | Port exactly. Golden tests pin it |
| Media types | Many RAW formats | Photos: jpg jpeg png heic heif webp gif tif tiff bmp raw dng. Videos: mp4 mov m4v avi mkv mts m2ts 3gp webm | Linux v1 matches. CR2/CR3/NEF/ARW/ORF/RAF are a *new* feature for both apps |
| Dates | `DateTimeOriginal` + `OffsetTimeOriginal` | EXIF `DateTimeOriginal`, else TIFF `DateTime`, parsed in the **Mac's local time zone** (offset ignored). Video: AVAsset `creationDate`, else `com.apple.quicktime.creationdate`. Then file mtime, then birth time. Dates ≤ 1980‑01‑01 count as missing | Core keeps this order, adds OffsetTime as an improvement, and makes the local-zone behaviour explicit in tests |
| Undated | "Undated group" | `isUndated` only when **no** capture or file date. Group id `"undated"`, placed last | Replaced by Rob's folder-context rule (§7) |
| Grouping | Gap > 36 h | Gap `visitGapHours` (config, default 36), split when gap is **strictly greater**. Groups sorted by **total bytes, largest first**. ID = `visit-` + FNV‑1a‑64 hex of `"<firstPath>|<startEpochSeconds>"`. Title `MMM d[–MMM d] · <top-level folder>`. Folder hint = most common first path component under the root | Port bit-for-bit so IDs (and `keepTargets`) match across apps |
| Indexing | Background, parallel | `Task.detached`, **sequential**, progress callback currently discarded. Skips hidden files, packages, and any path containing `/_rejected/` or `/.trash/` | Rust: rayon for metadata, real progress |
| Commit scope | Global | **Per visit** (`commitGroup`). Dry run returns `wouldMove` and moves nothing | Same |
| Keys | — | `K` keep, `R` reject, ⌘Return opens commit. Confirm dialog with Cancel as the default. ⌘⇧R re-index, ⌘? help | Linux: same letters, Ctrl+Return, F1 / Ctrl+? help |
| Help | Markdown rendered natively | `HelpLibrary` loads `Resources/Help/*.md` from the SwiftPM bundle `Unswiper_Unswiper.bundle` (found by hand, since `Bundle.module` crashes in the .app). `HelpView` is a **hand-written block renderer** (headings, lists, code) with `AttributedString` for inline styling, plus a searchable sidebar. Menu: Unswiper Help (⌘?), Keyboard Shortcuts, Getting Started, Release Notes, Report an Issue. `--help-selftest` and `--show-help` flags. Issues/Releases URLs are hard-coded to `visit-media-reduce` | Linux reuses the same .md files and the same flags. URLs move to `unswiper` |
| "Show in Finder" | Mapped to FileManager1 | **Not in the app** | Moved to "new features" |
| Folder access | Bookmarks | Security-scoped bookmark is stored in `config.bookmarkData` (base64 in JSON), used even though the app isn't sandboxed | Linux ignores the field but keeps it on round-trip |
| Thumbnails | QuickLook | `QLThumbnailGenerator` in `ThumbnailView` | As planned |
| Packaging | — | SwiftPM, macOS 14, bundle ID `io.github.rranslam.Unswiper` (was `com.rranslam.unswiper` up to 0.1.2; §14.3), universal build with arm64 fallback, **ad-hoc** codesign, `hdiutil` UDZO. `release.yml` runs on tag `v*` on `macos-latest` with `softprops/action-gh-release@v2`. **No test target** | §8 + §9 |

## 3. Feature parity and macOS → Linux mapping

| Feature / macOS API | Linux / core equivalent |
|---|---|
| SwiftUI windows + menu bar (`Library` menu, Help menu) | gtk4-rs + libadwaita (relm4). Header bar + primary menu with the same items |
| `NSOpenPanel` + security-scoped bookmark | `gtk::FileDialog::select_folder` → xdg-desktop-portal. In Flatpak the Documents portal path persists |
| `FileManager.trashItem` | freedesktop Trash spec in core (`platform::trash`). GIO / Trash portal in Flatpak |
| ImageIO EXIF | `nom-exif` or `kamadak-exif` (JPEG/HEIC/TIFF/DNG) |
| AVFoundation creation date | pure-Rust MOV/MP4 atom parser (`mvhd`, `com.apple.quicktime.creationdate`). GStreamer discoverer for MKV/AVI/MTS/WebM |
| `QLThumbnailGenerator` | freedesktop thumbnail cache, then generate with `image` / `libheif-rs` / GStreamer frame grab |
| `~/Library/Application Support/Unswiper` | `$XDG_DATA_HOME/unswiper` (`library.json`, `commits/`), `$XDG_CACHE_HOME/unswiper` |
| `Task.detached` | rayon workers + relm4 commands. The UI thread never touches the filesystem |
| SwiftPM resource bundle (Help) | `include_str!` of the same `.md` files, with a shared Markdown block renderer (`pulldown-cmark` → GTK widgets) |
| FSEvents (not in v0.1.x) | **Live folder watching (v1):** `notify` crate (inotify on Linux, FSEvents on macOS), debounced via `notify-debouncer-full` |
| ~1 s debounced save, flush on quit/commit | same, atomic temp + fsync + rename |

### 3.0 Live folder watching (v1, Rob's decision)
- Core module `watch`, built on `notify` + `notify-debouncer-full` (≈1–2 s debounce). Backends: **inotify** on Linux, **FSEvents** on macOS (also used when the Mac app moves to the core in Phase 6).
- Events trigger an **incremental re-index** of affected paths only: new files are added and grouped, deleted/moved files are marked missing, modified files get fresh size/mtime (so commit validation stays correct). Decisions on unchanged items are preserved.
- Ignore our own moves (the commit journal suppresses events for paths being moved) and the `_Rejected/` folder.
- inotify limits: large libraries can exceed `fs.inotify.max_user_watches` (default 8192 on some older kernels; 24.04 defaults are higher). On `ENOSPC`, fall back to a periodic rescan and show a hint with the sysctl fix. Network/FUSE mounts often don't emit events: detect and fall back to polling (`PollWatcher`) or manual re-index.
- Flatpak: inotify works on portal-granted (Documents portal) paths, but verify in the Phase 0 spike.
- Watching pauses while a commit or restore runs, then rescans touched folders.

### 3.1 Trash (most important)
- Implement the spec in core: `$XDG_DATA_HOME/Trash/{files,info}` for the home filesystem. On other mounts use `$topdir/.Trash/$uid` (only if it's sticky, not a symlink, and admin-made), else `$topdir/.Trash-$uid`. Write the `.trashinfo` (`Path=` URL-encoded, `DeletionDate=`) with `O_EXCL` **before** the rename, so we always know `resultPath`. That fixes the empty-`resultPath` gap.
- Never copy across devices into home trash. If a mount has no usable trash, that file fails with "use Quarantine". **Never delete.**
- Flatpak: try the Trash portal. It doesn't return a location, so in Flatpak the trash mode records a "portal" entry and Restore says "use your file manager". Alternatively we ask for `--filesystem=xdg-data/Trash`. Week-1 spike.
- macOS backend: keep `trashItem` via a thin Swift shim, or `NSFileManager` through `objc2`. The core trait `Trasher` hides this.

## 4. Toolkit choice
Rust + gtk4-rs/libadwaita (relm4) is recommended for Linux. Slint stays the runner-up. egui, iced and Tauri are rejected for the reasons in rev 1 (native look, accessibility, WebKitGTK quirks).

## 5. Architecture and data formats

```
unswiper/                         (repo root after rename)
├─ apps/macos/                    SwiftUI app (stays), links UnswiperCore.xcframework
├─ crates/unswiper-core           pure logic + IO, no GTK
│   ├─ model.rs   MediaItem, SidecarFile, VisitGroup, AppConfig, LibrarySnapshot, CommitManifest
│   ├─ index.rs   walk, filter (hidden, /_rejected/, /.trash/), pair, extract dates
│   ├─ meta/      exif.rs, quicktime.rs, folder_date.rs (§7)
│   ├─ group.rs   gap clustering, FNV-1a IDs, titles, Undated
│   ├─ commit.rs  validate → journal → move → manifest
│   ├─ restore.rs
│   ├─ store.rs   load/save library.json v1/v2, debounced, atomic
│   └─ platform/  Trasher {freedesktop | macos | portal}, Fs, Clock traits
├─ crates/unswiper-ffi            UniFFI bindings for Swift
├─ crates/unswiper-gtk            relm4 app
├─ crates/unswiper-cli            index | groups | commit --dry-run | restore (tests + power users)
├─ help/                          the 11 .md pages, single source for both apps
└─ tests/fixtures/                golden corpus (§10.3)
```

**library.json:** schema **v1** is exactly today's Swift format, and the Rust reader/writer round-trips it byte-for-byte (sorted keys, pretty-printed, ISO 8601 seconds, unknown fields kept through `#[serde(flatten)]`). Schema **v2** adds `"schemaVersion": 2` and stores item paths relative to `config.rootPath`. Both apps read v1 and v2. v2 ships in the same release on both platforms. Undecodable files are backed up as `library.json.bad-<ts>`, never silently replaced.

## 6. Commit safety (both platforms, in core)
1. Canonicalise root and file. Require root + `/` as a prefix. Regular files only, symlinks never followed out of root.
2. Size exact, mtime ±1 s (±2 s on FAT/exFAT). In v2, also check `dev`+`ino`, recorded at index time.
3. Validate the whole set before moving any of it (same as Swift).
4. **Journal first:** write `commits/<stamp>.json` with `status: pending` entries, `fsync`, then move, updating each entry. On launch, any pending journal gives "Finish or roll back the interrupted commit". This is new for both apps.
5. Quarantine uses `rename(2)`. If it crosses devices: copy, fsync, verify size and hash, then remove the source.

## 7. Undated rule (Rob's decision)

**Status:** ships in Mac v0.1.2 (Swift, `FolderDate.swift`); the Rust core ports the same rule in Phase 1.
A file with no **real capture date** (no EXIF/QuickTime date) is *not* dated by mtime when its folder tells us where it belongs.

Resolution order per item:
1. Capture date (EXIF `DateTimeOriginal` → TIFF `DateTime` → QuickTime). This sets `dateSource = capture`.
2. **Folder date** from the path relative to the root, scanning from the deepest folder upward:
   - `YYYY/MM/DD`, `YYYY/MM`, `YYYY` as consecutive components (year 1980–2099, month 01–12 or 1–12, day valid for that month).
   - A single component `YYYY-MM-DD`, `YYYY_MM_DD`, `YYYY.MM.DD`, `YYYYMMDD`, or `YYYY-MM`, optionally followed by a name (`2026-01-01 Lisbon`). The year must be 1980–2099 and the date must be valid. `20260101` only counts if the whole component (or its leading token) is those 8 digits.
   - Precision is kept (`day | month | year`). The item is placed at noon local time of the first day of that span, and flagged `dateSource = folder`.
3. **Named folder** (no date in the path, but the item's folder isn't the root): the item joins the visit that holds most of its *dated siblings in the same folder* (`dateSource = sibling`). If no sibling is dated, all such items in that folder form their own visit titled with the folder name (`dateSource = folder-name`, the group sorts by folder name).
4. Only if the file sits directly in the root with no capture date → **Undated** group (id `undated`, unchanged).
- File mtime/birth time is no longer used for grouping (it's still recorded for validation). This is a deliberate change from v0.1.1: the Mac app adopts it in v0.1.2, and the Linux app ships with it in v0.2.
- Not dates: `IMG_2026…`, `12345`, `2026-13-40`, `0000`, versions like `v2026`. A bare `1999` component **is** a year folder, and `1999 Photos` counts as year 1999 (leading token). Only whole components or a leading date token count.
- **Tests:** a parser table with 60+ cases (valid, invalid, edge: Feb 29 in leap and non-leap years, `2026/1/5`, `2026/01/01/extra`, `Trip 2026-01-01`, unicode names). Property tests: any valid `NaiveDate` formatted in each supported pattern parses back to itself, and random non-date strings never parse. Fixture folders `2026/01/01/`, `2025-07-04 Beach/`, `Lisbon/` (mix of dated and undated), and root-level undated files. Expected groups are pinned as goldens.

## 8. Packaging and distribution

| Format | Name | Notes |
|---|---|---|
| Flatpak (Flathub) | `io.github.rranslam.Unswiper` | GNOME runtime, portals, bundled libheif + libde265. Avoid `--filesystem=home` |
| .deb | `unswiper` | `cargo-deb`, built on **ubuntu-24.04** so it installs on 24.04+ and Debian 13. Depends on `libgtk-4-1 (>= 4.14), libadwaita-1-0 (>= 1.5), libheif1 (>= 1.17), libheif-plugin-libde265, gstreamer1.0-plugins-good`; Recommends `ffmpeg` (optional) |
| AUR | `unswiper`, `unswiper-bin` | PKGBUILD from the release tarball. `-bin` uses the CI artifact |
| AppImage | `Unswiper-x86_64.AppImage` | Lowest priority |
| macOS DMG | `Unswiper-<v>.dmg` | Signed + notarized (§9.2) |

**Ubuntu 24.04 baseline (Rob's decision).** 24.04 ships GTK 4.14 and libadwaita 1.5, so compile gtk4-rs with feature `v4_14` and libadwaita-rs with `v1_5`, not newer. Feature limits: no `AdwMultiLayoutView`/`AdwSpinner`/`AdwButtonRow`/bottom-sheet (1.6+) and no newer `AdwToggleGroup`/`AdwWrapBox` (1.7+); use `AdwNavigationSplitView`, `AdwBreakpoint`, `AdwAlertDialog` (all 1.5) and a plain `GtkSpinner`. `gtk::FileDialog` (4.10+) is available. libheif 1.17 on 24.04 is fine for HEIC decode. The Flatpak uses the current GNOME runtime regardless. CI builds and tests on 24.04 and runs install tests on 24.04, 26.04, Debian 13 and Arch.

**libheif is a hard dependency (Rob's decision):** HEIC decode is required for thumbnails/compare, not an optional preview; packages depend on it and the app shows a clear error if the HEVC plugin is missing. **ffmpeg stays optional:** dates use pure Rust, thumbnails use GStreamer; ffmpeg is only an optional fallback for odd video containers.

Desktop file, AppStream metainfo and icons all use `io.github.rranslam.Unswiper`. Releases live at `https://github.com/rranslam/unswiper/releases`. Beginner install steps for each distro go in the README, as explicit as the macOS steps.

## 9. macOS on the shared core

### 9.1 UI decision: keep SwiftUI over UniFFI (recommended)
- **Keep SwiftUI.** The 729 view lines plus the app shell already work and feel native. GTK on macOS looks foreign, needs a bundled GTK runtime (~60–80 MB), and has weak menu bar, Retina and notarization support. Slint would look neutral, but it would throw away a working UI and need a Help and dialogs rewrite.
- `unswiper-ffi` exposes `index`, `group`, `validate`, `commit`, `restore`, `load/save` with UniFFI. Build `UnswiperCore.xcframework` (arm64 + x86_64) with `cargo` + `xcodebuild -create-xcframework` and add it as a SwiftPM `binaryTarget`. Then delete `LocalTreeIndexer`, `CommitService` and most of `LibraryStore` (~560 lines) from Swift.
- Mac-only bits stay in Swift: bookmarks, `QLThumbnailGenerator`, `trashItem` (passed in as a UniFFI callback implementing `Trasher`).
- Revisit Slint only if a third desktop UI (Windows) becomes a goal.

### 9.2 Signing and notarization (Rob is enrolling in the Apple Developer Program)
1. Rob creates a **Developer ID Application** certificate and an **app-specific password** or **App Store Connect API key** (recommended).
2. Export the cert as `.p12`. Add repo secrets: `MACOS_CERT_P12_BASE64`, `MACOS_CERT_PASSWORD`, `KEYCHAIN_PASSWORD`, `AC_API_KEY_ID`, `AC_API_ISSUER_ID`, `AC_API_KEY_P8_BASE64`, `TEAM_ID`.
3. `release.yml` (macOS job): create a temporary keychain → import the cert → `build-dmg.sh` signs with `codesign --force --options runtime --timestamp --sign "Developer ID Application: … ($TEAM_ID)"` (sign the nested UniFFI dylib/framework first, **no `--deep`**). Add an entitlements file (no sandbox, no special entitlements needed today). Sign the DMG → `xcrun notarytool submit --wait` → `xcrun stapler staple` the DMG → verify with `spctl -a -vvv -t install` and `codesign --verify --strict` → upload.
4. Keep ad-hoc signing as the fallback when secrets are missing (forks, PRs).
5. The README and Help page `08-install-update.md` drop the "Open Anyway" steps once the first notarized release is out.
6. Bundle ID: **`io.github.rranslam.Unswiper`** on macOS too (decided 14 Oct 2026, §14.3). Preferences are migrated once from `com.rranslam.unswiper`; library data never depended on the ID.

## 10. Automated testing

### 10.1 Rust core unit tests (`cargo test`, target ≥ 85% line coverage of core)
- **Grouping:** empty, single item, gap exactly 36 h (same visit) vs 36 h + 1 s (split), custom `visitGapHours`, ordering by total bytes, title formats (same day / range), most-common folder ties, **FNV‑1a IDs equal to the Swift values** for fixed inputs (cross-checked against an XCTest-generated table), Undated last.
- **Date extraction:** EXIF DTO vs TIFF DateTime precedence, malformed strings, ≤ 1980 rejection, QuickTime `mvhd` epoch (1904) conversion, Apple creationdate with offset, local-zone interpretation (`TZ` set per test), folder-date parser table (§7).
- **Pairing:** `IMG_1.HEIC + IMG_1.MOV + IMG_1.AAE`, mixed case, `.mp4` *not* paired, JPG+HEIC same stem (two items, sidecars on the HEIC anchor), orphan AAE ignored, same stem in different folders not paired.
- **Validation:** outside root, `../` and symlink escapes, root prefix trick (`/photos2` vs `/photos`), missing, directory instead of file, size change, mtime ±0.9 s ok / ±1.1 s fail, missing `modifiedAt`, inode swap (v2), sidecar change blocks the whole set.
- **Manifest/restore:** journal states, latest-manifest selection by name, `.restored` rename only on full success, target-exists skip, empty `resultPath`, recreating missing parent folders.

### 10.2 Property tests (`proptest`)
- **Grouping invariants:** every non-undated item is in exactly one group. Within a group, consecutive gaps ≤ gap. Between adjacent clusters (by time), gap > gap. IDs are unique and unchanged when items are added after the last cluster. Output doesn't depend on input order.
- **Commit → restore round trip:** generate a random tree (depth ≤ 4, names with unicode, spaces and case collisions, sidecars) and random decisions. Commit in quarantine mode (and trash mode against a temp `XDG_DATA_HOME`), then restore. The tree is **byte-identical**: paths, contents, sizes and mtimes. Dry run never changes the tree.
- Folder-date parse/format round trip (§7).

### 10.3 Golden fixture corpus (`tests/fixtures`, < 2 MB, generated by `xtask gen-fixtures` with exiftool, checked in)
- JPEG with DTO + OffsetTime. JPEG with only TIFF DateTime. HEIC (iPhone) + MOV (Live) + AAE. JPEG + XMP. MP4 with `mvhd` only. MOV with `com.apple.quicktime.creationdate`. MKV (needs GStreamer, feature-gated). Corrupt EXIF. 1970-dated file.
- Undated files in `2026/01/01/`, `2026/02/`, `2025-07-04 Beach/`, `Lisbon/` (with and without dated siblings), and the root.
- `expected.json` holds per-file date, source, kind, pairing and group ID. The test compares field by field and prints the diff on failure.

### 10.4 Integration tests (temp dirs, `tempfile`)
- **freedesktop Trash:** home trash layout, `.trashinfo` contents and URL-encoding, name collisions (`a.jpg`, `a.1.jpg`), restore from info.
- **Other drives:** in CI, loopback **ext4** and **vfat/exFAT** images mounted with `udisksctl` or as root in a privileged container: `$topdir/.Trash-$uid` creation, a refused `.Trash` (not sticky), read-only mount → fail and suggest quarantine, never a cross-device copy into home trash.
- **Permission errors:** unreadable file, read-only directory, file with immutable attribute. The set fails, the others proceed, and the results list each failure.
- **Changed between index and commit:** edit, replace (same size, new inode), rename, delete, swap a sidecar. Each is refused with the right message.
- **Crash safety:** kill the process (fault injection through a `Fs` fake) after N moves → the pending journal recovers it.

### 10.4a Folder-watching tests
- Integration (`tempfile`): create, delete, rename, modify and move-in/out files while watching; assert the index updates within the debounce window and that decisions on unchanged items survive.
- Self-move suppression: a commit does not produce spurious "missing" items.
- Overflow/`ENOSPC` fallback to rescan (simulated via a low-limit test backend); polling fallback on a FUSE/loopback mount.
- Burst test: 5k files copied at once yields one batched re-index, not 5k.
- macOS `test-macos` runs the same suite on FSEvents.

### 10.5 library.json cross-platform compatibility
- Checked-in samples produced by the **current Swift app** (v0.1.1): empty, with `keepTargets`, without the optional fields (pre-P0), with `bookmarkData`, plus manifests in trash and quarantine modes.
- Rust: decode → encode yields identical bytes. v1 → v2 migration and back. Unknown fields kept.
- Swift (XCTest): decode JSON that Rust wrote. Both suites run in CI on every PR.

### 10.6 UI tests
- **relm4 component tests:** drive component messages (`Keep`, `Reject`, `OpenCommit`, `Confirm`) against a fake core and assert the model state and the emitted commands, with no display needed.
- **Headless smoke:** start `unswiper-gtk` under **Xvfb** (X11) and **weston --backend=headless** (Wayland) with `--fixture-library tests/fixtures --script smoke.ron`. Open the library, reject 2 items, dry-run commit, open Help (assert 11 topics, like `--help-selftest`), quit. Check exit code 0 and the accessibility tree via AT-SPI (`dogtail`/`pyatspi`). Save a screenshot as a CI artifact.
- Manual checklist each release on GNOME (Ubuntu), KDE Plasma and Hyprland (Omarchy).

### 10.7 Packaging tests
- **.deb:** `docker run ubuntu:26.04` (and `debian:trixie`) → `apt install ./unswiper_*.deb` → `unswiper --version`, `unswiper --help-selftest`, `unswiper-cli index tests/fixtures` → `apt remove` leaves nothing behind.
- **AUR:** `archlinux:latest` container, non-root builder → `makepkg -si` from the PKGBUILD against the release tarball → same smoke commands. `namcap` lint.
- **Flatpak:** `flatpak-builder --user --install` with the GNOME SDK in the `ghcr.io/flathub-infra/flatpak-github-actions` container. `flatpak run io.github.rranslam.Unswiper --help-selftest`. `flatpak-builder-lint` on the manifest and AppStream file.
- **macOS:** `build-dmg.sh` then `--help-selftest` (already there), plus `spctl` and `stapler validate` once signed.

### 10.8 CI matrix (GitHub Actions)
| Job | Runners / containers | Steps |
|---|---|---|
| `lint` | ubuntu-24.04 | `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, `cargo deny check` (licenses: MIT-compatible only, so it flags GPL/HEVC issues), `typos` |
| `test-linux` | ubuntu-24.04 (x86_64), ubuntu-24.04-arm (aarch64), container `archlinux:latest` (x86_64) | `cargo nextest run`, proptest (256 cases on PRs, 10k nightly), integration incl. loopback mounts (privileged step) |
| `test-macos` | macos-14 (arm64), macos-13 (x86_64) | `cargo nextest` (core + macOS trasher), `xcodebuild test`/`swift test` for the Swift app + FFI |
| `ui-smoke` | ubuntu-24.04 | Xvfb + weston headless runs |
| `coverage` | ubuntu-24.04 | `cargo llvm-cov`, **fail below 80% overall / 85% core** (approved by Rob), upload to Codecov |
| `install-matrix` (tags + nightly) | containers `ubuntu:24.04`, `ubuntu:26.04`, `debian:13`, `archlinux:latest` | install the .deb / AUR package, check `libheif` present, launch headless, run a watch + index smoke test |
| `package` (tags + nightly) | ubuntu-24.04, -arm, arch container, macos-14 | .deb/AUR/Flatpak/DMG builds + §10.7 container installs |
| `release` (tags `v*`) | as above | Attach .deb, tarballs, .flatpak, signed DMG; update AUR via `KSXGitHub/github-actions-deploy-aur` |

Branch protection on `main` requires `lint`, `test-linux`, `test-macos` and `coverage`.

### 10.9 Interim: XCTest for the Swift app (v0.1.x)
Add a `testTarget("UnswiperTests")` to `Package.swift` and split the logic into an `UnswiperKit` library target, so the tests don't link the app's `@main`. Cover `VisitGrouper` (gap edges, IDs), `CommitService.validate` (all failure messages), commit+restore on a temp dir in quarantine mode, LibraryStore decode of old JSON (and the silent-reset bug), pairing via `LocalTreeIndexer.index` on a temp tree, and `HelpLibrary.topics.count == 11`. Run `swift test` in a new `ci.yml` on `macos-14` for every PR. About 1.5 days. These tests later become the oracle for the Rust port (§10.1 ID table, §10.5 samples).

## 11. Phases and effort (one developer + AI help)

| Phase | Content | Tests delivered | Effort |
|---|---|---|---|
| **0** Setup and rename | ✅ **Done 13 Oct 2026:** repo renamed `visit-media-reduce` → **`unswiper`** (GitHub keeps redirects for web, git and API), links updated, local remote updated. Remaining: Update README badges and release links, `HelpLibrary.issuesURL/releasesURL`, CHANGELOG links, `release.yml` (artifact names unchanged). Run `git remote set-url origin https://github.com/rranslam/unswiper.git` on Rob's Mac. Reserve Flatpak/AUR/deb names. Send notes manager the new links (repo, releases, issues). Ship v0.1.2 with the new URLs. Freeze schema v1 samples. Generate the fixture corpus. Set up the Cargo workspace. CI skeleton. Spike Flatpak trash and the Hyprland portal | XCTest interim suite (§10.9), compat samples, CI `lint` | 4–5 days |
| **1** Core: model, index, dates, pairing, grouping, store | Port bit-for-bit, plus the folder-context Undated rule, plus CLI | Unit + property + goldens (§10.1–10.3, 10.5) | 6–8 days |
| **1b** Live folder watching | `watch` module (notify/inotify, debounce, incremental re-index, self-move suppression, ENOSPC/polling fallback) | Watch tests (§10.4a) | 2–3 days |
| **2** Commit/restore | Validation, journal, quarantine, freedesktop trash, restore | Integration + round-trip property (§10.2, 10.4) | 4–6 days |
| **3** GTK UI | Setup, visits, reduce grid, compare, keep target, confirm, progress, restore | relm4 component tests | 7–9 days (+0.5–1 for the 24.04 / libadwaita 1.5 baseline) |
| **4** Thumbnails, perf, Help | Cache, HEIC/video, 50k-item perf, Help window from shared `help/` | Headless smoke, perf benchmark (`criterion`) | 4–5 days |
| **5** Linux packaging and release | Flatpak, .deb, AUR, (AppImage), install docs, **v0.2.0** Linux | Container install tests, full CI matrix | 4–5 days |
| **6** macOS on the shared core | `unswiper-ffi` + xcframework, SwiftUI calls core, delete the Swift logic, schema v2 + new Undated rule on Mac, **signing + notarization** in `release.yml` | `test-macos` job, XCTest against FFI, notarization check | 6–8 days |

**Total: about 38–50 working days (8–10 weeks)**, up from 35–46 in rev 2 (+2–3 days folder watching, +0.5–1 day 24.04 baseline; the repo rename is already done). Linux v0.2.0 lands after Phases 0–5 (32–42 days). The macOS core switch and signed DMG follow in Phase 6. Testing is about 30% of each phase's effort and is included above.

## 12. Risks
- **Flatpak sandbox:** folder-grant persistence and trash restore are the biggest unknowns. Spike in Phase 0.
- **Hyprland:** needs `xdg-desktop-portal-gtk` *as well as* `-hyprland` for the file chooser. Detect it and explain in-app.
- **HEIC/HEVC codecs:** libheif is now required; distros that ship it without the HEVC plugin (Fedora-style) get a clear in-app error and install hint. Dates still don't need decoding.
- **inotify limits / network mounts:** large libraries can hit `max_user_watches`; network/FUSE mounts may not emit events. Fall back to rescan/polling (§3.0).
- **24.04 baseline:** pins us to GTK 4.14 / libadwaita 1.5 widgets until 24.04 is dropped.
- **Trash on removable, network or exFAT drives:** fail to quarantine, never delete. Restore needs the drive mounted.
- **Case sensitivity:** pairing key is case-folded (same as macOS). Warn about `a.jpg`/`A.JPG` siblings that would collide on macOS.
- **Timestamps:** the JSON stores whole seconds, so keep the ±1 s check (±2 s FAT). Add the inode check.
- **ID compatibility:** any change to FNV input or float/second rounding breaks saved `keepTargets`. Pinned by the cross-language ID table.
- **Behaviour change:** the new Undated rule regroups some libraries. Both apps change together in v0.2 with a CHANGELOG note, and re-indexing is required.
- **Repo rename:** old clones and links keep working through redirects, but GitHub breaks the redirect if a new repo named `visit-media-reduce` is ever created. Don't reuse the old name.
- **Silent library reset bug** in v0.1.x: fix early (Phase 0, v0.1.2).
- **UniFFI + notarization:** nested binaries must be signed inside-out with hardened runtime. Test in CI before the first signed release.

## 13. Roadmap after the Linux port
Order approved 14 Oct 2026 (§15.8): Linux v0.2.0 → **v0.3: blur detection (§15) + video playback (§16)** → geo grouping → Google Photos.

1. **Geo grouping:** split and merge visits by GPS (EXIF GPS, QuickTime ISO 6709) with distance thresholds. Offline reverse-geocoding for titles.
2. **Google Photos source:** read-only via the Photos Library/Picker API. Decisions export to an album, since the API can't delete.
3. Extra RAW formats (CR2/CR3/NEF/ARW/ORF/RAF), "Show in file manager", restore any past commit, (live folder watching moved into v1).

## 14. Decisions

### 14.1 Resolved (13 Oct 2026)
- **Plan approved** by Rob.
- **Repo rename:** done, now `rranslam/unswiper`.
- **Apple Developer signing:** still pending; Rob is setting up the Developer Program. §9.2 applies once the certificate exists.
- **Minimum Linux:** Ubuntu 24.04 LTS (GTK 4.14, libadwaita 1.5), see §8.
- **HEIC:** libheif required (hard dependency).
- **ffmpeg:** optional.
- **Live folder watching:** in v1 (§3.0, Phase 1b, §10.4a).
- **Coverage gate:** 80% overall / 85% core approved.

### 14.2 Resolved (13 Oct 2026, follow-up)
- **AppImage:** stays in the plan (Phase 5, alongside Flatpak, .deb and AUR).
- **Repo visibility:** `rranslam/unswiper` stays **private until v0.2.0** ships.
- **Apple Developer ID:** pending; Rob is setting it up. §9.2 applies once the certificate exists.
- **Only open item:** the macOS bundle ID (resolved; see §14.3).

### 14.3 Resolved (14 Oct 2026)
1. **macOS bundle ID:** `io.github.rranslam.Unswiper`, the same as Linux (`build-dmg.sh` Info.plist). Data in
   `~/Library/Application Support/Unswiper` is not keyed by the ID, so libraries, decisions and commit records
   carry over unchanged. `BundleIDMigration` copies the old `UserDefaults` domain once (window state). macOS will
   ask again for folder-access permissions (they are per bundle ID) and treats it as a new app in Login Items/
   Launchpad. Nothing is open in §14 any more.

## 15. Blur / out-of-focus detection (new feature, Rob 14 Oct 2026)

**Goal:** find out-of-focus or shaky photos automatically and pre-mark them so they're easy to drop. Runs offline with no external API. Setting **on by default**. Evaluation: `docs/blur-evaluation.md`.

### 15.1 Approach
- **New crate `crates/unswiper-blur`** (MIT, pure Rust: `image`, `imageproc`, `rayon`). It decodes through the core's existing image path (libheif for HEIC), so no new system dependencies are needed and Flatpak/AppImage are unaffected.
- **Where it runs (both, sharing one crate):**
  1. **Core background job** (`unswiper-core::quality`). After indexing, unscored photos go on a low-priority worker queue: `nice`/QoS background, N−1 threads, cancellable. The open visit is scored first. Live folder watching enqueues new files.
  2. **CLI `unswiper-blur`**: `unswiper-blur scan <dir|library.json> [--json] [--threshold] [--write-cache]`. Use it for big backlogs, headless runs, and threshold tuning. It shares the cache format, so the app picks its results up.
- **Score:**
  1. Decode to a 1024 px long edge (use the embedded preview when it's large enough), convert to grayscale, Gaussian σ=1.
  2. Split into a 4×4 tile grid. Take the Laplacian variance and Tenengrad for each tile, and skip flat tiles (low intensity variance).
  3. `sharpness = p90 over tiles` (tolerates bokeh, since a sharp subject anywhere passes). If AF-point metadata exists, use the AF tiles instead.
  4. Also store `global` and a `motion` flag (gradient-orientation anisotropy) for display.
- **Decision (two-stage, conservative):**
  - *Relative:* within a visit/burst (≥3 photos taken within a few seconds), flag a photo if `sharpness < 0.35 × burst max`.
  - *Absolute floor:* flag a photo with `sharpness < T_abs`, where `T_abs` is calibrated on the fixture set and set low.
  - *Never flag:* low-confidence cases (dark: mean luma < 40/255; very high ISO), Portrait/depth photos, screenshots and non-camera images, videos.
  - Sensitivity setting: Low / **Normal** / High scales both thresholds.

### 15.2 Semantics: suggest, never move
- Auto-reject = **"reject-suggested"** (a new decision source), shown as Reject with a "blurry" badge. It counts toward Commit like any reject, but the confirmation lists "N suggested as blurry" separately, and nothing moves until the user commits the visit (the existing Commit-open-visit rule still applies).
- **Manual decisions always win.** Never override Keep/Reject/Undecided that the user set. Pressing U on a suggested item clears it and records `blurDismissed`, so rescans don't re-flag it.
- If the setting is turned off, suggestions become hidden and uncounted (cached scores stay).
- Filter: "Show blurry suggestions" in the visit view, plus "Keep all in visit" / "Reject all suggested."

### 15.3 Data / schema
- Additive, optional fields per item in library.json: `quality: { v: 1, sharpness, global, motion, confidence, scoredAt, fileSize, mtime }` and decision source `"auto-blur"`. Cache key = size + mtime + algorithm version. A changed file or a bumped algorithm triggers a rescore.
- Works with v1 and v2: the Rust reader keeps unknown fields (`serde(flatten)`). **Caveat:** the current Swift app's Codable reader *ignores* unknown keys but *drops* them on save. Until Phase 6 (Mac on core), the Mac app either (a) adds the optional fields to its model (small change), or (b) keeps scores in a sidecar `quality-cache.json`. Recommendation: **sidecar cache** for scores (rebuildable, can't corrupt library.json), and only the suggestion/dismissal state in library.json via (a).

### 15.4 Settings and UI
- Setting "Detect blurry photos" (default **On**), Sensitivity (Low/Normal/High), and "Rescan library."
- Linux: Preferences window (`AdwPreferencesWindow`, switch row + combo row), progress in the header bar ("Checking sharpness 120/900"), a blur badge on thumbnails.
- macOS: Settings scene (⌘,), Toggle + Picker, progress in the toolbar, and the same badge. Before Phase 6, the Mac app calls the `unswiper-blur` crate through UniFFI (or runs the CLI as a helper) rather than re-implementing it in Swift. Both must produce identical scores (cross-check test).
- Help page: what's flagged, why, and how to undo.

### 15.5 Performance targets
- ≤ 40 ms per 12 MP JPEG and ≤ 80 ms per HEIC per core on a 2020-era laptop (decode-bound). 1,000 photos in under 30 s on 4 cores.
- UI stays responsive (scoring off the main thread, no frame > 16 ms). Memory < 200 MB extra.
- The rescan cost is near zero when the cache hits.

### 15.6 Test plan
- **Labeled fixture set** `tests/fixtures/blur/`: about 60–100 small (1024 px) images, CC0 or Rob's own, with labels in `labels.csv` (sharp / defocus / motion / bokeh-sharp / night-sharp / night-blurry / low-texture-sharp / burst sets). Synthetic blur (Gaussian, motion kernels) is generated from sharp originals by `xtask`.
- Unit tests: tile scoring, flat-tile skip, monotonicity (more blur gives a lower score: proptest), resolution invariance (same image at 2 sizes within ±10%), relative burst logic, manual-decision precedence, cache invalidation.
- **Accuracy gate in CI:** on the fixture set, **precision ≥ 0.95** for suggestions (false rejects are the costly error) and recall ≥ 0.70; zero flags on bokeh-sharp and night-sharp.
- Benchmark (`criterion`), not gating, tracked over time.
- Cross-check: the Rust CLI vs the Mac binding produce identical JSON.
- Coverage: the crate is inside the 85% core gate.

### 15.7 Placement and effort
Order: **Flatpak (finish) → AppImage in CI → v0.2.0 Linux release → 15. Blur detection (v0.3) → Geo grouping → Google Photos.**
Rationale: packaging is nearly done and gates v0.2. Blur is self-contained in core, which makes it the most useful next culling feature. It also benefits Google Photos later (scoring downloaded thumbnails). The Mac app gets it fully in Phase 6, with an interim UniFFI/CLI bridge.
Effort: 6–9 days. Crate + CLI 2–3, core job + cache 1–2, Linux UI 1, Mac UI/bridge 1–2, fixtures/calibration 1–2.
Optional v2: a permissively licensed ONNX model through `tract` for better bokeh handling.

### 15.8 Decisions (Rob, 14 Oct 2026)
- **Approved:** the plan above and the order in §15.7: Flatpak → AppImage in CI → **Linux v0.2.0** → **blur detection (v0.3)** → geo grouping → Google Photos.
- **Decided:** blurry photos are **pre-marked Reject and moved only on commit**. They are never moved or deleted automatically, manual decisions always win, and the existing commit-the-open-visit rule applies.
- **Deferred** until v0.3 work starts: the score storage (sidecar cache vs library.json fields, §15.3), the Mac bridge (UniFFI vs CLI helper, §15.4), the sensitivity defaults and accuracy-gate numbers (§15.6), and the optional ONNX model.

## 16. Video playback during review (new feature, Rob 14 Oct 2026; plan only)

Today a video tile only shows a frame thumbnail, so deciding Keep/Reject on a clip means opening it in another app. Users need to play videos inside Unswiper while reviewing a visit, on both Linux and Mac. Nothing here is implemented yet.

### 16.1 Linux (GTK 4)
- Use **`gtk::Video`** with a **`gtk::MediaFile`** (`MediaFile::for_file`). GTK 4's media backend is GStreamer (`libmedia-gstreamer`), which the Flatpak (GNOME 50 runtime) and the AppImage already ship for thumbnails. No new dependency.
- AppImage: also bundle GTK's media module (`gtk-4.0/<ver>/media/libmedia-gstreamer.so`) and the audio side of playback: add the `autodetect`, `pulseaudio`, `pipewire`, `volume`, `audioconvert` and `audioresample` plugins to the list in `build-appimage.sh` (video goes through GTK's own sink). Check `GTK_MEDIA` isn't overridden.
- The viewer is an `adw::Dialog` (or a full-window overlay page) holding the `gtk::Video` with GTK's built-in controls (play/pause, scrub bar, volume).

### 16.2 Mac
- **AVKit `AVPlayerView`** (wrapped in `NSViewRepresentable`) with an `AVPlayer` per open clip, `controlsStyle = .inline`. HEVC/H.264 decode in hardware; no extra code for codecs.

### 16.3 UX (same on both)
- Video tiles get a **play badge** (▶ plus duration) so they're recognisable at a glance.
- **Space** or **double-click** opens the viewer on the selected item; Space again (or Esc) closes it. Left/Right arrows go to the previous/next item, so photos open in the same viewer as a large preview.
- Playback starts **muted** (a mute toggle remembers the last choice per session); it autoplays and loops short clips (< 10 s).
- Scrubbing via the built-in controls; **J/K/L-free**: the existing **K / R / U keys keep working inside the viewer** (Keep / Reject / Undecided apply to the shown item and show the badge), so a whole visit can be reviewed without leaving the viewer.
- **Live Photos (option, off by default):** play the paired `.mov` motion part when the still is opened, or on hover/press-and-hold. Uses the existing Live Photo pairing in the core.
- Playing never changes files; decisions are still applied only by Commit on the open visit.

### 16.4 Codecs
- Linux: H.264 and HEVC need `gst-libav` (or VA-API). The **Flatpak** gets them from the auto-installed `org.freedesktop.Platform.codecs-extra` extension; the **AppImage** bundles `libav` (already done for thumbnails). On a distro build without codecs, the viewer shows "This video format can't be played here" with an **Open in default app** button (`gtk::FileLauncher`), never a blank window.
- Mac: AVFoundation covers H.264/HEVC/ProRes; unsupported files get the same fallback via `NSWorkspace.open`.

### 16.5 Tests
- Core/state (Rust): the viewer navigation model (next/prev within the visit, filters honoured) and "decision keys act on the viewed item", unit-tested in `state.rs` without GTK.
- GTK: an Xvfb test that opens the viewer on the generated H.264 test clip and checks that the `MediaFile` reaches *prepared* with a duration > 0 and no error (both AppImage CI arches and the Flatpak build run it via `--present-viewer` like `--present-dialog`).
- Mac: XCTest for the navigation model; a UI test that the player item becomes `.readyToPlay` on a bundled short HEVC clip.
- Manual checkout step: play an iPhone HEVC clip in each package; Live Photo option.

### 16.6 Open questions
1. Viewer as a dialog over the grid, or a full-window page with a back button (better for large screens)?
2. Autoplay and loop defaults, and whether the mute state should persist across launches.
3. Live Photo motion: on open, on hover, or only on press-and-hold?
4. Trim/"keep only part of a clip": out of scope (Unswiper never edits media)? Recommend yes, out of scope.
5. Should photos also open in the viewer (large preview with zoom)? Cheap to add with the same navigation.

### 16.7 Release slot
**Decided (Rob, 14 Oct 2026): all video playback ships in v0.3, together with blur detection (§15).** That covers basic playback (viewer, Space/double-click, scrubbing, mute, decision keys in the viewer) and the Live Photo motion option, on Linux and Mac. There is no v0.2.1 playback release. Order otherwise unchanged (§15.8).
