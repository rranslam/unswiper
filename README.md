# Unswiper

Pare down your photos and videos **one visit at a time** (grouped by place and time), instead of swiping left/right through everything. A local **desktop** app for macOS; Linux and mobile later.

*Formerly visit-media-reduce.* Free and open source under the [MIT License](LICENSE).

**Status:** macOS SwiftUI MVP in [`apps/macos`](apps/macos) (local tree source). Review fixes P0/P1/P2 applied 2026-10-11 — see [`docs/review-fixes-2026-10-11.md`](docs/review-fixes-2026-10-11.md). Linux, Google source, and mobile not started.

## Download & Install (macOS)

You need a Mac running **macOS 14 (Sonoma) or newer**. Works on Apple Silicon and Intel Macs.

### Step 1 — Download

1. Open this link in Safari (or any browser): **https://github.com/rranslam/visit-media-reduce/releases/latest**
2. Scroll down to the section called **Assets**. (If you only see a small triangle and the word "Assets", click it to open the list.)
3. Click the file whose name ends in **`.dmg`**, for example **`Unswiper-0.1.0.dmg`**. Do *not* click "Source code".
4. Your browser downloads it. In Safari, a blue arrow in the top-right of the window shows the download progress. Wait until it finishes.

### Step 2 — Install

1. Open **Finder** (the blue-and-white smiley face at the left end of the Dock, the row of icons at the bottom of your screen).
2. In the Finder window's left sidebar, click **Downloads**.
3. **Double-click** `Unswiper-0.1.0.dmg`. After a few seconds a new window opens showing the **Unswiper** icon and an **Applications** folder icon.
4. **Click and hold** on the Unswiper icon, **drag** it on top of the Applications folder icon, and let go. This copies the app into your Applications folder.
5. Eject the installer disk: in the Finder sidebar, under **Locations**, find **Unswiper 0.1.0** and click the small **eject (⏏)** button next to it. (You can also drag the `.dmg` file in Downloads to the Trash now; you don't need it anymore.)

### Step 3 — Open it the first time

Unswiper is not yet signed by a paid Apple Developer account, so the first time you open it macOS will stop it with a warning like *"Unswiper" Not Opened* or *Apple could not verify "Unswiper" is free of malware*. This is expected. You only have to approve it **once**.

**macOS 15 (Sequoia) and newer:**

1. In Finder's sidebar click **Applications**, then **double-click Unswiper**.
2. A warning appears. Click **Done** (do **not** click "Move to Trash").
3. Click the **Apple menu ()** in the top-left corner of the screen, then click **System Settings…**.
4. In the left column of System Settings, click **Privacy & Security**.
5. On the right side, **scroll all the way down** to the **Security** section. You'll see a line saying *"Unswiper" was blocked to protect your Mac.*
6. Click **Open Anyway** next to it.
7. Type your Mac login password (the one you use to unlock your Mac) and click **OK**, or use Touch ID.
8. One more box may appear asking if you want to open it. Click **Open Anyway** (or **Open**).

Unswiper now opens. From now on you can open it normally.

**macOS 14 (Sonoma):** In Finder go to **Applications**, hold the **Control** key and click Unswiper (or right-click it), choose **Open** from the menu, then click **Open** in the box that appears.

**Tip:** To find Unswiper later, press **Command (⌘) + Space**, type `Unswiper`, and press Return. To keep it in the Dock, while it's open, right-click its Dock icon and choose **Options → Keep in Dock**.

### Updating to a new version

1. Quit Unswiper if it's open (click its window, then press **⌘ + Q**).
2. Download the newest `.dmg` from the [Releases page](https://github.com/rranslam/visit-media-reduce/releases/latest) and install it exactly as in Steps 1–2.
3. When Finder asks whether to replace the existing Unswiper, click **Replace**.
4. You may need to do the Step 3 "Open Anyway" approval again for the new version.

Your settings and decisions are kept when you update.

### Uninstalling

1. Quit Unswiper (**⌘ + Q**).
2. Open **Finder → Applications**, drag **Unswiper** to the **Trash** in the Dock.
3. To also remove its saved data: in Finder, click the **Go** menu at the top of the screen, choose **Go to Folder…**, paste `~/Library/Application Support/` and press Return. Drag the **Unswiper** folder (and the **visit-media-reduce** shortcut, if you see one) to the Trash.
4. Empty the Trash if you want (right-click the Trash in the Dock → **Empty Trash**).

Uninstalling never touches your photos. Files you rejected are in the macOS Trash or in a `_Rejected/Unswiper` folder inside your photo folder (older versions used `_Rejected/visit-media-reduce`) until you delete them yourself.

## Build from source (macOS)

Requires full **Xcode** (not just Command Line Tools) with the license accepted.

```
cd apps/macos
swift build
swift run
```

To build the downloadable app and `.dmg` yourself:

```
apps/macos/scripts/build-dmg.sh 0.1.0
```

The result is in `apps/macos/dist/` (`Unswiper.app` and `Unswiper-0.1.0.dmg`). Pushing a tag such as `v0.1.0` to GitHub runs `.github/workflows/release.yml`, which builds the `.dmg` and attaches it to a GitHub Release.

Try it on a **copy** of a small photo folder first. Re-index after upgrading (new index fields, stable visit IDs).

## MVP features

- Pick a local folder; background indexer (EXIF dates for photos, `AVAsset` creation date for videos)
- Visit groups by time gap, sorted by size; "Undated" group for files with no usable date
- Reduce screen with per-visit keep target, keep/reject, side-by-side compare
- Commit rejects to Trash or a `_Rejected/` quarantine folder, or dry run (nothing moves)
- Safety: files validated (inside library, unchanged size/mtime) before moving; confirm dialog; Live Photo + sidecar pairing; per-commit manifest with **Restore Last Commit**
- Commit/restore off the main thread; debounced saves

Formerly: `visit-media-reduce` and, before that, `android-visit-media-reduce` (stub left at that path). The GitHub repo and local folder keep the name `visit-media-reduce` for now.

## Restart here

1. [`STATUS.md`](STATUS.md) — current direction and resume checklist  
2. [`SESSION.md`](SESSION.md) — full prompts + chats  
3. Product docs:
   - [`docs/architecture.md`](docs/architecture.md) — staged architecture (desktop-first, cross-platform)
   - [`docs/ux-model.md`](docs/ux-model.md) — desktop MVP UX
   - [`docs/sources-and-setup.md`](docs/sources-and-setup.md) — **Google API** *or* **local tree** config
4. History:
   - [`docs/architecture-android-mvp.md`](docs/architecture-android-mvp.md) — prior Android-first MVP (superseded as primary target)
   - [`docs/architecture-draft-v0.md`](docs/architecture-draft-v0.md) — earliest flat scan draft
   - [`snapshots/`](snapshots/) — dated freezes

## Product one-liner

Point at a **local media tree** *or* connect **Google** (OAuth) → form visit groups → reduce with context (e.g. keep ~10 of 50 from a Mt Hood trip) → commit rejects safely on disk (and/or via API where allowed).

## Tree

```
visit-media-reduce/   (repo folder; app is Unswiper)
├── README.md
├── STATUS.md
├── SESSION.md
├── apps/macos/               ← SwiftUI macOS MVP (Swift Package)
├── config/examples/          ← sample config for Google + local tree
├── docs/
│   ├── architecture.md
│   ├── review-fixes-2026-10-11.md
│   ├── ux-model.md
│   ├── sources-and-setup.md
│   ├── architecture-android-mvp.md
│   └── architecture-draft-v0.md
└── snapshots/
    ├── 2026-10-11/           ← Android-era planning freeze
    └── 2026-10-11-desktop-refactor/
```

## Cursor chat reference

- Transcript id: `f5c9a60c-cfae-4433-bec3-3e9030987803`
- Path: `~/.cursor/projects/Users-rranslam/agent-transcripts/f5c9a60c-cfae-4433-bec3-3e9030987803/`

## Suggested next steps

- Test the macOS MVP on a copied real library; collect UX issues  
- Add tests for commit validation, restore, and visit grouping  
- Geo-aware grouping and better visit titles  
- Google source (OAuth desktop) using `docs/sources-and-setup.md`  
- Linux build path (shared domain core) and later mobile

## License

MIT — see [LICENSE](LICENSE). Copyright (c) 2026 Rob Ranslam.
