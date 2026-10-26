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
| Grouping | Gap > 36 h | Gap `visitGapHours` (config, default 36), split when gap is **strictly greater**. Groups sorted by **start date, oldest first** (ties by ID; undated folder sets last). Changed 26 Oct 2026 by Rob from total bytes, largest first. ID = `visit-` + FNV‑1a‑64 hex of `"<firstPath>|<startEpochSeconds>"`. Title `MMM d[–MMM d] · <top-level folder>`. Folder hint = most common first path component under the root | Port bit-for-bit so IDs (and `keepTargets`) match across apps |
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
Order approved 14 Oct 2026 (§15.8): Linux v0.2.0 → **v0.3: blur detection (§15) + video playback (§16)** → **v0.4: photo info (§17)** → geo grouping → Google Photos.

1. **Geo grouping:** split and merge visits by GPS (EXIF GPS, QuickTime ISO 6709) with distance thresholds. Offline reverse-geocoding for titles.
2. **Google Photos source:** see §19 (Google Takeout import, decided 15 Oct 2026). The old idea "decisions export to an album" no longer works: since March 2025 the API can only add **app-created** items to albums, and it still can't delete.
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

### 15.9 v0.3 milestone 1 — what was built (15 Oct 2026)
Blur detection end to end on Linux plus the core. Decisions taken while building (open to change):
- **Storage (the deferred §15.3 question):** recommendation (b) as written. Scores live in the sidecar `quality-cache.json` (support folder, next to library.json; keyed by size + mtime + algorithm version; rebuildable). library.json gets only the optional item keys `decisionSource: "auto-blur"` and `blurDismissed: true`, and config keys `blurDetection` (absent = on) and `blurSensitivity` (absent = normal), all omitted when unused, so v1 files are byte-identical. Code: `unswiper-core::quality`.
- **Crate layout:** `crates/unswiper-blur` holds the score, decision rules, cache, parallel scan and the `unswiper-blur` binary. It depends on `unswiper-core` (not the other way round), so the core stays free of image decoding. The app's background job lives in `unswiper-gtk::blur` instead of `unswiper-core::quality` for the same reason.
- **Score:** as §15.1, with two changes found in testing: (1) tiles with low contrast (std < 30) are scaled up to that contrast, so faint but sharp scenes (fog, pale text) aren't flagged; (2) the flat-tile cutoff is std 4. Tenengrad and AF-point tiles are not used yet. The embedded EXIF preview is not used yet (full decode, then resize).
- **Rules:** absolute floor (Low 7 / Normal 13 / High 25; was 8/15/30 before the 15 Oct real-photo calibration), burst-relative (≥3 photos ≤3 s apart; 0.25 / 0.35 / 0.45 × burst max), plus a shake rule (gradient anisotropy ≥ 0.7 (was 0.5) and sharpness < 4 × floor). Never flagged: confidence < 0.5 (mean luma < 40, or fewer than 2 textured tiles), videos, PNG/GIF/BMP, undated photos in bursts. Portrait/depth and high-ISO detection need EXIF and are not done yet.
- **Settings UI:** a **Blurry Photos** section in the main menu (toggle, sensitivity radio submenu, "Check now", clear / confirm all in visit) rather than an `AdwPreferencesWindow`, to match the existing Commit Mode menu. Easy to move into a Preferences window later.
- **Background work:** all cores but one (rayon), cancellable, open visit scored first; not yet `nice`d.
- **Accuracy gate:** synthetic only for now (`crates/unswiper-blur/tests/accuracy.rs`, generated scenes: sharp, defocus, motion, bokeh-sharp, night, low-texture, faint, bursts). Currently precision 1.00 and recall 0.98 against the ≥ 0.95 / ≥ 0.70 gate. **Calibration on real photos is still needed** before release: a labeled set of ~60–100 of Rob's photos.
- **Not in M1:** Mac app parity (bridge choice UniFFI vs CLI helper still open), the cross-check test Rust vs Mac, `criterion` benchmark, AppImage/Flatpak shipping of the `unswiper-blur` CLI.

### 15.10 v0.3 milestone 3 — calibration and Mac parity (15 Oct 2026)
- **Calibration:** 85 of Rob's real photos (not committed). The shake rule flagged sharp seascapes and panels, so anisotropy went 0.5 → 0.7 and the floors went 8/15/30 → 7/13/25. False positives on sharp photos are now Low 1.3 %, Normal 6.4 %, High 12.8 % (were 6.4 / 14.1 / 23.1 %). Full numbers and the method are in `docs/blur-evaluation.md`.
- **Mac bridge: bundled CLI helper**, not UniFFI. `Unswiper.app/Contents/MacOS/unswiper-blur` is a universal Rust build without libheif; it reads HEIC through macOS `sips`.
  - The app saves library.json, then runs `unswiper-blur scan library.json --json --write-cache --sensitivity S --only <visit list>` for the open visit, then once more for the whole library. It applies the verdict in Swift with `BlurRules`, a line-for-line port of `unswiper_core::quality`.
  - Why the helper: it needs no FFI, no Xcode project change and no Swift–Rust ABI. It shares the score cache (`quality-cache.json`) and the rules with Linux. A crash in the helper can't take the app down.
  - The cost is one process per check, which is cheap because cached scores aren't recomputed.
- **Mac library.json** now models `decisionSource`/`blurDismissed` (items) and `blurDetection`/`blurSensitivity` (config), so they survive Mac saves. Before this, a Mac save dropped them. Other unknown keys are still dropped by the Mac app; that's tracked for 1.0.
- **Mac UI:**
  - an orange **Blurry?** badge
  - an All/Photos/Videos/**Blurry** filter
  - a per-visit **Blurry?** menu
  - a **Blurry Photos** menu bar menu
  - **U**/Clear in the grid
  - a commit-sheet note
  - a "Checking sharpness…" line
  - a Help page (13 topics)
- **Tests:** XCTest for the rules (same cases as Rust), key round-trip, the AppState flow with a fake checker, helper argument and JSON parsing, and an end-to-end test with the real helper. CI builds the helper and sets `UNSWIPER_BLUR_HELPER`. The Mac CI now also runs on `v*` branches.

## 16. Video playback during review (new feature, Rob 14 Oct 2026; M2 built 15 Oct 2026, see §16.8)

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

### 16.8 M2 built (15 Oct 2026)
Answers to §16.6 as built (change any of them on request):
1. **Full-window page** with a **Grid** back button (not a dialog): more room for video, and the sidebar stays visible on Linux.
2. Autoplay on open; loop under 10 s (and Live Photo motion always loops); **mute is per session only** (starts muted every launch).
3. Live Photo motion plays **on open** in the viewer when the option is on (Linux menu ☰ → Viewer, Mac View menu); no hover. Stored as `livePhotoMotion` in the config extras (Linux).
4. Trim: out of scope.
5. Photos open in the same viewer as a large preview (no zoom yet).

Linux: `player.rs` (pure rules: motion clip = a `.mov/.mp4/.m4v` sidecar of a photo, loop rule, duration format), viewer model in `state.rs` (navigation without wrap, follows filters/removals, decisions act on the viewed item), `gtk::Video` + `MediaFile` in `ui.rs`, durations on tiles via `video::duration`. Smoke test `viewer_plays_a_video` runs `--present-viewer` on a generated clip and needs `viewer: prepared duration_ms>0`. Timed runs (`--quit-after`) are `NON_UNIQUE`. AppImage bundles `libmedia-gstreamer.so` (GTK_PATH, GTK_MEDIA=gstreamer) plus autodetect/pulseaudio/pipewire/volume/audioconvert/audioresample.
Mac: `ViewerModel.swift` (pure, XCTest `ViewerModelTests`) and `MediaViewer.swift` (AVPlayerView `.inline`, muted start, loop, NSWorkspace fallback). **Not compiled on the box**: build and test on a Mac.
Not done: AppImage CI smoke of `--present-viewer` (the Rust test-linux job covers it), photo zoom, Mac `.readyToPlay` UI test.

## 17. Photo info (EXIF) view (new feature, Rob 15 Oct 2026; plan only)

Rob tested the 0.3.0-dev Mac build: blur detection is good as is. Artistic, intentional blur is hit and miss, which is acceptable because **U** clears a suggestion for good. The next request is a way to see the key camera and settings info while choosing between similar shots. Nothing here is implemented yet.

### 17.1 Fields (one shared list, same order on both platforms)
| Group | Field | Photo source | Video source |
|---|---|---|---|
| Camera | Camera (Make + Model), Lens (LensMake/LensModel) | EXIF 0x010F/0x0110, 0xA433/0xA434 | QuickTime `com.apple.quicktime.make/model` |
| Settings | Focal length (+ 35 mm equivalent), Aperture (f/), Shutter (1/250 s), ISO, Flash (fired / off), Exposure compensation (±EV) | EXIF FocalLength, FocalLengthIn35mmFilm, FNumber, ExposureTime, PhotographicSensitivity, Flash, ExposureBiasValue | none (shown as "—") |
| When | Date/time **with time zone** (OffsetTimeOriginal; otherwise "time zone unknown") and the date source the index used (EXIF / file name / folder / file time) | EXIF DateTimeOriginal + OffsetTimeOriginal | QuickTime creation date (UTC) / `com.apple.quicktime.creationdate` (has offset) |
| Image | Dimensions (after orientation), Orientation, Duration (video), Codec (video), Frame rate (video) | EXIF PixelX/YDimension or the decoder | GStreamer / AVAsset |
| File | File name, size (with sidecars: "+ IMG_1234.MOV, 2.1 MB"), format (HEIC, JPEG, …), folder (click to reveal in Files/Finder) | filesystem + core item | same |
| Location | GPS latitude/longitude (+ altitude), "Open in Maps" link | EXIF GPS IFD | QuickTime ISO 6709 (`©xyz`, `com.apple.quicktime.location.ISO6709`) |
| Unswiper | Decision (Keep/Reject/Undecided, "suggested as blurry"), **blur score** (sharpness, motion, confidence) and suggestion reason (soft / shake / burst / not checked: video, screenshot, too dark) | quality-cache.json + library.json | "not checked (video)" |

