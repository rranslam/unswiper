# Unswiper

Pare down your photos and videos **one visit at a time** (grouped by place and time), instead of swiping left/right through everything. A local **desktop** app for macOS and Linux; mobile later.

*Formerly visit-media-reduce.* Free and open source under the [MIT License](LICENSE).

**Status:** macOS SwiftUI MVP in [`apps/macos`](apps/macos) (local tree source). Review fixes P0/P1/P2 applied 2026-10-11 — see [`docs/review-fixes-2026-10-11.md`](docs/review-fixes-2026-10-11.md). Linux app (GTK 4, Rust core) released in 0.2.0 as AppImage and Flatpak. Google source and mobile not started.

## Download & Install (macOS)

You need a Mac running **macOS 14 (Sonoma) or newer**. Works on Apple Silicon and Intel Macs.

### Step 1 — Download

1. Open this link in Safari (or any browser): **https://github.com/rranslam/unswiper/releases/latest**
2. Scroll down to the section called **Assets**. (If you only see a small triangle and the word "Assets", click it to open the list.)
3. Click the file whose name ends in **`.dmg`**, for example **`Unswiper-0.2.0.dmg`**. Do *not* click "Source code".
4. Your browser downloads it. In Safari, a blue arrow in the top-right of the window shows the download progress. Wait until it finishes.

### Step 2 — Install

