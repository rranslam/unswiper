# unswiper-hash: user guide

> **Planned — not built yet.** This guide describes the approved design ([DESIGN.md](DESIGN.md), Rob, 26 Oct 2026). Commands, options and outputs may still change. All sample outputs below are **made-up examples** with no real files.

`unswiper-hash` keeps a fingerprint (hash) of every photo and video in a library, so you can find out later:
- which files were **damaged** on disk or in a copy;
- which were **edited**, or only had their **metadata** (dates, tags) changed;
- which were **rotated**, **moved or renamed**, and which are **exact duplicates**.

It only **reads** your files. It never changes, moves or deletes them, and it never writes anything into your photo folders. Its database lives in your app-data folder.

Contents: [Install](#install) · [Where things are stored](#where-things-are-stored) · [Mode 1: with the Unswiper apps](#mode-1-with-the-unswiper-apps) · [Mode 2: by hand](#mode-2-by-hand-standalone-cli) · [Mode 3: scheduled](#mode-3-scheduled-cron-systemd-launchd) · [Reading reports](#reading-reports) · [The log file](#the-log-file) · [What to do about each finding](#what-to-do-about-each-finding) · [Mac and Linux differences](#mac-and-linux-differences-at-a-glance)

Windows: planned later, after the Mac and Linux versions.

## Install

### Mac
`unswiper-hash` ships inside the Unswiper app (from the `.dmg`):

```console
/Applications/Unswiper.app/Contents/MacOS/unswiper-hash --version
```

To type just `unswiper-hash`, add a link to it in a folder on your PATH (once):

```console
sudo mkdir -p /usr/local/bin
sudo ln -s /Applications/Unswiper.app/Contents/MacOS/unswiper-hash /usr/local/bin/unswiper-hash
```

To read the man page, add the app's man folder to `MANPATH` in `~/.zshrc`:

```console
export MANPATH="/Applications/Unswiper.app/Contents/Resources/man:$MANPATH"
man unswiper-hash
```

### Linux
- **Flatpak:** `flatpak run --command=unswiper-hash io.github.rranslam.Unswiper --version`. For a short command, add `alias unswiper-hash='flatpak run --command=unswiper-hash io.github.rranslam.Unswiper'` to `~/.bashrc`. The Flatpak only sees folders it has been given access to: `flatpak override --user --filesystem=~/Pictures:ro io.github.rranslam.Unswiper`.
- **AppImage:** the AppImage contains the tool. Extract it once, or run `./Unswiper-x86_64.AppImage --appimage-extract usr/bin/unswiper-hash` and copy `squashfs-root/usr/bin/unswiper-hash` to `~/.local/bin/`.
- **From source:** `cargo install --path crates/unswiper-hash` (puts it in `~/.cargo/bin/`).
- Man page (AppImage and cargo): `unswiper-hash --print-man > ~/.local/share/man/man1/unswiper-hash.1`, then `man unswiper-hash`.

Shell completions (bash, zsh, fish) are in `completions/`.

## Where things are stored

| | Mac | Linux |
|---|---|---|
| Hash store (default) | `~/Library/Application Support/Unswiper/hashes.sqlite` | `$XDG_DATA_HOME/unswiper/hashes.sqlite` (usually `~/.local/share/unswiper/hashes.sqlite`) |
| Suggested log folder | `~/Library/Logs/Unswiper/` (shows in Console.app) | `$XDG_STATE_HOME/unswiper/` (usually `~/.local/state/unswiper/`) |
| External drives | `/Volumes/<name>` | `/media/<user>/<name>` or `/run/media/<user>/<name>` |

- Use `--store FILE` (or `UNSWIPER_HASH_STORE`) for a store elsewhere, e.g. on an external drive next to (not inside) the photo folder, so it travels with the drive.
- The store keeps paths **relative to the library folder**, so the same library under `/Volumes/Photos` on the Mac and `/media/rob/Photos` on Linux is recognised as the same library.

## Mode 1: with the Unswiper apps

**Today (until app integration, milestone M5):** the apps and `unswiper-hash` don't talk to each other. Use `unswiper-hash` by hand or on a schedule (modes 2 and 3) on the same folder you open in Unswiper. Nothing in your library or your Keep/Reject decisions changes.

**After app integration (planned, M5):** the apps **read** the hash store (they never write it). What changes for you:
- **Moves and renames keep your decisions.** If you move or rename photos outside Unswiper, your Keep/Reject choices and blur results follow them: automatically when exactly one file matches, or Unswiper asks when it's ambiguous.
- **"Possibly damaged" badge** on photos whose bytes changed although nothing should have touched them.
- **Library ▸ Verify Library…** runs `unswiper-hash verify` with a progress bar. Same results as the command line.
- library.json gets no hash fields; you can still delete the hash store at any time and rebuild it with `scan`.

## Mode 2: by hand (standalone CLI)

First scan: reads every photo and video once. This can take a while: roughly the time it takes to copy the library.

```console
$ unswiper-hash scan ~/Pictures
Scanning Pictures (store: ~/.local/share/unswiper/hashes.sqlite)
  48,211 files, 212.4 GB read in 31 min
  new 48,211
Done. Exit 0.
```

Later scans only read new and changed files, so they take seconds to minutes:

```console
$ unswiper-hash scan ~/Pictures
Scanning Pictures
  48,255 files, 0.9 GB read in 14 s
  new 37 · edited 2 · metadata only 3 · rotated 1 · moved 5 · missing 0
Done. Exit 0.
```

Quick look without reading any file contents:

```console
$ unswiper-hash status ~/Pictures
Pictures: 48,255 known · 0 new · 0 missing · 0 changed (size/date)
Last scan 2026-10-26 14:10 -07:00 · last verify 2026-10-19 03:00 -07:00
```

Full check: re-reads everything. It can be resumed: stop with Ctrl+C and run it again to continue.

```console
$ unswiper-hash verify ~/Pictures
Verifying Pictures: 48,255 files
  ok 48,252 · metadata only 2 · possibly damaged 1
POSSIBLY DAMAGED (same size and date, different content):
  2019/IMG_1234.JPG   last good 2026-10-19
Done. Exit 1 (needs attention).
```

Other commands:

```console
$ unswiper-hash moves ~/Pictures          # old path -> new path, from recent scans
$ unswiper-hash dups ~/Pictures           # exact duplicates, largest first
$ unswiper-hash export ~/Pictures --out Pictures-hashes-2026-10-26.json
$ unswiper-hash export ~/Pictures --format sha256sums --out SHA256SUMS
$ unswiper-hash diff Pictures-hashes-2026-10-26.json /Volumes/Backup/Pictures
```

Comparing a library with a backup, or between two machines:

```console
$ unswiper-hash diff ~/Pictures /Volumes/Backup/Pictures
same 48,190 · only in A 61 · only in B 3 · moved 2 · different content 1
DIFFERENT CONTENT:
  2021/PXL_20210921_201801.jpg
Done. Exit 1.
```

**Checksum files without unswiper-hash.** `export --format sha256sums` writes a standard file that the built-in tools can check. Run the check from the library folder:
- **Mac:** `shasum -a 256 -c SHA256SUMS`
- **Linux:** `sha256sum -c SHA256SUMS`
- `--format b3sums` needs `b3sum` (`brew install b3sum` on the Mac, the `b3sum` package on Linux), then `b3sum -c B3SUMS`.

Useful options: `--json` (machine-readable results on stdout), `--exclude GLOB`, `--jobs N` (use 1 for a spinning disk), `--low-priority`, `--limit-mb-s N`, `-v`/`-q`, `--log-file PATH`, `--log-level LEVEL`. See `man unswiper-hash` or `unswiper-hash <command> --help`.

## Mode 3: scheduled (cron, systemd, launchd)

A weekly `verify` is the typical setup: it re-reads everything and tells you only when something needs attention. `unswiper-hash schedule --print` prints a ready-made job for your system (it never installs anything); the examples below are what it prints.

Rule of thumb: **results go to stdout, log messages to stderr**. With `-q`, a run with nothing to report prints nothing, so cron sends no mail; a run with findings prints the report, so you get an email. The log file keeps the full history.

### Linux: systemd user timer (recommended)

`~/.config/systemd/user/unswiper-hash-verify.service`:
```ini
[Unit]
Description=Verify photo library hashes

[Service]
Type=oneshot
Nice=10
IOSchedulingClass=idle
ExecStart=%h/.local/bin/unswiper-hash verify %h/Pictures --log-level info
```

`~/.config/systemd/user/unswiper-hash-verify.timer`:
```ini
[Timer]
OnCalendar=Sun 03:00
Persistent=true

[Install]
WantedBy=timers.target
```

```console
$ systemctl --user daemon-reload
$ systemctl --user enable --now unswiper-hash-verify.timer
$ systemctl --user list-timers unswiper-hash-verify.timer
$ journalctl --user -u unswiper-hash-verify -n 50      # read the last run
```

- `Persistent=true` runs a missed job (machine off or asleep at 03:00) after the next boot.
- To run while logged out: `loginctl enable-linger $USER`.
- Flatpak: use `ExecStart=/usr/bin/flatpak run --command=unswiper-hash io.github.rranslam.Unswiper verify %h/Pictures`.

### Linux or Mac: cron

```console
$ crontab -e
# weekly verify, Sunday 03:00; mail only when something needs attention
MAILTO=rob@example.com
0 3 * * 0  $HOME/.local/bin/unswiper-hash verify "$HOME/Pictures" -q --log-file "$HOME/.local/state/unswiper/hash.log"
```

cron does **not** run missed jobs: if the computer is asleep or off at 03:00, that week is skipped. On the Mac, prefer launchd.

### Mac: launchd (recommended on the Mac)

`~/Library/LaunchAgents/io.github.rranslam.unswiper-hash.verify.plist`:
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>io.github.rranslam.unswiper-hash.verify</string>
  <key>ProgramArguments</key><array>
    <string>/Applications/Unswiper.app/Contents/MacOS/unswiper-hash</string>
    <string>verify</string><string>/Users/rob/Pictures</string>
    <string>--log-file</string><string>/Users/rob/Library/Logs/Unswiper/hash.log</string>
  </array>
  <key>StartCalendarInterval</key><dict>
    <key>Weekday</key><integer>0</integer><key>Hour</key><integer>3</integer><key>Minute</key><integer>0</integer>
  </dict>
  <key>ProcessType</key><string>Background</string>
  <key>StandardOutPath</key><string>/Users/rob/Library/Logs/Unswiper/hash-report.txt</string>
  <key>StandardErrorPath</key><string>/Users/rob/Library/Logs/Unswiper/hash.err</string>
</dict></plist>
```

```console
$ mkdir -p ~/Library/Logs/Unswiper
$ launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/io.github.rranslam.unswiper-hash.verify.plist
$ launchctl kickstart gui/$(id -u)/io.github.rranslam.unswiper-hash.verify     # run once now, to test
$ launchctl print gui/$(id -u)/io.github.rranslam.unswiper-hash.verify | grep -E "state|last exit"
$ launchctl bootout gui/$(id -u)/io.github.rranslam.unswiper-hash.verify       # remove the job
```

(`launchctl load -w <plist>` also works on older macOS.) Mac notes:
- **Sleep:** if the Mac is asleep at 03:00, launchd runs the job when it wakes (one run, not one per missed week). It doesn't wake the Mac. If you want that, use `sudo pmset repeat wakeorpoweron Su 02:55:00`.
- **Permissions:** macOS protects `~/Pictures`, `~/Desktop`, `~/Documents`, the Photos library and external drives. A background job gets "Operation not permitted" (logged as a `warn` per folder, exit 3 if nothing could be read) until you allow it: **System Settings ▸ Privacy & Security ▸ Full Disk Access**, then add `Unswiper.app` (the tool inside it inherits the permission). For cron, add `/usr/sbin/cron` instead.
- The **Photos app library** (`Photos Library.photoslibrary`) is a package managed by Photos. Scanning it is read-only and safe but reports internal files; Unswiper is meant for plain folders.

## Reading reports

### Exit codes
| Code | Meaning | What to do |
|---|---|---|
| 0 | All good. New, edited, moved, rotated or metadata-only files are normal and exit 0 | Nothing |
| 1 | Needs attention: possibly damaged files, or `diff` found differences | Read the report ([below](#what-to-do-about-each-finding)) |
| 2 | Usage error (unknown option, bad level) | Check the command |
| 3 | Could not run: I/O or store error, permission denied, folder missing (e.g. the external drive is not connected) | Read the log; on the Mac check Full Disk Access |

### JSON
With `--json`, stdout carries one JSON document and nothing else (log messages stay on stderr):

```console
$ unswiper-hash scan ~/Pictures --json | jq '.counts'
{
  "files": 48255, "read": 41, "new": 37, "edited": 2, "metadataOnly": 3,
  "rotated": 1, "moved": 5, "missing": 0, "possiblyDamaged": 0
}
```

```json
{
  "format": 1, "command": "scan", "root": "Pictures", "libraryId": "lib-3f9c…",
  "startedAt": "2026-10-26T14:09:49-07:00", "finishedAt": "2026-10-26T14:10:03-07:00",
  "exitCode": 0,
  "counts": { "files": 48255, "read": 41, "new": 37, "edited": 2, "metadataOnly": 3, "rotated": 1, "moved": 5, "missing": 0, "possiblyDamaged": 0 },
  "findings": [
    { "kind": "moved", "from": "Inbox/IMG_0042.HEIC", "to": "2026/Lisbon/IMG_0042.HEIC" },
    { "kind": "metadataOnly", "path": "2024/IMG_20240612_133241.jpg" },
    { "kind": "rotated", "path": "2023/DSC_0101.JPG", "rotation": 90, "approx": false },
    { "kind": "edited", "path": "2025/PXL_20250610_232011.jpg" }
  ]
}
```

### Finding kinds
| Finding | What it means |
|---|---|
| `new` | Not seen before |
| `edited` | Picture or video content changed, and the file date changed too: a normal edit or re-save |
| `metadataOnly` | Only metadata changed (dates, tags, GPS, the orientation flag); the picture is the same |
| `metadataOnly` + `mtimeUnchanged` | Metadata changed but the file date was kept: a tool that hides its changes; flagged |
| `rotated` | The same picture turned 90/180/270° or mirrored (`approx: true`: matched except the edge strip, typical for a lossless JPEG rotate whose size isn't a multiple of 16 pixels) |
| `moved` | The same bytes under a new path (moved or renamed), also when the metadata changed or it was rotated in the same step |
| `missing` | Known file no longer there (kept in the store; `gc` removes old entries) |
| `possiblyDamaged` | Same size and file date, different content: likely disk, copy or sync damage |
| duplicates (`dups`) | The same bytes at two or more paths |

## The log file

Each run appends to the log file (if you gave `--log-file`): one line per message, newest at the end.

```text
2026-10-26T03:00:01.004-07:00 INFO  unswiper_hash  run started: version=0.7.0 command=verify root=Pictures store=~/Library/Application Support/Unswiper/hashes.sqlite level=info
2026-10-26T03:12:44.310-07:00 INFO  unswiper_hash::verify  metadata only: 2024/IMG_20240612_133241.jpg
2026-10-26T03:21:09.872-07:00 WARN  unswiper_hash::verify  possibly damaged: 2019/IMG_1234.JPG (size and mtime unchanged, content hash differs; last good 2026-10-19)
2026-10-26T03:30:31.517-07:00 INFO  unswiper_hash  run finished: command=verify files=48255 read=48255 GB=212.4 seconds=1830 metadataOnly=1 possiblyDamaged=1 exit=1
```

Each line: time (local, with offset), level (`ERROR`, `WARN`, `INFO`, `DEBUG`, `TRACE`), module, message. The last line of every run is the **`run finished` summary**, with counts and the exit code. It's written even when a run fails or is stopped ("interrupted, resumable").

Paths in the log are relative to the library folder (no home folder). File contents, hash values (except at `debug`) and EXIF values like dates and GPS are never logged. Use `--log-redact-paths` before sharing a log.

### Reading it
```console
$ tail -n 20 hash.log                       # the latest run
$ grep "run finished" hash.log | tail -5    # one line per run: did anything need attention?
$ grep -E " (WARN|ERROR) " hash.log         # only problems
$ tail -f hash.log                          # watch a run live
```
- **Mac:** files in `~/Library/Logs/Unswiper/` appear in **Console.app** under *Log Reports*; `open -a Console ~/Library/Logs/Unswiper/hash.log`. A job's stderr goes to the `StandardErrorPath` file from the plist. (`log show` is for the system log; unswiper-hash writes to files, not the unified log.)
- **Linux:** with systemd, `journalctl --user -u unswiper-hash-verify --since "7 days ago"` (add `-p warning` for problems only); with `--log-file`, `less +G ~/.local/state/unswiper/hash.log`.

### Levels
| Level | Shows |
|---|---|
| `error` | The run could not do its job (store error, root missing) |
| `warn` (default on screen) | Possibly damaged files, unreadable files or folders (permission denied) |
| `info` (default in the log file) | Run start and summary, and each finding: edited, metadata only, rotated, moved, missing |
| `debug` | Each file as it is hashed, hash values, timings |
| `trace` | Everything, for bug reports |

Set with `--log-level`, `-v` / `-vv` (more), `-q` (errors only), or `UNSWIPER_HASH_LOG=info` (the same syntax as `RUST_LOG`, e.g. `warn,unswiper_hash::verify=debug`). Python names `warning` and `critical` also work.

### Rotating the log
Built in: `--log-max-size 10M --log-keep 5` (keeps `hash.log.1` … `hash.log.5`). Or use the system tool:
- **Mac (newsyslog):** `/etc/newsyslog.d/unswiper-hash.conf`:
  `/Users/rob/Library/Logs/Unswiper/hash.log  rob:staff  600  5  10240  *  N`
- **Linux (logrotate):** run it from a user cron job. `~/.config/logrotate/unswiper-hash`:
  `~/.local/state/unswiper/hash.log { size 10M  rotate 5  compress  missingok  notifempty }`
  plus `logrotate -s ~/.local/state/logrotate.status ~/.config/logrotate/unswiper-hash` from cron.

The log file is opened fresh on each run, so neither tool needs to signal anything.

## What to do about each finding

- **Possibly damaged:** don't open and re-save the file. Check it against a backup: `unswiper-hash diff ~/Pictures /Volumes/Backup/Pictures` (Mac) or `… /media/rob/Backup/Pictures` (Linux). If the backup copy matches the last good hash, copy it back. If you have no good copy, open the photo to see whether it still displays. Then check the disk: Disk Utility ▸ First Aid (Mac), or `smartctl -a` and the file system's check (Linux). `verify --out damaged.txt` writes the list for a restore script.
- **Metadata only:** usually harmless: a tool rewrote dates, tags or GPS (some Windows photo tools do this when renaming or re-dating). If the dates are now wrong, restore the file from a backup, or fix the dates with exiftool. If it's flagged with "file date unchanged", find out which tool did it.
- **Rotated:** someone or some app turned the picture. Nothing is lost. "approx" means the narrow edge strip differs, which is normal for lossless JPEG rotations of sizes that aren't multiples of 16 pixels.
- **Edited:** normal after editing. If you didn't edit it, check which app touched it (sync tools sometimes re-encode).
- **Moved:** normal after reorganising. With app integration (M5), Unswiper carries your decisions over.
- **Missing:** deleted, or on a drive that isn't connected. If the drive isn't connected, the run stops with exit 3 ("root not found"), not a flood of "missing". After a deliberate clean-up, `unswiper-hash gc ~/Pictures --older-than 30` removes old entries.
- **Duplicates:** `dups` lists exact copies, largest first. unswiper-hash never deletes; remove copies yourself, or review them in Unswiper.

## Mac and Linux differences at a glance

| Topic | Mac | Linux |
|---|---|---|
| Install | Inside `Unswiper.app/Contents/MacOS/`; link into `/usr/local/bin` | Flatpak `flatpak run --command=unswiper-hash io.github.rranslam.Unswiper`; AppImage (extract); `cargo install` |
| Store | `~/Library/Application Support/Unswiper/` | `$XDG_DATA_HOME/unswiper/` (`~/.local/share/unswiper/`) |
| Logs (suggested) | `~/Library/Logs/Unswiper/` | `$XDG_STATE_HOME/unswiper/` (`~/.local/state/unswiper/`) |
| Schedule | launchd agent (`launchctl bootstrap`), runs after wake | systemd user timer (`Persistent=true`, journalctl) or cron |
| Permissions | Full Disk Access for background jobs (Pictures, external drives) | Normal file permissions; Flatpak needs `--filesystem` |
| View logs | Console.app, `tail -f` | `journalctl --user -u …`, `less`, `tail -f` |
| Rotate logs | newsyslog, or `--log-max-size` | logrotate, or `--log-max-size` |
| SHA-256 check | `shasum -a 256 -c SHA256SUMS` | `sha256sum -c SHA256SUMS` |
| External drives | `/Volumes/<name>` | `/media/<user>/<name>`, `/run/media/<user>/<name>` |
| Case sensitivity | APFS is case-insensitive by default: `IMG_1.JPG` and `img_1.jpg` are the same file; a case-only rename is reported as moved | ext4 is case-sensitive: they are two files. Comparing a Mac and a Linux copy, `diff` warns about names that differ only in case |

Windows: later.

## See also
`man unswiper-hash`, `unswiper-hash <command> --help`, [DESIGN.md](DESIGN.md) (how it works and why), and the main [Unswiper README](../../README.md).