Missing values are left out (no row), except in the Settings group, where "—" makes it clear that the camera didn't record the value. Values are formatted for people, e.g. "f/1.8", "1/120 s", "ISO 64", "26 mm (26 mm equiv.)", "+0.7 EV", "12.2 MP · 4032 × 3024".

**GPS privacy note:** coordinates are personal. They are read and shown only on this computer, never uploaded, logged, or written into library.json or the cache. Unswiper doesn't change the photo's own metadata. "Open in Maps" is the only action that sends anything out, and only when the user clicks it (it opens the default maps app or OpenStreetMap in the browser). There is a setting **Show Location in Photo Info** (proposed default: on, see §17.7). The Help page says where the location comes from and how to strip it (outside Unswiper).

### 17.2 Access (same on both platforms)
- **Right-click → Show Info** on a tile opens a **popover** next to it: a compact view of Camera, Settings, When, File and Unswiper.
- **Inspector panel** (toggle): **I** in a visit / the viewer, **⌘I** on Mac and **Ctrl+I** on Linux. It's also in the menus: Mac **View → Show Inspector**, Linux main menu ☰ → **Show Photo Info**. It shows the full list for the **selected** item (or the item open in the viewer), and with several selected it shows the first one plus "N selected". It follows selection changes and Left/Right in the viewer.
- **Compare (Mac)**: one info line per photo under each picture (focal length, f/, shutter, ISO, blur score), because that is where choosing between shots happens. On Linux, once Compare exists.
- The panel's open/closed state is remembered per session (proposed). Keys don't clash: K/R/U decide, I shows info, Space opens the viewer.

### 17.3 Linux (GTK 4 / libadwaita)
- Popover: a plain `gtk::Popover` with an `adw::PreferencesGroup`-style list of `adw::ActionRow`s (title = field, subtitle = value, selectable text).
- Panel: an `adw::OverlaySplitView` (sidebar on the right, collapses on narrow windows) wrapping the grid/viewer `content` stack, with an `adw::PreferencesPage` of groups. Toggled by `win.info` (stateful action, I/Ctrl+I), works the same way in the viewer.
- Location row: an "Open in Maps" button using `gtk::UriLauncher` with `geo:lat,lon` (falls back to an OpenStreetMap https URL).
- Reading happens off the main thread (like thumbnails); the panel shows "Reading…" and updates when done. Results are cached per path+mtime in memory only.

### 17.4 Mac (SwiftUI)
- Panel: `.inspector(isPresented:)` (macOS 14) on the visit grid and the viewer, toggled with ⌘I/I, with `Form`/`LabeledContent` sections. Popover: `.popover` from the context menu's **Show Info**.
- **Data source (choose in M1 of §17):** (a) Swift ImageIO `CGImageSourceCopyPropertiesAtIndex` + AVAsset metadata: native, handles HEIC/ProRAW for free, no helper call; or (b) a new helper command `unswiper-blur info <file> --json` (rename the helper to `unswiper-helper` at that point) for one formatter shared with Linux. **Recommendation:** (b) for the field list, wording and formatting, so both apps match and the tests are shared. The helper decodes HEIC metadata through the EXIF reader (HEIC carries an ordinary Exif box; `kamadak-exif` reads it from the ISOBMFF container) with `sips -g all` only as a fallback. Keep (a) as the fallback if the helper call is too slow for scrolling (target: < 50 ms per photo, cached).

### 17.5 Data: Rust core
- **Extend `unswiper_core::meta::exif`** (today it only reads dates) into `meta::info::photo_info(path) -> PhotoInfo { make, model, lens, focal_mm, focal_35mm, f_number, exposure_s, iso, flash, exposure_bias_ev, taken_at, offset, width, height, orientation, gps: Option<Gps> }` using the existing `kamadak-exif` dependency (MIT/Apache). It already parses JPEG, TIFF, HEIF/HEIC (ISOBMFF) and PNG `eXIf`, so no libheif call is needed for metadata. Use libheif (Linux) / sips (Mac helper) only for the dimensions of HEIC files without the EXIF size tags.
- **Videos:** extend `meta::quicktime` (already reads the creation date) for duration, dimensions, codec FourCC, make/model and ISO 6709 location from the `moov/udta`/`meta` keys, in pure Rust and testable. GTK can fill gaps from **GStreamer Discoverer** (`gst_pbutils::Discoverer`: duration, codecs, frame rate, tags) and Mac from **AVAsset** (`load(.duration)`, `.metadata`). The pure-Rust reader stays the source of truth for tests.
- `meta::info::format(&PhotoInfo) -> Vec<(Group, Label, Value)>`: the shared, human-readable formatter (rounding rules, "1/120 s", "f/1.8", "+0.7 EV", GPS to 5 decimals). Linux uses it directly; Mac gets it through the helper's JSON (`[{group, label, value}]` plus raw numbers for Compare).
- **Blur:** the panel reads the score and reason from `quality-cache.json` (`unswiper_blur::cache`) and the suggestion state from library.json. New small API: `explain(score, thresholds) -> Reason/“ok” text` (for example "Sharpness 6.5 (blurry below 13 at Normal)").
- Nothing new is written to library.json. GPS is never stored (§17.1). Geo grouping (§13.1) will reuse `Gps` from the same reader.

