<!-- SPDX-License-Identifier: MIT -->
<!-- Copyright (c) 2026 Rob Ranslam -->
# 0.2.0 checkout

A manual pass before announcing 0.2.0. Use a **copy** of a small folder (about 20 files), with a few
JPEGs, at least one **HEIC** photo, and one iPhone **.mov** (H.264 or HEVC) from two different days.
Download the files from the release page: `x86_64` files for Intel/AMD PCs, `aarch64` files for Arm (Apple Silicon Mac running Asahi or a Linux VM). `uname -m` tells you which. Tick each step; note anything odd in an issue.

## macOS (Unswiper-0.2.0.dmg)

1. Install: open the dmg, drag Unswiper to Applications, start it, approve via System Settings ▸ Privacy & Security ▸ **Open Anyway**. If 0.1.x was installed, check that your old library and window size are still there and that macOS asks again for folder access (bundle ID changed).
2. Index: choose the test folder. Visits appear grouped by day; counts match the folder.
3. HEIC + video: the HEIC shows a thumbnail; the .mov shows a frame thumbnail.
4. Keep/Reject: open a visit, mark 2 files Reject (R) and the rest Keep (K). Counts update.
5. Commit: Review & Commit. The dialog names *this visit* and the reject count; other visits are untouched. The 2 files go to the Trash (or `_Rejected/Unswiper`).
6. Restore: Restore Last Commit. Both files are back in place; the visit shows them again.
7. Watching: with Unswiper open, copy a new photo into the folder. It appears within a few seconds without Re-index.
8. Quit and reopen: decisions are still there.

## Ubuntu 24.04 (GNOME)

1. Install the AppImage: `chmod +x Unswiper-x86_64.AppImage && ./Unswiper-x86_64.AppImage`. If it complains about FUSE: `sudo apt install libfuse2t64`. Separately, install the Flatpak: `flatpak install --user Unswiper-x86_64.flatpak`, then start it from the app menu. Do steps 2–8 with **both**.
2. Index: **Choose Folder…** (Ctrl+O) and pick the test folder. Visits appear in the sidebar with photo/video counts.
3. HEIC + video: the HEIC shows a thumbnail; both .mov clips show a frame thumbnail (Flatpak: the first run may download the codecs extension).
4. Keep/Reject: open a visit, select 2 files and press **R**, select the rest and press **K**. The sidebar counts update.
5. Commit: **Review & Commit…** (Ctrl+Enter). The dialog names the open visit and "2 files"; confirm. The files are in the GNOME Trash (Files ▸ Trash); other visits are unchanged. Commit is greyed out for a visit with no rejects.
6. Restore: menu ▸ **Restore Last Commit** (Ctrl+Shift+R). Both files return to their folder.
7. Watching: copy a new photo into the folder with Files. It appears within a few seconds.
8. Quit (Ctrl+Q) and restart: decisions are kept. Help (F1) opens.

## Omarchy (Arch, Hyprland)

Do this on x86_64 and, if available, on an **Arm** setup (Apple Silicon Mac: Asahi or a Linux VM) with the `aarch64` files.


1. Install: `sudo pacman -S fuse2`, then `chmod +x Unswiper-x86_64.AppImage && ./Unswiper-x86_64.AppImage` (Arm: `Unswiper-aarch64.AppImage`; if FUSE is missing, add `--appimage-extract-and-run`). Check that the wrong-arch file fails with `Exec format error` and the right one starts. Check it runs as a **Wayland** window (`hyprctl clients` shows `xwayland: 0`). Optionally also the Flatpak (`sudo pacman -S flatpak`, add Flathub, `flatpak install --user Unswiper-<arch>.flatpak`) and launch it from Super + Space.
2. Index: Ctrl+O opens the portal folder dialog; pick the test folder. (If no dialog appears: `sudo pacman -S xdg-desktop-portal-gtk`.)
3. HEIC + video: HEIC and both .mov thumbnails show.
4. Keep/Reject: R / K on a selection; tiling resize keeps the grid usable.
5. Commit: Ctrl+Enter, confirm. The files are in `~/.local/share/Trash/files`.
6. Restore: Ctrl+Shift+R. The files are back.
7. Watching: `cp` a new photo into the folder from a terminal. It appears within a few seconds.
8. Quit and restart: decisions kept.