1. Open **Finder** (the blue-and-white smiley face at the left end of the Dock, the row of icons at the bottom of your screen).
2. In the Finder window's left sidebar, click **Downloads**.
3. **Double-click** `Unswiper-0.2.0.dmg`. After a few seconds a new window opens showing the **Unswiper** icon and an **Applications** folder icon.
4. **Click and hold** on the Unswiper icon, **drag** it on top of the Applications folder icon, and let go. This copies the app into your Applications folder.
5. Eject the installer disk: in the Finder sidebar, under **Locations**, find **Unswiper 0.2.0** and click the small **eject (⏏)** button next to it. (You can also drag the `.dmg` file in Downloads to the Trash now; you don't need it anymore.)

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

### Getting help inside the app

Once Unswiper is open, click **Help** in the menu bar at the top of the screen and choose **Unswiper Help** (or press **⌘ + ?**, that is Command + Shift + /). A Help window opens with step-by-step topics on the left and a search box. The Help menu also has **Keyboard Shortcuts**, **Release Notes**, and **Report an Issue**. See [CHANGELOG.md](CHANGELOG.md) for what changed in each version.

### Updating to a new version

1. Quit Unswiper if it's open (click its window, then press **⌘ + Q**).
2. Download the newest `.dmg` from the [Releases page](https://github.com/rranslam/unswiper/releases/latest) and install it exactly as in Steps 1–2.
3. When Finder asks whether to replace the existing Unswiper, click **Replace**.
4. You may need to do the Step 3 "Open Anyway" approval again for the new version.

Your settings and decisions are kept when you update.

**Updating from 0.1.x to 0.2.0:** the app's internal ID changed to `io.github.rranslam.Unswiper`. Your libraries and decisions are kept, but macOS asks **once more** for permission to open your photo folders. Click **Allow**.

### Uninstalling

1. Quit Unswiper (**⌘ + Q**).
2. Open **Finder → Applications**, drag **Unswiper** to the **Trash** in the Dock.
3. To also remove its saved data: in Finder, click the **Go** menu at the top of the screen, choose **Go to Folder…**, paste `~/Library/Application Support/` and press Return. Drag the **Unswiper** folder (and the **visit-media-reduce** shortcut, if you see one) to the Trash.
4. Empty the Trash if you want (right-click the Trash in the Dock → **Empty Trash**).

Uninstalling never touches your photos. Files you rejected are in the macOS Trash or in a `_Rejected/Unswiper` folder inside your photo folder (older versions used `_Rejected/visit-media-reduce`) until you delete them yourself.

## Download & Install (Linux)

Works on a recent 64-bit Linux, for example **Ubuntu 24.04 or newer** (GNOME) and **Arch / Omarchy** (Hyprland), on normal PCs **and** on Arm computers. Pick **one** of the two downloads below. If you're unsure, use the **AppImage**.

### Step 0 — Which file do I need? (x86_64 or aarch64)

Every download comes in two versions. The name ends in the type of processor it runs on:

| Your computer | Pick the file with |
|---|---|
| Normal PC or laptop (Intel or AMD), including Linux in a VM on an **Intel** Mac | **`x86_64`** |
| **Apple Silicon Mac** (M1–M5) running Linux: Asahi Linux, or a Linux VM (UTM, Parallels, VMware Fusion, try_omarchy-style) | **`aarch64`** |
| Other Arm boards (Raspberry Pi 4/5 with 64-bit OS, Arm laptops) | **`aarch64`** |

Not sure? Open a Terminal and type `uname -m`, then press Enter. It prints **`x86_64`** or **`aarch64`**; pick the file with that word in its name. If you picked the wrong one, starting it fails with **`Exec format error`**: just download the other one.

The commands below use the x86_64 names. **On Arm, type `aarch64` instead of `x86_64`** everywhere (for example `Unswiper-aarch64.AppImage`).

### Option A — AppImage (one file, nothing to install)

1. Open **https://github.com/rranslam/unswiper/releases/latest** in your browser.
2. Under **Assets**, click **`Unswiper-x86_64.AppImage`** (Arm: **`Unswiper-aarch64.AppImage`**). It is saved to your **Downloads** folder.
3. Open a **Terminal** (Ubuntu: press the **Super/Windows key**, type `Terminal`, press Enter. Omarchy: **Super + Enter**).
4. Type these lines, pressing **Enter** after each one:
   ```bash
   cd ~/Downloads
   chmod +x Unswiper-x86_64.AppImage
   ./Unswiper-x86_64.AppImage
   ```
   `chmod +x` marks the file as a program. You only need to do it once.
5. Unswiper opens.

**If nothing happens and you see an error mentioning `libfuse.so.2` or FUSE** (common on Ubuntu 24.04), install FUSE 2 once and try step 4 again:
```bash
sudo apt install libfuse2t64
```
(On Ubuntu 22.04 the package is called `libfuse2`. On Arch/Omarchy: `sudo pacman -S fuse2`.) Without FUSE you can still run it with `./Unswiper-x86_64.AppImage --appimage-extract-and-run`.

**Arch Linux ARM / Omarchy on Arm (Asahi or a Mac VM):** FUSE 2 is often missing or not installable there. If the AppImage prints a FUSE error, skip FUSE and start it like this instead (it unpacks itself to a temporary folder each time, which takes a few seconds):
```bash
cd ~/Downloads
chmod +x Unswiper-aarch64.AppImage
./Unswiper-aarch64.AppImage --appimage-extract-and-run
```

**Tip:** Move the file somewhere permanent, e.g. `mkdir -p ~/Applications && mv ~/Downloads/Unswiper-x86_64.AppImage ~/Applications/`. To update, download the new AppImage and replace the old file (repeat the `chmod +x`).

### Option B — Flatpak (sandboxed, adds Unswiper to your app menu)

1. Make sure Flatpak and the Flathub source are set up (one time):
   - **Ubuntu 24.04:** `sudo apt install flatpak`, then log out and back in.
   - **Arch / Omarchy:** `sudo pacman -S flatpak`.
   - Then, on both: `flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo`
2. From the [latest release](https://github.com/rranslam/unswiper/releases/latest), download **`Unswiper-x86_64.flatpak`** (Arm: **`Unswiper-aarch64.flatpak`**) to **Downloads**.
3. In a Terminal:
   ```bash
   cd ~/Downloads
   flatpak install --user Unswiper-x86_64.flatpak
   ```
   (Arm: `flatpak install --user Unswiper-aarch64.flatpak`.)
   Answer **y** when asked. The first install also downloads the GNOME runtime and video codecs from Flathub (several hundred MB, one time only).
4. Start it from your app menu (**Unswiper**), or with `flatpak run io.github.rranslam.Unswiper`.

To update, download the new `.flatpak` and run the same `flatpak install --user` command. To uninstall: `flatpak uninstall --user io.github.rranslam.Unswiper`. Unswiper is not on Flathub yet.

### Notes for Arch / Omarchy (Hyprland)

- Both downloads run natively on **Wayland**/Hyprland; no X11 needed.
- For the AppImage, install FUSE 2 once: `sudo pacman -S fuse2`. On **Arm** (Apple Silicon Mac with Asahi or a VM) use the **aarch64** files; if `fuse2` isn't available, run the AppImage with `--appimage-extract-and-run` (see above) or use the Flatpak.
- The Flatpak adds Unswiper to the Omarchy app launcher (Super + Space) automatically; the AppImage doesn't, so start it from a terminal.
- Choosing a library folder uses the desktop's file dialog portal. Omarchy ships `xdg-desktop-portal-gtk`/`-hyprland`; if the folder dialog doesn't open, install `xdg-desktop-portal-gtk`.

### Where Linux keeps its data

Decisions and commit history: `~/.local/share/unswiper/` (Flatpak: `~/.var/app/io.github.rranslam.Unswiper/data/unswiper/`). Rejected files go to your desktop Trash (or `_Rejected/Unswiper` inside the library, if you chose that), so you can restore them. Removing the app never touches your photos.

## Build from source (macOS)

Requires full **Xcode** (not just Command Line Tools) with the license accepted.

```
cd apps/macos
swift build
swift run
```

Run the automated tests (also run by GitHub on every push and pull request, see `.github/workflows/ci.yml`):

```
cd apps/macos
swift test
```

To build the downloadable app and `.dmg` yourself:

```
apps/macos/scripts/build-dmg.sh 0.1.2
```

The result is in `apps/macos/dist/` (`Unswiper.app` and `Unswiper-0.2.0.dmg`). Pushing a tag such as `v0.1.2` to GitHub runs `.github/workflows/release.yml`, which builds the `.dmg` and attaches it to a GitHub Release.

Try it on a **copy** of a small photo folder first. Re-index after upgrading (new index fields, stable visit IDs).

## MVP features

- Pick a local folder; background indexer (EXIF dates for photos, `AVAsset` creation date for videos)
- Visit groups by time gap, sorted by size; "Undated" group for files with no usable date
- Reduce screen with per-visit keep target, keep/reject, side-by-side compare
- Commit rejects to Trash or a `_Rejected/` quarantine folder, or dry run (nothing moves)
- Safety: files validated (inside library, unchanged size/mtime) before moving; confirm dialog; Live Photo + sidecar pairing; per-commit manifest with **Restore Last Commit**
- Commit/restore off the main thread; debounced saves

Formerly: `visit-media-reduce` and, before that, `android-visit-media-reduce` (stub left at that path). The GitHub repo was renamed to `unswiper` (old links redirect); the local folder may still be named `visit-media-reduce`.

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
unswiper/   (repo folder)
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