### 17.6 Tests
- **Rust unit tests** for each field on small generated fixtures: build JPEGs with known EXIF via a tiny EXIF writer in test code (as the date tests do), a HEIC fixture from libheif in CI, and a QuickTime fixture with ISO 6709. Cover missing tags, odd rationals (0/0, 1/0), Flash bit fields, negative exposure bias, southern/western GPS, the OffsetTime* variants, and the orientation swapping width/height.
- **Formatter golden tests:** one table `fields → expected strings`, shared by Rust and Swift (the Mac test decodes the helper JSON and compares it with the same table).
- **Property test:** the formatter never panics and never prints "NaN"/"inf" for arbitrary rationals.
- **Privacy test:** after showing info for a GPS photo and saving, library.json and quality-cache.json contain no coordinates.
- **GTK:** state tests for the panel model (follows selection and the viewer, multiple selection text) and an Xvfb smoke `--present-info` that prints the formatted rows for the first photo. **Mac:** XCTest for the inspector model plus the helper `info` call (skipped without the helper, like today's blur integration test).
- Manual checkout: an iPhone HEIC, a Live Photo, an Android JPEG, a DSLR JPEG, a screenshot (no EXIF), an MP4/MOV with location.

### 17.7 Open questions for Rob
1. **Default view:** popover only, panel only, or both (proposed: both; the panel starts closed, I/⌘I opens it)?
2. **Which fields** in the short popover (proposed: camera, lens, focal/f/shutter/ISO, date with zone, size, blur score)? Is anything in §17.1 unwanted?
3. **GPS:** show coordinates at all? If so, on by default with the setting to hide them (proposed), or off by default? Is "Open in Maps" OK given that it opens a web/maps app?
4. Show the raw blur numbers (sharpness/motion/confidence) or just the reason in words (proposed: the words, with the numbers as smaller text)?
5. Should the Compare view (Mac) get the one-line settings strip now, or wait for the panel?
6. Mac data source: shared Rust formatter through the helper (recommended) or native ImageIO?

### 17.8 Suggested release slot
**v0.4, as the first item, before geo grouping.** The GPS reader (EXIF GPS IFD + QuickTime ISO 6709) and its tests are exactly what geo grouping needs (§13.1), so building info first gives geo a tested foundation and lets Rob see whether his photos actually carry location before it's used for grouping. Rough effort:
- Rust reader + formatter + tests: about 2 days.
- GTK popover/panel: 1.5 days.
- Mac inspector/popover + helper `info`: 1.5 days.
- Docs/Help: 0.5 day.

New order: v0.3 (blur + video) → **v0.4: photo info (§17) → geo grouping** → Google Photos.

### 17.9 Approved defaults and M1 (Rob, 15 Oct 2026; v0.4)
- **Approved:** both the right-click **Show Info** popover and the inspector (I, ⌘I / Ctrl+I); fields camera and lens, exposure (shutter, aperture, ISO, focal length), date, dimensions/size and file info; **GPS shown locally, hideable, default on, never sent anywhere**; blur shown as words **Sharp / Soft / Blurry** (no numbers); Compare shows info for the selected item only; the Mac uses the shared Rust reader through the bundled helper.
- **Milestones:** M1 Rust reader + formatter → M2 GTK popover + inspector → M3 Mac popover + inspector + helper `info`, CHANGELOG and Help.
- **M1:** `unswiper_core::meta::info` reads `MediaInfo` with `kamadak-exif` (JPEG, TIFF/DNG, HEIC, PNG, WebP; JPEG SOF / PNG IHDR for the size when EXIF has none) and, for videos, `meta::quicktime::video_facts` (duration from `mvhd`, display size from the video `tkhd` incl. 90° rotation, codec FourCC from `stsd`, Apple make/model/creation date/ISO 6709 location, `udta/©xyz` fallback). `info::format(info, file, options) -> Vec<Row{group, label, value, short}>` is the one formatter; `short` marks the popover rows. Rules: the Exposure group shows "—" for values a camera didn't record, but is left out for files with no camera data (screenshots) and for videos; the lens maker is added only for third-party lenses; exposure compensation beyond ±20 EV and x/0 rationals count as missing. `unswiper_blur::decide::sharpness_word` gives the words: Blurry when suggested or below the floor, Soft below 2× the floor, otherwise Sharp; low-confidence photos are "Not checked" (videos always are). Golden table `crates/unswiper-core/testdata/info-format-golden.json` (5 cases), to be reused by the Mac tests.

### 17.10 M2: Linux (built 15 Oct 2026)
- **Right-click** a tile → a popover with the `short` rows (camera, lens, shutter, aperture, ISO, focal length, date, dimensions, size, format, sharpness) and **Show All Info (I)**. The EXIF/moov read for the popover is synchronous (one small read).
- **Inspector**: an `adw::OverlaySplitView` sidebar on the right of the grid/viewer stack, toggled by **I** / **Ctrl+I**, the stateful `win.info` action and ☰ → **Show Photo Info**. Closed at start; open/closed is per session (`AppState::info_open`). It shows `AppState::info_target()`: the viewer item, else the first selected item in grid order with "(and N more selected)". Groups are `adw::PreferencesGroup`s of `ActionRow`s with selectable values. Reading happens on a thread ("Reading…"), cached in memory per path and size+mtime (`info::InfoCache`). Below 1100 sp wide the sidebar floats over the grid (breakpoint).
- **Location**: shown by default; ☰ → **Show Location in Photo Info** turns it off (config key `infoShowLocation: false`, absent = on). The map button opens OpenStreetMap through `gtk::UriLauncher`, only on click. Coordinates are never written (smoke test checks library.json).
- **Compare strip**: Linux has no Compare view yet, so the strip is one caption line under the grid with `info::strip()` for the **selected item only** (e.g. `3.8 mm · f/2 · 1/45 s · ISO 342 · Sharp`); hidden in the viewer and with nothing selected.
- **Sharpness**: from `quality-cache.json` (reloaded after each scan) via `sharpness_word`; a blur suggestion in library.json wins. Off when blur detection is off; "Not checked" for screenshots, unscored photos and videos.
- **Tests**: `info.rs` unit tests (file facts incl. sidecars, words from the cache, grouping, cache invalidation), `AppState` tests (target follows viewer/selection/filter; location setting round-trip), and an Xvfb smoke `--present-info` that prints the rows, opens the panel and the popover, checks that library.json holds no coordinates, and that the setting hides the Location rows.

### 17.11 M3: Mac and the helper `info` command (built 15 Oct 2026)
- **Helper:** `unswiper-blur info [--library library.json] [--sensitivity S] [--blur-off] [--hide-location] FILE...` prints `[{path, rows, strip}]` with the shared formatter; with `--library` it uses the index (Live Photo sidecar sizes) and the blur state and scores next to library.json. `info --format CASES.json` formats the golden table so the Mac tests can compare the helper with Rust. `sharpness_for` / `file_facts` moved into `unswiper_blur::info` and the Linux app now calls them, so both apps share one rule. Rob's three M2 choices (lens maker only for third-party lenses; no Exposure group for screenshots/videos; Soft below 2× the floor) are approved as they are. The strip leaves out "Not checked".
- **Mac UI:** `.inspector` on the visit grid and the viewer (I in the grid and viewer, ⌘I and **View ▸ Show Inspector**, **Info** button), right-click ▸ **Show Info** popover with the short rows and **Show All Info**, the settings strip in the bottom bar, and in Compare under the focused picture only (click a picture to focus it). **View ▸ Show Location in Photo Info** uses `AppConfig.infoShowLocation` (same library.json key as Linux; nil = on). Info is read on demand in the background, cached in memory per settings, and dropped on re-index and after blur checks.
- **Tests (Mac):** `InfoTests.swift` — target/heading/sections/compare focus/maps URL rules, the golden table decodes and groups, helper arguments and parsing, the location setting round-trip in library.json, caching per settings and no coordinates in library.json (fake runner), and two helper integration tests (golden table identical to Rust; camera, aperture, ISO, size and GPS from an ImageIO-written JPEG, location hidden with the setting), skipped without `UNSWIPER_BLUR_HELPER` like the blur integration test.

## 18. Content hashes for change and bit-rot detection (design approved 15 Oct 2026; unscheduled, after v0.4)

Rob asked (15 Oct 2026) whether Unswiper should keep a **per-file content hash** of every photo and video, to notice when a file changed (a delta), got corrupted, or silently rotted on disk. Nothing here is scheduled; this section only explores the options.

### 18.1 What a hash would give us
- **Bit rot / corruption:** a file whose size and mtime are unchanged but whose hash changed was damaged (disk, copy, sync tool). Today nothing notices this.
- **Deltas:** "what changed since last time" across re-indexes, independent of mtimes (which copies and sync tools rewrite).
- **Identity across moves/renames:** the same bytes under a new path are the same photo, so decisions, blur scores and (later) geo data could follow the file.
- **Duplicates:** identical files in two folders (a by-product, not a goal; near-duplicates need perceptual hashes, a different feature).

### 18.2 Algorithm
- **BLAKE3** (CC0/Apache-2.0, pure Rust crate `blake3`, SIMD, multi-threaded for large files): about 1–3 GB/s per core, so a 50 GB library is minutes of disk-bound reading, not CPU-bound. **SHA-256** is the conservative alternative (standard tools everywhere: `sha256sum`, `shasum -a 256`, `Get-FileHash`), at roughly a third of the speed without SHA extensions.
- Both are far beyond what bit-rot detection needs; the choice is about **interoperability vs speed**. Store the algorithm name with every hash (`"blake3:…"` / `"sha256:…"`) so it can change later.
- Hash the **whole file bytes** (metadata edits count as changes). A separate "pixel hash" ignoring metadata is out of scope.

### 18.3 Single source of truth (Rob's main concern)
| Option | Pros | Cons |
|---|---|---|
| **A. In library.json** (per item) | One file, already synced between the Mac and Linux apps | library.json is per library *view*, rewritten often, grows with every item; a damaged library.json loses the hashes too; it is keyed by path, so moves look like delete + add |
| **B. Sidecar per photo** (`IMG_1.JPG.b3`) or per folder (`.unswiper-hashes`) | Travels with the files across machines, disks and backups; any tool can verify | Writes into Rob's photo folders (Unswiper avoids that today); clutters; other apps and sync tools may drop or duplicate it |
| **C. Separate hash DB** next to library.json (`hashes.sqlite` or `hashes.json` in the support folder) | Not in the photo folders; can be large; can index by hash *and* by path; separate from decisions, so either can be rebuilt | Another file to back up; per machine unless synced; needs a format and a migration story |

**Leaning:** **C**, a separate store in the support folder, keyed by content hash with the last known path(s), size and mtime, and **library.json referencing only the hash** (one short field per item) once it exists. The hash store is the single source of truth for hashes; library.json stays the source of truth for decisions. A per-folder **export** (`SHA256SUMS`/`B3SUMS` in the standard `sha256sum -c` format) can be written **on request** for archiving, not kept in sync automatically.

### 18.4 Moves, renames and edits
- Re-index already sees adds/removes. With hashes, a removed path + an added path with the **same hash and size** is a **move/rename**: carry the decision, blur score and info over (as `quality::carry_over` does for blur today).
- Same path, same size+mtime, different hash → **possible corruption**: flag it, never "fix" it. Same path, new mtime, new hash → an **edit** (normal).
- Copies (same hash, two paths) are reported as duplicates only on request.
- Cost: hashing only happens for **new or changed (size/mtime) files**; a full **verify** pass (re-read everything and compare) is a separate, explicit action ("Verify Library…", resumable, low priority, with progress), never automatic on launch.

### 18.5 Multiple machines and platforms
- Paths differ between the Mac, Linux and Windows (`/Users/rob/Pictures` vs `/home/rob/Pictures` vs `D:\Photos`), so the store must key by **hash + path relative to the library root**, not absolute paths. That also lets two machines compare notes.
- Sync tools (iCloud, Syncthing, Dropbox) preserve bytes but not always mtimes; hashes make "unchanged" detection robust there.
- Line endings / case-insensitive file systems: hash bytes only; compare paths with the platform's case rules (APFS and NTFS are case-insensitive by default).
- Windows needs no special handling for the hash itself; the store and paths use `/` separators internally (as library.json does).

### 18.6 Companion app or CLI?
- Hashing and verifying a whole library is **batch work** that fits a CLI better than the review UI: `unswiper-hash scan|verify|export|diff <library>` (Rust, shares the store format), runnable from cron/launchd/Task Scheduler on a NAS or server without a display. The app would only **read** the store (show a "changed/damaged" badge, carry decisions across moves) and offer "Verify Library…" that runs the same code.
- A separate companion GUI is not worth it; mature tools already exist for pure integrity checking (e.g. `rhash`, `hashdeep`, `par2`, `snapraid`, ZFS/Btrfs scrubbing). Unswiper's value is tying hashes to **its** decisions and visits, not replacing those tools.

### 18.7 Open questions
1. BLAKE3 (fast, needs our tool to verify) or SHA-256 (slower, verifiable with built-in OS tools)?
2. Is a support-folder store acceptable, or does Rob want hashes to travel **with** the photos (sidecars / per-folder manifest)?
3. Should a detected move carry decisions automatically, or ask?
4. Should verify ever run on its own (e.g. weekly), or only on request?

### 18.8 Recommendation
Don't build it into v0.4. If wanted later: a **separate hash store in the support folder** (keyed by BLAKE3 + root-relative path, with size/mtime, algorithm tagged), filled incrementally for new/changed files, plus a small **`unswiper-hash` CLI** for scan/verify/export (`B3SUMS`/`SHA256SUMS` on request). The app uses it read-only to carry decisions across moves/renames and to flag possible corruption; library.json gets at most a hash reference per item. Rough effort: CLI + store 2–3 days, app integration (move carry-over, badge, "Verify Library…") 2 days.

### 18.9 Decision (Rob, 15 Oct 2026)
- **Approved design:** a separate **`unswiper-hash`** CLI with **its own hash store**, so hashing stays a separate concern from reviewing: library.json keeps decisions, the hash store keeps hashes, and the review apps never write hashes. The app may later read the store (move/rename carry-over, a "possibly damaged" badge), as in §18.8.
- **Schedule:** not scheduled; after v0.4. The open questions in §18.7 (BLAKE3 vs SHA-256, support-folder store vs hashes travelling with the photos, automatic vs asked carry-over on moves, verify on request vs scheduled) are decided when it is scheduled.

## 19. Google Photos source: Google Takeout import (plan 15 Oct 2026; v0.5; M1 and M2 built)

Rob's order (15 Oct 2026): **Google Photos next**, then geo grouping, then Windows.

**Rob's decisions (15 Oct 2026):**
- **Takeout only.** The Picker API, OAuth and Google verification are **rejected** (reasons in §19.2).
- The app guides the user through the Takeout setup (§19.4), then **unpacks the archives** (`.zip` or `.tgz`, any number of parts) **into a folder the user chooses** and indexes it. After that, Unswiper works **only on the unpacked folder, like any local library**; the archives are no longer needed.
- Rejects can't be deleted in Google Photos, so Unswiper gives a **"Delete these in Google Photos" list plus a CSV** (§19.7).
- Scale to plan for: Rob has **about 120 GB in Google Photos and about 350 GB local**. Priority: **maximum automation** and **clear time and space figures** before anything starts (§19.6).

Facts are from Google's official docs, checked 15 Oct 2026 (sources in §19.12).

### 19.1 What the Google Photos APIs allow in 2026 (verified)
- **Library API: app-created media only.** On 31 March 2025 Google removed the `photoslibrary.readonly`, `photoslibrary.sharing` and `photoslibrary` scopes. Listing, searching and getting media items and albums now only works for items **the app itself uploaded**. Calls that relied on the removed scopes return `403 PERMISSION_DENIED`. Shared-album methods are gone.
- **Picker API: the user picks, the app gets a copy.** The app creates a session and gets a `pickerUri`. The user opens it in their browser (signed in to Google), picks up to **2000 items per session**, and the app polls until `mediaItemsSet` is true, then lists the picked items. Scope: `photospicker.mediaitems.readonly`. Each item's `baseUrl` lasts **60 minutes** and needs the OAuth bearer token.
  - Photos: `=d` downloads the image **with EXIF but without location** (Google strips GPS). `=wW-hH` gives a scaled copy.
  - Videos: `=dv` gives a **transcoded** "high quality" copy, not the original file. Motion photos: image via `=d`, the video part via `=dv`.
  - Sessions expire (`expireTime`); the app should delete them when done.
- **No delete, no trash.** Neither API has a delete or trash method for media items. The Library API's remaining methods are `batchCreate`, `batchGet`, `get`, `list`, `patch` and `search`, all limited to app-created items. **An app can't remove photos from a user's Google Photos library, and since 2025 it also can't add the user's own (non-app-created) photos to an album.** So the old "export rejects to an album" plan (§13) is not possible either.
- **No whole-library export API.** The Data Portability API has no Google Photos resource; its only photo scope is for photos posted on Google Maps. **Google Takeout** can export all of Google Photos (originals plus JSON metadata files), with optional scheduled exports every 2 months for a year (Google Photos must be the only product selected). Archives expire after about 7 days. There is no API to start or fetch a Takeout export.
- **No local sync folder from Google.** Google Drive for desktop doesn't sync Google Photos. The Google Photos desktop uploader only uploads. Third-party sync tools such as rclone's Google Photos backend use the same API and have the same 2025 limits.

### 19.2 Rejected: Picker API, OAuth and Google verification
Researched in the first draft of this section (commit `1957c4d`) and rejected by Rob on 15 Oct 2026, because:
- **Location is stripped:** Picker downloads (`=d`) remove GPS, which breaks Photo Info location and the planned geo grouping.
- **Videos are transcoded:** `=dv` gives a "high quality" re-encode, not the original file.
- **It still can't delete:** no Google Photos API can delete, trash or add the user's own photos to an album, so the Picker brings no commit advantage over Takeout.
- **Verification is heavy:** a domain we own, a homepage and privacy policy on it, a scope justification, a demo video, and weeks (or months) of Google review, with an "unverified app" user cap until then.
- **Testing mode is not a product:** only listed test users, and refresh tokens expire after **7 days** (sign in again every week).
- Also: 2000 items per pick, 60-minute download URLs, token storage per platform, and a Google Cloud project to maintain, all for a worse copy than Takeout gives.

Consequences: no Google sign-in, no tokens, no network code and no Google Cloud project. The import is **fully offline** after the download. Option C of the first draft (a folder the user keeps in sync) stays a documentation note: if the photos already live locally, review that folder as usual.

### 19.3 What Takeout gives
- **Originals** (photos and videos as uploaded), with EXIF intact, plus a JSON sidecar per item:
  - recent exports: `<file>.supplemental-metadata.json`; older: `<file>.json`;
  - long names are cut at about 46–51 characters (`IMG_…jpg.supplemen.json`, or inside the media name);
  - duplicates of a name: `IMG(1).jpg` ↔ `IMG.jpg.supplemental-metadata(1).json`;
  - edited copies (`-edited`, localized variants) have no JSON of their own and use the original's;
  - the video of a Live Photo/motion photo uses the still's JSON;
  - fields used: `photoTakenTime.timestamp` (string, seconds), `geoData` then `geoDataExif` (0/0 means none), `title`, `description`;
  - album folders have a `metadata.json` (album title), not a sidecar.
- Layout: `Takeout/Google Photos/<folder>/…`, where `<folder>` is a year folder (`Photos from 2019`) or an album. The `Google Photos` folder name is localized (`Google Fotos`, `Google Photos`…). The same photo can appear in a year folder and in album folders (byte-identical copies).
- Parts: with a 50 GB size limit, 120 GB arrives as about 3 independent archives (`takeout-<date>-001.zip`, `-002.zip`, …). Each part is a complete archive; a sidecar can be in a different part from its photo.
- Archives expire after **about 7 days** and each can be downloaded **at most 5 times**.
- **Scheduled exports:** "every 2 months for 1 year"; for Google Photos, Google says scheduled exports cover "new and updated data", and Google Photos must be the only product selected. Whether a later export contains only new items or everything again is not documented clearly, so the import handles both (§19.5, incremental re-import).

### 19.4 Guided setup: "Import from Google Photos" (M2, built in both apps)
A step-by-step assistant in both apps (GTK and Mac), written for beginners. Each step has a short explanation, a **Done, next** button and **Back**. Wording below is the help text; the button and option names were checked against Google's help page on 15 Oct 2026 (§19.12). Google's own screen wording can change; the assistant says "the names may differ slightly".

**Step 1. Open Google Takeout.**
Click **Open Google Takeout**. Your web browser opens takeout.google.com. If Google asks, sign in to the Google account that has your photos.
*(Implementation: open `https://takeout.google.com/settings/takeout/custom/photos` (no parameters); `photos` is the product's `data-id`. If the preselection ever fails, the steps below still work by hand; the assistant also has an **Open takeout.google.com** fallback button. M2 check (15 Oct 2026): signed out, the link redirects to Google sign-in with the full `custom/photos?frequency=2_months` continue URL kept; Google Photos' own Settings ▸ Export ▸ Backup link points to `takeout.google.com/settings/takeout/custom/photos`, which confirms the `photos` id. **Rob's check, signed in (19 Oct 2026):** `custom/photos?frequency=2_months` preselects **only Google Photos** (good) but does **not** preselect the 2-month frequency, although Google's help lists the parameter. So the link drops `?frequency=2_months`, and step 3 tells the user to pick **Export every 2 months for 1 year** by hand.)*

**Step 2. Choose only Google Photos.**
1. At the top of the list, under **Select data to include**, click **Deselect all**.
2. Scroll down to **Google Photos** and tick its box.
3. Leave **All photo albums included** as it is.
4. Scroll to the bottom and click **Next step**.

**Step 3. Choose how Google sends it.**
1. **Destination** (Google calls it "Delivery method"): choose **Send download link via email**.
2. **Frequency**: Google starts with **Export once**; change it by hand to **Export every 2 months for 1 year** (the link can't preselect it, Rob checked 19 Oct 2026). (This only works when Google Photos is the only product ticked. Unswiper imports only what is new each time.)
3. **File type**: choose **.zip**.
4. **File size**: choose **50 GB** (fewer, bigger parts).
5. Click **Create export**.

**Step 4. Wait for Google's email.**
Google prepares the export. This can take from a few minutes to a few days; most people get it the same day. You'll receive an email titled something like "Your Google data is ready to download". You can close Unswiper meanwhile; the assistant remembers where you were.

**Step 5. Download all parts into one folder.**
1. In the email, click **Download your files**.
2. Google shows one **Download** button per part (for 120 GB with 50 GB parts, about 3). Click each one. Google may ask for your password again.
3. Save all parts in **one folder**, for example `Downloads` or a folder like `GooglePhotosTakeout` on a disk with enough space.
4. Download them within 7 days: links expire, and each part can be downloaded only 5 times.

**Step 6. Choose the folders and check the space.**
1. **Archives folder:** click **Choose…** and pick the folder with the downloaded parts. Unswiper finds every `.zip` and `.tgz` in it, including all parts.
2. **Library folder:** click **Choose…** and pick (or create) the folder where your photos should go, for example `GooglePhotosLibrary`. This becomes a normal Unswiper library.
3. Unswiper shows the dry run, without changing anything: number of archives, total size, number of photos and videos, how much is new, the space needed (keeping or deleting the archives), the free space, and an estimated time.
4. If the space is too small, a red note says how much is missing and suggests ticking **Delete each archive after unpacking** or choosing another disk. **Unpack** stays disabled until it fits.

**Step 7. Unpack.**
1. Optional: tick **Delete each archive after unpacking**. Each part is deleted only after it unpacked completely and without conflicts.
2. Click **Unpack**. A progress bar shows the part being unpacked, files and GB done, speed and time left.
3. You can stop and continue later: finished files and parts are skipped.

**Step 8. Index.**
Unswiper opens the library folder and indexes it like any local folder, using Google's JSON files for dates and places when the photo itself has none. From here on everything works as usual (visits, Keep/Reject, commit). The archives are not used again.

**Next time (every 2 months):** Google emails again. Download the new parts into the archives folder and click **Import from Google Photos** again; after an import the assistant reopens at step 6 with the same folders. The same library folder is used, and only items not imported before are added (photos you already rejected and deleted are not brought back).

### 19.5 How the import works (M1, built)
New crate `crates/unswiper-takeout` (Rust, shared by both apps through the CLI/helper):
- **Folder of archives:** `find_archives(folder)` takes every `.zip`/`.tgz`/`.tar.gz` in the folder (all parts of all exports); `find_parts(archive)` adds the sibling parts of one archive.
- **Safe unpacking:** only `Takeout/Google Photos/<folder>/…` is unpacked, to `<library>/<folder>/…`; other products are listed and skipped. Absolute paths, `..`, drive letters, NUL bytes, symlinks and hard links are refused and listed. Folders are created one level at a time and never through an existing symlink. **Nothing is written outside the library folder.**
- **Names:** Google's names are kept exactly (renaming would break sidecar matching). Unswiper's own names follow Rob's convention: `unswiper-takeout-state.json`, `*.unswiper-part`.
- **Resumable:** each file is written to `<name>.unswiper-part`, size-checked, synced and renamed, so a finished file is always whole. Finished archives are recorded in the state file and skipped next time; an existing file with the same size counts as done. **A different existing file with the same name is never overwritten** (conflict, listed; that archive is not deleted).
- **Free-space check:** before unpacking, the dry run computes the space needed and compares it with the free space on the library disk (`statvfs`), keeping a 1 GB reserve; it refuses to start if it doesn't fit, and also if the free space **can't be measured** (mandatory since M2, §19.5b).
- **Delete after unpack (option):** each archive is deleted once it unpacked completely without conflicts (never an archive without Google Photos in it, never one with conflicts).
- **Dry run:** total archive size, media items and sidecars, uncompressed size, what is new, space needed with and without deleting archives, free space. Zip archives are measured exactly from their directory in seconds. A `.tgz` has no directory: by default its file size is used (photos barely compress, so this errs high); `--exact` reads it completely.
- **Incremental re-import:** every imported file is recorded (path and size) in the state file. A later export only adds new items: a file imported before is not written again even if it was deleted or moved since (**rejects stay gone**), and a photo with the same file name and size as one already imported in another folder is skipped as a duplicate (the album copy of a year-folder photo; option `--keep-duplicates` turns this off). A content hash for renamed copies is a later improvement (M4).
  - Note: Rob asked for matching by "size + date". At unpack time the date is in the sidecar, which may be in another part, so M1 matches by path + size and by file name + size; the hash in M4 covers the rest.
- **Progress:** archive n of N, files, bytes written and estimated total bytes (for a percentage and time left), reported every 25 files or 64 MB.
- **Sidecar matching and dates:** all the naming cases of §19.3, scored best match first; `TakeoutDates` wraps the normal date reader: EXIF/QuickTime date first, then `photoTakenTime`; location: EXIF GPS first, then `geoData`/`geoDataExif`. Sidecar dates are recorded as `Capture` (no new `DateSource`, so library.json and the Mac app are unchanged). JSON files are not media, so the indexer ignores them.
- **CLI (for testing, scripting and the Mac helper):**
  - `unswiper-cli takeout import <folder|archive>... --to <library> [--dry-run] [--exact] [--delete-archives] [--keep-duplicates] [--no-space-check]`
  - `unswiper-cli index <library> --takeout`
- Tests use synthetic archives built in the tests (no personal data): every sidecar naming case, zip-slip variants, symlinks, multi-part sets with a sidecar in another part, resume after interruption, conflicts, other products, dry-run figures, space refusal, archive deletion, and incremental re-import with a deleted reject.

### 19.5a M2 as built (15 Oct 2026)
- **One wording source:** the 8 steps live in `crates/unswiper-takeout/src/takeout-guide.json` (step id, title, action, Markdown body, Takeout link and fallback link). The Mac bundles a byte-identical copy (`apps/macos/Sources/Unswiper/Resources/Takeout/takeout-guide.json`); Rust and Swift tests both fail if they differ. The dry-run and result lines ("lines") are also made in Rust, so both apps show the same figures and words.
- **Linux (GTK):** Library menu and setup page: **Import from Google Photos…** opens an assistant window (step label, explanation, Back / Next). Step 1 has **Open Google Takeout** and **Open takeout.google.com**. Step 6 has **Choose…** for the archives and library folders, **Delete each archive after unpacking** and **Also unpack album copies** (off = duplicates skipped), the dry-run figures and **Check Again**; Next stays disabled until the folders are valid and the space fits. Step 7: **Unpack**, a progress bar with part, files, GB and time left, **Stop**, and **Continue Unpacking** after a stop or error. Step 8: **Open Library** indexes the folder with Google's dates (`dates_for` picks the Takeout fallback when the library has `unswiper-takeout-state.json`). The step, folders and options are kept in `takeout-setup.json` in the app's support folder.
- **Mac (SwiftUI):** the same assistant in its own window (`takeout`), opened from the Library menu, the setup screen and Help. It runs the bundled helper: `unswiper-blur takeout import <archives> --to <library> --json --dry-run` for the figures and `--progress-json` for the unpack (NDJSON, see below); **Stop** ends the helper and the next run resumes. Setup state is kept in UserDefaults (`takeoutSetup`). Indexing a Takeout library asks the helper for Google's dates (`unswiper-blur takeout dates <library>`, JSON array of `{path, taken, lat, lon}`); the Swift indexer uses them only when a file has no capture date.
- **Helper protocol (one JSON object per line on stdout):** `{"event":"plan","plan":{…},"fits":true,"needed":…,"lines":[…]}`, `{"event":"progress","archive":1,"archives":2,"files":…,"bytes":…,"bytes_total":…}`, `{"event":"done","report":{…},"lines":[…]}`, `{"event":"error","kind":"cancelled|not_enough_space|space_unknown|no_archives|no_photos|archive|write|unknown_format|…","message":…}`. Exit code 0 on success. Sample outputs are kept in `crates/unswiper-takeout/tests/fixtures/helper/` and checked by the Rust and Swift tests.
- **Stop and continue:** `unpack_cancellable` stops between files and saves the state; finished parts and files are skipped next time.
- **Options:** `--delete-archives`, `--keep-duplicates` (album copies; default off), `--no-space-check` (CLI only, the explicit opt-out of §19.5b).

### 19.5b Free-space check is mandatory (Rob, 19 Oct 2026)
Rob: "a full download can be huge". The import **never starts without a known free space** that fits. There is no "unknown, continue" any more.
- **Core:** `Plan::fits` is false when the free space is unknown; `unpack` refuses with `SpaceUnknown` (kind `space_unknown`) unless the space check is off. Only the CLI can switch it off, explicitly, with `--no-space-check`; neither app passes it. The dry-run text then says why and how to fix it, and the apps keep **Next** and **Unpack** disabled (Mac: the helper also refuses).
- **Measuring (`measure_free_space`):** `statvfs` on the library folder (or its nearest existing parent). Flatpak: a folder picked through the document portal (`/run/user/…/doc/<id>/…`) already reports the real disk. xdg-desktop-portal 1.14 and later pass `statvfs` through for document folders (checked in `document-portal-fuse.c` at tags 1.14.0, 1.16.0, 1.18.4 and main, 19 Oct 2026); only its virtual folders reply with empty sizes. If a mount reports no sizes (`f_blocks == 0`), Unswiper reads the host path from the portal's `user.document-portal.host-path` attribute (xdg-desktop-portal 1.19 and later; 1.18.4 on Ubuntu 24.04 doesn't have it). It measures there only if the sandbox can see that folder and a probe file proves it is the same folder; a path like `/media/…` can exist in the sandbox and be a different disk.
- **Permission:** none added. The manifest already has `--filesystem=xdg-pictures`, and the help suggests `Pictures/GooglePhotosLibrary`. With that grant the file chooser returns the real path for folders in Pictures, so `statvfs` measures the real disk directly.
- **When it still can't be measured:** the figures say "can't be measured, so Unswiper won't start" and give the fix for beginners. (1) Choose a library folder inside Pictures and click **Check Again**. (2) In Flatpak, run the exact `flatpak override --user --filesystem='<host path>' io.github.rranslam.Unswiper` line (with the host path when the portal names it, else a placeholder to replace), then reopen Unswiper. On the Mac, choose a folder on a disk of this Mac. The help pages (both apps) have a section "If the free space can't be measured"; the CLI adds the `--no-space-check` hint.
- **Tests:** measuring with stand-in system calls (normal disk, no statvfs, portal without host path, portal host path that is the same folder, a different folder, a folder not visible), the real xattr call, `fits` with unknown space, the beginner wording with and without Flatpak and quoting, the help pages, and on the Mac the model (Next and Unpack blocked) and the helper event with `free_bytes: null`.
- **Not tested here:** a real document-portal folder inside the Flatpak. The box has no Flatpak or portal. Rob should check it once (§19.11).

### 19.6 Time and space estimates
**Space** (library on the same disk as the downloaded archives; Takeout zips are about the size of the photos, since photos and videos don't compress):

| Rob's 120 GB | Peak disk use | After the import |
|---|---|---|
| Keep the archives | about 240 GB (120 archives + 120 library) | 240 GB until you delete the archives |
| **Delete each archive after unpacking** | **about 170 GB** (120 + one 50 GB part) | 120 GB |
| Archives on another disk (for example a USB disk) | 120 GB on the library disk | 120 GB |

- Album copies skipped as duplicates make the library smaller than the export.
- Plus Unswiper's 1 GB reserve. The dry run shows the exact figures before anything is written.
- The 350 GB local library is separate and unchanged; reviewing both needs no extra space (commit moves files, it doesn't copy).

**Time:**
- **Google prepares the export:** minutes to a few days (usually the same day). Nothing to do meanwhile.
- **Download, 120 GB:** about 2.7 h at 100 Mbit/s, 32 min at 500 Mbit/s, 16 min at 1 Gbit/s (Google's servers can be slower).
- **Dry run:** seconds for zip archives (only the directory is read); a `.tgz` with `--exact` takes about as long as unpacking it.
- **Unpack:** limited by the disk. Measured on the development box (SSD): 2 GB of synthetic Takeout (one zip, one tgz) in **4 s, about 500 MB/s**. Expect about **4–10 min for 120 GB on an internal SSD**, about **20–40 min** for an SSD to a USB 3 hard disk, and **40–70 min** within one hard disk (reading and writing on the same disk).
- **Index:** the 200 synthetic files indexed in under a second; indexing reads only headers, so time depends on the number of files, not their size. The usual index time for a local library applies (Rob's 350 GB library is indexed today already).
- **Re-import every 2 months:** only the new items are written; the dry run says how many.

### 19.7 Rejected photos: "Delete these in Google Photos" (M3)
No API can delete in Google Photos, so commit gets a **Google Photos rejects** step:
- Unswiper moves the local copies to Trash/quarantine as today (they're copies), clearly labelled.
- A list: thumbnail, file name, date taken, album/year folder, grouped by visit and day, so the user can open that day in Google Photos and multi-select the shots.
- **Export as CSV** (`unswiper-google-photos-rejects-<date>.csv`: file name, date taken, folder, size, latitude/longitude if known) for the user's records.
- Wording: **"Unswiper can't delete photos from Google Photos. Here are the N photos you rejected. Delete them in Google Photos."**
- Rejects stay gone on the next import (§19.5).

### 19.8 Privacy
- No Google sign-in, no tokens, no network access: Takeout import is fully offline.
- Location from Google's JSON is shown under the existing Photo Info location setting.
- The state file holds only relative paths, sizes and archive names, in the library folder.

### 19.9 Mac and Linux parity
- The Rust crate does all the work. Linux calls it directly; the Mac app runs the bundled helper (as with blur/info) with JSON progress (`takeout import --json --progress-json`, built in M2, §19.5a). Mac helper builds keep using `--no-default-features`.
- The guided setup (§19.4) and the rejects list exist in both apps with the same wording.

### 19.10 Milestones

| Milestone | Content | Effort |
|---|---|---|
| **M1** Takeout core (**built**, branch `v0.5`) | `unswiper-takeout` crate: safe resumable multi-part unpack, folder of archives, free-space check, delete after unpack, dry run, incremental re-import, progress, sidecar matching, date/geo fallback into the index, CLI, tests | done |
| **M2** Guided setup, both apps (**built**, branch `v0.5`) | The 8-step assistant (§19.4), dry run and space screen, progress with time left, cancel/resume, JSON progress for the Mac helper, help page "Import from Google Photos" in both apps (§19.5a). Not done: a "Google Photos (Takeout)" source label in the library header (moved to M3) | done |
| **M3** Rejects list | "Delete these in Google Photos" screen, CSV export, wording in Review & Commit | 2 days |
| **M4** Later | Content hash for renamed duplicates; optional Takeout description/title in Photo Info; reminder when the next scheduled export is due | 1–2 days |

Rejected: M4–M6 of the first draft (OAuth + Picker, verification, Picker release), see §19.2.

### 19.11 Open points
- Done (19 Oct 2026, Rob, signed in): `custom/photos` preselects only Google Photos; `frequency=2_months` is ignored, so it was dropped and the frequency is chosen by hand (§19.4). The plain takeout.google.com fallback stays.
- Flatpak (§19.5b): check once in the Flatpak that a library folder outside Pictures, picked through the chooser (for example on a USB disk), shows real free space; and that a folder where it can't be measured blocks with the override hint. Note: for a portal folder the archives count as "another disk" (different device), so deleting archives doesn't lower the space needed there; that is cautious but can overstate the needed space.
- Google's JSON files stay in the library and are not moved with rejects on commit yet (M3).
- Photo Info does not show the Takeout location yet (M4).
- Check with Rob's first real export whether later scheduled exports are full or only new items (the import handles both).
- Album copies are skipped as duplicates by default, so an album folder may hold only its `metadata.json` and the photos that are only in that album. If Rob wants albums complete, switch the default (costs space).

### 19.12 Sources (checked 15 Oct 2026)
- Updates to the Google Photos APIs (March 2025 changes, removed scopes, methods): https://developers.google.com/photos/support/updates
- Authorization scopes (Picker scope, remaining Library scopes, verification note): https://developers.google.com/photos/overview/authorization
- Picker API get started (session, `pickerUri`, polling): https://developers.google.com/photos/picker/guides/get-started-picker
- Picker sessions reference (2000-item limit, `expireTime`): https://developers.google.com/photos/picker/reference/rest/v1/sessions
- Picker media items and base URLs (60-minute URLs, `=d` without location, `=dv` transcoded, bearer token): https://developers.google.com/photos/picker/guides/media-items
- Library API mediaItems methods (no delete): https://developers.google.com/photos/library/reference/rest/v1/mediaItems
- OAuth 2.0 for desktop apps (PKCE, loopback, no secrets, OOB removed, revoke): https://developers.google.com/identity/protocols/oauth2/native-app
- OAuth 2.0 overview (7-day refresh tokens in Testing): https://developers.google.com/identity/protocols/oauth2
- Verification requirements (domain, privacy policy, video, CASA only for restricted): https://support.google.com/cloud/answer/13464321
- Unverified apps (user cap): https://support.google.com/cloud/answer/7454865
- Data Portability API scopes (no Google Photos resource): https://developers.google.com/data-portability/user-guide/scopes
- Google Takeout help (Deselect all/Next step/Create export, delivery by email link, scheduled Photos exports every 2 months for one year with Photos as the only product, 50 GB size limit, archives expire in about 7 days, 5 downloads per archive, `custom/<products>?frequency=2_months` URL parameters; the frequency parameter is not honoured, Rob 19 Oct 2026): https://support.google.com/accounts/answer/3024190

## 20. Geo grouping (design 25 Oct 2026; released in v0.6.0 on 26 Oct 2026; see §20.12–20.15)

Rob's order: v0.5 Google Photos (Takeout) → **v0.6 geo grouping** → Windows. Approved scope: group photos by **place**, using the GPS location stored in each file, with Takeout JSON `geoData` as an optional source. Items without a location stay in their **date-based set**, the same way undated items are handled today (§group.rs: they join the visit of their dated folder siblings). Place names come from an **offline, standalone open-source reverse geocoder with bundled data**; no outside API, no network (Rob's tools rule). App ID stays `io.github.rranslam.Unswiper`.

### 20.1 Sources of location (in order)
1. **The file's own metadata**: EXIF `GPSLatitude/GPSLongitude(+Ref)` for photos (already read by `unswiper-core::meta::info::gps`, §17); QuickTime `©xyz` / `com.apple.quicktime.location.ISO6709` and the MP4 `udta` location box for videos.
2. **Takeout sidecar** `geoData` (then `geoDataExif`), already parsed by `unswiper-takeout::dates` (§19). Used only when the file has none. `0.0, 0.0` means "no location" in Google's JSON and is ignored.
3. Nothing else: no guessing from folder names, no Wi-Fi/IP lookup.

Sample set (Rob's calibration photos, 25 Oct 2026, counts only): 91 files; **78 carry GPS** (76 of 85 JPG, 2 of 2 MP4); 0 of 2 3GP, 0 of 1 AVI, 0 of 1 MOV. So about 86 % have a location, and the "no location" path must work well.

### 20.2 Clustering: three grouping modes (Rob, 25 Oct 2026)
The grouping mode is chosen per library (`AppConfig.groupMode`) and per run on the CLI (`--group-mode`, §20.7):

| Mode | Value | What it does |
|---|---|---|
| **Visit only** (default) | `visit` | Time-gap visits exactly as in v0.5. Place names may still appear as a title hint, but grouping is unchanged. |
| **Geo only** | `geo` | True place clustering, regardless of time gaps (§20.2.2). |
| **Visit + geo** | `visit-geo` | Each time visit is split by place (§20.2.3). |

The default is `visit`, so existing libraries and users see no change until they pick another mode.

#### 20.2.1 Visit only (`visit`, default)
`group::cluster` with `visit_gap_hours`, undated items joined to their dated folder siblings: unchanged code path.

#### 20.2.2 Geo only (`geo`)
- **Algorithm: DBSCAN on haversine distance**, `eps = place_radius_km` (default **1 km** in this mode) and `min_pts = min_place_items` (default **3**), using a k-d tree / ball tree over the located items (kiddo or the geocoder's tree), so it is O(n log n) for 100k+ photos.
- Why DBSCAN: it needs no cluster count, finds places of any shape (a beach, a long trail), and marks one-off photos as noise instead of forcing them into a place. HDBSCAN handles varying density better (a city block vs. a national park) but is more complex, slower on 100k points, and its result changes more when photos are added; it is noted as an M3 option if fixed `eps` proves poor. k-means is rejected (needs k, round clusters).
- **Noise points** (located but in no dense place) form a place of their own when the reverse geocoder gives them the same town; otherwise they go to "Other places", sorted by time.
- **No location**: items without GPS **fall back to date-based sets**, built exactly like visit-only mode (time-gap visits; undated items by folder sibling as today). They are listed after the places, under "No location".
- **Sub-groups and order**: a place with photos from several trips is split into **time sub-groups** by the visit gap (`visit_gap_hours`), so "Lisbon, PT" holds "Jun 2019" and "Mar 2023". Places are ordered by their most recent item (newest first, like visits today); sub-groups oldest first inside a place.
- **Titles**: place = geocoder name ("Lisbon, PT"); sub-group = "<place> · <date range>". IDs: place `g-<geohash7 of centroid>`; sub-group `g-<geohash7>-<first item date>`, stable when photos are added unless a cluster centre moves.

#### 20.2.3 Visit + geo (`visit-geo`)
Geo refines today's time-gap visits; it does not replace them.
1. Build time visits as now (`group::cluster`, `visit_gap_hours`, default unchanged).
2. Inside each visit, split by place with a simple, deterministic pass over time-sorted located items: start a new place cluster when the item is more than **`place_radius_km` (default 2 km)** from the running cluster centroid (haversine) **and** the place change persists for at least **`min_place_items` (default 3)** items or **30 minutes**. This absorbs single stray GPS fixes and short detours.
3. Merge neighbouring place clusters in the same visit when their centroids are within `place_radius_km` (you left and came back).
4. Items **without** location stay with the cluster nearest in time inside their visit (or the whole visit if it has no located items). Undated items keep today's folder-sibling rule; undated **and** unlocated items behave exactly as now.
5. Optional cross-day joining (same place on consecutive days, for example a weekend at one campsite): join adjacent visits when centroids are within `place_radius_km` and the gap is under **36 h**. Off by default in v0.6, behind a setting; decided after Rob tries it.
Why not DBSCAN/HDBSCAN: the data is already a time series, the time-then-distance pass is O(n), stable when photos are added, and easy to explain. Thresholds are config in `AppConfig` (`placeRadiusKm`, `minPlaceItems`, `joinSamePlaceDays`) and CLI flags.

### 20.3 Place names (offline reverse geocoder)
Compared 25 Oct 2026:

| Option | Code licence | Data | Size | Rust | Mac |
|---|---|---|---|---|---|
| **A. `reverse_geocoder` crate (gx0r/rrgeo) 4.1.1** | MIT OR Apache-2.0 | GeoNames `cities1000` (CC BY 4.0), k-d tree | crate about 2.2 MB compressed, about 150k places in memory | native crate | through the existing Rust helper |
| B. Own table: GeoNames `cities1000`/`cities15000` + `admin1CodesASCII` + `countryInfo`, `kiddo` k-d tree (MIT/Apache) | ours (MIT) | CC BY 4.0 | cities15000 about 3 MB, cities1000 about 10 MB raw | yes | through the helper, or a Swift k-d tree over the same file |
| C. Nominatim / Photon (OSM) self-hosted | GPL-2.0 / Apache-2.0 | OSM, ODbL | tens of GB, needs a server or Java | HTTP only | HTTP only |

Rejected: C (far too big, a server, ODbL share-alike obligations) and Apple `CLGeocoder` (online, Apple-only, rate limited).

**Recommendation: A behind our own small `unswiper-geo` crate**, so we can swap to B later (for example to add admin-1 region names or translations) without touching callers. Output per place: nearest city/town name, admin-1 (state) and country code; titles like "Bend, OR" or "Lisbon, PT". The Mac app gets the same answers from the bundled Rust helper (`unswiper-blur`, already shipped in the dmg and called over JSON lines): add a `{"cmd":"geo","points":[[lat,lon],...]}` request. One implementation, identical names on both platforms. GeoNames attribution (CC BY 4.0) goes in About/Help and `THIRD_PARTY_NOTICES`. Data is a build-time asset; the Flatpak and AppImage need no new permissions.

### 20.4 Fit with existing visit/date grouping
- Visit IDs stay stable: a geo sub-group's ID is `<visit id>-p<n>`, ordered by first item time. A visit with one place keeps its existing ID and looks the same as today.
- Titles: "<place> · <date range>"; visits without location keep today's date title and folder hint.
- Decisions, commits, restore and manifests are per item, so nothing changes there.

### 20.5 `library.json` schema and migration
- `CURRENT_SCHEMA_VERSION` 1 → **2**.
- `MediaItem` gets optional `location: {lat, lon, source: "exif"|"quicktime"|"takeout"}` (rounded to 5 decimals, about 1 m) — written only when present, so v1 readers' `extra` passthrough keeps working.
- `VisitGroup` gets optional `place: {name, admin1, country, lat, lon}` and `parentVisitId`.
- `AppConfig` gets **`groupMode`** (`"visit"` | `"geo"` | `"visit-geo"`, default **`"visit"`**), `placeRadiusKm` (optional; defaults 1 km for `geo`, 2 km for `visit-geo`), `minPlaceItems`, `joinSamePlaceDays`. A missing or unknown `groupMode` reads as `visit`.
- v1 migration: a v1 file has no `groupMode`, so it loads as `visit`: identical grouping to v0.5. `location` is still filled on the next index, so switching modes later needs no re-scan. The setting is written only when the user changes it.
- Migration: a v1 file loads as before; the next index fills `location` and regroups. Older app versions opening a v2 file keep unknown fields (the existing `extra` flatten) and fall back to time grouping. Swift and Rust share the same serialised layout (`to_json` golden tests).

### 20.6 UI
- **GTK and Mac**: the visit list shows the place name as the title and a small pin icon; a visit with several places expands into its places. A **Grouping** picker in Preferences / Library settings (GTK: an `AdwComboRow` in the preferences window; Mac: a `Picker` in Settings and in the View menu) with **By visit (time)** (default, exact v0.5 behaviour), **By place** and **By visit and place**. Changing it regroups in the background and saves `groupMode`. In **By place** the sidebar shows places with their time sub-groups, then "Other places" and "No location". Photo Info (§17) shows the place name next to the coordinates it already shows.
- No maps or map tiles in v0.6 (tiles would need the network). A later option could link "Open in Maps" that the user clicks themselves.

### 20.7 CLI
- `unswiper-cli index <folder> [--group-mode visit|geo|visit-geo] [--place-radius-km N] [--min-place-items N] [--join-same-place]`.
- `--group-mode` overrides the saved `AppConfig.groupMode` **for that run only**; it is never written back. Without it the saved setting is used (default `visit`). `--no-geo` is dropped: it is `--group-mode visit`. Geo flags are ignored with a warning in `visit` mode; `--join-same-place` applies only to `visit-geo`.
- `unswiper-cli config set group-mode <mode> <library.json>` changes the saved setting.
- `unswiper-cli groups <library.json> --json` includes `place`.
- `unswiper-cli geo <lat> <lon>` prints the place name (debug and tests).

### 20.8 Tests
- Modes: `visit` output is byte-identical to v0.5 on the same library (golden test); CLI `--group-mode` overrides without changing the saved config; unknown/missing `groupMode` reads as `visit`.
- Geo only: DBSCAN with known synthetic clusters, noise, a place visited on two trips (two sub-groups), no-location items in date sets, ordering and stable IDs.
- Visit + geo: haversine, the split/merge rules with stray fixes, items without location, undated plus unlocated, cross-day join, `0,0` ignored, stable IDs after adding photos.
- Geocoder: fixed points (a few world cities, an ocean point, near the antimeridian and the poles) with expected names.
- Schema: v1 → v2 load and round trip; Rust/Swift golden JSON; Mac `swift test` through the helper `geo` command.
- Fixtures are synthetic files with made-up coordinates; **Rob's sample photos are never committed** (local calibration only).

### 20.9 Privacy
Location never leaves the machine: no network calls, the geocoder data is bundled, coordinates are stored only in the local `library.json`, and logs never print coordinates. Help (Privacy page) says so. Exports such as the §19.7 CSV leave out coordinates unless the user asks.

### 20.10 Milestones
- **M1 core**: `unswiper-geo` crate, location reading for photos and videos plus Takeout fallback, the three grouping modes (visit unchanged, DBSCAN geo, visit-geo split), schema v2 with `groupMode`, CLI `--group-mode`, tests.
- **M2 apps**: GTK and Mac UI, helper `geo` command, the Grouping mode picker in GTK and Mac preferences, Help and attribution.
- **M3 tune**: calibrate thresholds per mode (and decide whether `geo` needs HDBSCAN) on Rob's libraries (locally), cross-day join decision, release notes. Then v0.6.0 after Rob tests and says go.

### 20.11 Open points
- Decided (Rob, 25 Oct 2026): default `place_radius_km` is 1 km in `geo` and 2 km in `visit-geo`; tune in M3 with Rob's new geo-diverse test photos.
- Decided (Rob, 25 Oct 2026): start with option A; add option B data (GeoNames admin1CodesASCII, pinned snapshot) later only if admin-1 names outside the US are needed. The `unswiper-geo` crate keeps that swap local.
- Decided (Rob, 25 Oct 2026): all other §20 choices approved; development can start.
- Decided (Rob, 25 Oct 2026): videos without location (3GP, AVI, many MOV) take their place hint from neighbouring items taken at the same time.

### 20.12 M1 as built (25 Oct 2026)
- `unswiper-core::geo`: `group_with(items, GroupSettings, PlaceNamer)` with the three modes. `visit` calls the unchanged v0.5 `group::group`. Golden tests compare it against the Swift-written crosscheck library and the v0.5 grouper.
- `geo`: DBSCAN. Neighbours are found with a 3-D grid on the sphere (cell size = radius) instead of a k-d tree. Chord distance is never longer than arc distance, so no neighbour is missed, also at the poles and across the antimeridian. Each place is split into trips by `visitGapHours`, with IDs `g-<geohash7>-<YYYYMMDD of first item, UTC>` and `parentVisitId` = `g-<geohash7>`. Located noise in one town (2 or more items) forms a place; the rest goes to `g-other` "Other places". The date sets of items without a location get `parentVisitId` = `no-location`. Undated items that have a location join the trip holding most of their folder siblings.
- `visit-geo`: implements §20.2.3. A single visit at one place keeps its v0.5 ID and only gains `place` and a place title.
- Titles follow the existing app convention, date range first: `Jun 1–Jun 3 · Lisbon, PT`. A place without a name is shown as "Unnamed place".
- Place hint (§20.11): an item without a location uses the location of the nearest located item taken within 30 minutes, for grouping only. The hint is not stored.
- `unswiper-geo`: wraps `reverse_geocoder` 4.1.1. Points more than 100 km from any town get no name. US titles use the state abbreviation, other countries the country code.
- Schema 2: `CURRENT_SCHEMA_VERSION` = 2 in Rust. The Swift `currentSchemaVersion` stays 1 until M2, which is safe because the apps keep separate libraries.
- CLI: `groups <library.json>` uses the saved mode; any flag applies to that run only. `config set|get group-mode`. `geo <lat> <lon>`. `commit --dry-run --visit` uses the saved mode, so IDs match `groups`.
- Helper: `unswiper-blur geo` reads one JSON request on stdin and writes the places as JSON. It is a new subcommand; `scan`, `info` and `takeout` are unchanged.
- The indexer (and so the GTK app) fills `location`, but still groups by visit until M2 adds the mode picker.

### 20.13 M2 as built (25 Oct 2026)
- **GTK**: there is no preferences window, so the picker is a **Grouping** section in the main menu (radio items like Commit Mode). It saves `AppConfig.groupMode` and regroups at once. In `geo` and `visit-geo` the view-model regroups with `group_with` after every index, folder-watch batch and commit; `visit` keeps the v0.5 grouper and the live index unchanged. Place groups get a pin icon.
- **Mac**: `Models.swift` is schema 2 (`currentSchemaVersion` = 2; reads v1, writes v2) with `location`, `place`, `parentVisitId`, `groupMode` (string; missing or unknown = visit), `placeRadiusKm`, `minPlaceItems` and `joinSamePlaceDays`. A golden `library-v2.json` is written back byte for byte by both Rust and Swift. The picker sits in View ▸ Grouping and above the visits list. Place modes call the helper: `unswiper-blur group --library library.json [--mode M]` reads the saved library, reads GPS for items without a location, groups with the Linux code and prints `{groups, locations}`; it never writes library.json. Visit mode stays in Swift.
- **Attribution**: THIRD_PARTY_NOTICES is installed in the Flatpak and AppImage (`share/doc/unswiper`) and in the dmg (`Contents/Resources` and next to the app); GTK About ▸ Legal and Mac About Unswiper show it.
- **Help**: both Visits pages explain the three modes; the Mac Privacy page matches Linux on stored locations.
- Not in M2: the Swift indexer still does not read GPS itself (the helper fills it on the first place-mode regroup), and the Takeout `geoData` fallback is Linux-only until M3.

### 20.14 M3 as built (25 Oct 2026)
Smoke test on Rob's geo-diverse photos (142 items) and the calibration set (91 items), run locally. Only counts are recorded here; the photos are never committed.
- **Dates**: capture date, then (photos) the EXIF GPS timestamp in UTC (`dateSource: "gps"`), then a date in the file name (`"filename"`), then the folder name. File names: `YYYYMMDD[_-]HHMMSS` anywhere (`IMG_`, `VID_`, `PANO_`, `PXL_`, plain), 14+ digit runs (`BURST…`), and `YYYY-MM-DD[ _-]HH.MM.SS`. Years 1990–2099, the date must exist, no time = local noon, `PXL_` = UTC. Same code shape in Rust (`meta::file_name_date`, `meta::exif::gps_date`) and Swift (`FileNameDate`, `LocalTreeIndexer.gpsInfo`). Effect: undated items fell from 56 to 1 (142 items) and from 52 to 1 (91 items). Visit-mode grouping is unchanged for the same dates (golden tests), but re-indexed libraries get more visits because more items have dates (23 → 28 and 11 → 13).
- **Geo tuning**: DBSCAN at 1 km did **not** chain. The 19 km (and 3 km) wide places came from grouping DBSCAN noise by town name alone. Fixes: (1) noise in one town forms a place only through single-link chains of at most 2 radii (`TOWN_NOISE_RADII`); (2) places with the same town label whose centres are within 5 km (`TOWN_MERGE_KM`) merge. Result: 15 → 11 places, widest place 19.25 → 4.5 km (a merged town), towns appearing as more than one place 4 → 1 (two places of one name 7.5 km apart, kept apart), closest place centres 1.19 → 4.05 km. Other places now holds 6 items instead of 2.
- **HDBSCAN** (scikit-learn, haversine, min cluster size 3) on the same points gave 20 clusters with one 37.5 km wide cluster and more fragmentation. It also adds a dependency. Not adopted.
- **Radius**: after the town merge, geo results barely change between 0.5 and 2 km (11 places each), and visit-geo gives the same 3 split visits at 1, 2 and 3 km. The defaults stay 1 km (geo) and 2 km (visit-geo).
- **visit-geo**: a detour of 2 or more items (`MOVE_ON_ITEMS`) that moves on or ends the visit without returning is now its own part. Before, it was treated as a stray, and visits covering two places were left whole. visit-geo groups spanning more than one geo place: 4 → 0 (142 items) and 2 → 0 (91 items).
- **Mac**: the indexer reads EXIF GPS (location and timestamp) and QuickTime ISO 6709 locations itself; the helper `group` command still fills any missing locations.

### 20.15 M3b: By Place by name (decision, 25 Oct 2026)

Rob, 25 Oct 2026: By Place should group all photos with one place name, e.g. Walla Walla, into a single group regardless of date and time. Distance clustering gave many separate places for one town. This supersedes DBSCAN (§20.3 and §20.14) for `geo`:

- `geo`: key = reverse-geocoded (name, admin1, country). One group per key, items in time order, no trips (`parentVisitId` empty), newest place first. Keys whose short label collides get the title "name, admin1, country". Located items with no name (> 100 km from any town) go to **Other places**; unlocated items to one **No location** group. Videos still borrow a location within 30 minutes (§20.11). `placeRadiusKm` and `minPlaceItems` no longer apply to `geo`; `dbscan` stays in the code for reference.
- `visit-geo`: kept. Each visit splits where the place name changes (stray, 30-minute and move-on rules unchanged) instead of at the 2 km radius. Neighbouring towns under 2 km apart now split; spots over 2 km apart with one name stay together. The radius is used only by `joinSamePlace`.
- `visit`: unchanged.

## 21. Future: Map view (heat map) (idea 25 Oct 2026; not scheduled)

Rob's idea, 25 Oct 2026, in his terms:

- A Google Photos-style map: the Collection shown as dots on a map.
- Photo density shown as a colour gradient, like the heat or rain intensity on a weather map.
- Zoom into an area and a separate thumbnail panel shows the photos there, fewer as you zoom in, in time order. Spots visited often then show similar photos, or how the area changes over time.

Notes:

- This needs a true map application: likely a separate app or a later milestone, not part of v0.6.
- The v0.6 data already supports it: per-item `location` and date, and place groups in library.json schema 2.
- Open question: map tiles. Online tiles leak the viewed area to the tile provider, which conflicts with the local-only privacy promise. Offline tiles cost disk space.
- By Place by name (§20.15) is the interim step.
