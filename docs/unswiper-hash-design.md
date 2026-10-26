# unswiper-hash: design (draft, 26 Oct 2026)

Status: **design for review, not scheduled, nothing built.** It implements the approach Rob approved on 15 Oct 2026 in [plan §18.9](linux-port-plan.md#189-decision-rob-15-oct-2026): a separate file-hashing CLI with its own hash store. It recognises moved files, works across machines and adds no files to photo folders. Estimate: 4–5 days (M1–M4); app integration is extra (M5). The numbered **open choices** for Rob are in §12.

## 1. Goals and non-goals

**Goals**
- Keep a content hash for every photo and video under a library root, updated incrementally: only new files and files whose size or mtime changed are read.
- **Verify** on request: re-read files and report **possible corruption** (same size and mtime, different bytes), **edits** (new mtime and new bytes), **missing** and **new** files.
- Recognise **moves and renames** (same bytes, new path) and report **exact duplicates** (same bytes, two or more paths).
- Work **across machines** (Mac, Linux, later Windows): compare one library on two machines, or a library with its backup, by content and root-relative path.
- **Never write into photo folders.** The store lives in the user's app-data folder. Checksum files (`B3SUMS`, `SHA256SUMS`) are written only when asked, to a path the user gives.
- Headless and scriptable: cron, launchd or systemd timers on a NAS; `--json` output; meaningful exit codes.

**Non-goals**
- No changes to the review apps in M1–M4. The apps never write hashes; library.json is unchanged.
- No near-duplicate or perceptual ("looks the same") matching. Exact bytes only.
- No repair or parity (that's par2, snapraid, ZFS or Btrfs). No moving or deleting files: unswiper-hash only reads and reports.
- No sync service. Cross-machine work uses manifests the user copies, or a store on a shared disk.
- No Windows build yet (deferred), but the design keeps it possible: `/` in stored paths, no Unix-only APIs in the store.

## 2. Plain-language summary

`unswiper-hash scan ~/Pictures` reads each photo and video once and remembers a fingerprint of its bytes in a small database in Unswiper's app-data folder. Later scans only read files that are new or look changed, so they are quick. `unswiper-hash verify` re-reads everything (slowly, resumable) and tells you if a file's bytes changed although nothing should have touched it: that is possible disk or copy damage. Because it knows the fingerprints, it can tell "this file moved from A to B" apart from "A was deleted and B is new", and list exact duplicates. To compare two machines or a backup, export a manifest on one and `diff` it on the other.

## 3. CLI

`unswiper-hash <command> [options]`. Every command takes `--store <file>` (default: §5), `--json` (machine-readable output on stdout, progress on stderr) and `-q/--quiet`.

| Command | What it does |
|---|---|
| `scan <root>` | Walk `<root>` (media files only, same extension list as unswiper-core). Hash new files and files whose size or mtime changed; record moves, edits, new and missing. Never reads unchanged files. |
| `verify <root>` | Re-read **every** known file and compare. Reports `damaged?` (size and mtime same, hash different), `edited`, `missing`. Resumable: progress is saved, `--restart` starts over. Never updates a stored hash for a `damaged?` file. |
| `status <root>` | No reading of file contents: counts of known, new, missing and changed (by size/mtime), plus the last scan and verify times. |
| `moves <root>` | Moves and renames found by the last scans (old path → new path). |
| `dups <root>` | Groups of exact duplicates (same hash, ≥ 2 live paths), largest first. |
| `export <root> --out <file>` | Portable manifest (§7). `--format b3sums` or `sha256sums` writes a standard checksum file instead (`b3sum -c` / `sha256sum -c` / `shasum -a 256 -c` compatible); SHA-256 is computed on the fly when not stored. |
| `diff <a> <b>` | Compare two manifests, or a manifest and a root: same, only in A, only in B, moved, different content at one path. |
| `forget <root>` | Remove a root from the store. `gc` drops entries of files missing for more than `--older-than` days (default: never automatic). |

Common options: `--jobs N` (default `min(4, cores)`; use 1 for spinning disks), `--exclude <glob>` (repeatable), `--include-hidden`, `--no-follow-symlinks` (default; symlinks are not followed), `--low-priority` (nice/ionice on Linux, `taskpolicy -b` style background QoS on the Mac), `--limit-mb-s N` for throttling.

Exit codes: 0 all good; 1 differences found that need attention (`damaged?` files, or diff not equal); 2 usage error; 3 I/O or store error. `edited`, `new`, `moved` alone exit 0.

## 4. Hash algorithm

| | BLAKE3 | xxh3-128 | SHA-256 |
|---|---|---|---|
| Kind | Cryptographic | Non-cryptographic | Cryptographic |
| Speed, one core | ~1–3 GB/s (SIMD), multi-threaded per file | ~10+ GB/s | ~0.5–2 GB/s (with SHA extensions) |
| Collision safety | 256-bit, no known attacks | Fine by chance, but craftable collisions | 256-bit |
| Verify without our tool | `b3sum` (separate install) | `xxhsum` (separate install) | Built in: `sha256sum`, `shasum -a 256`, `Get-FileHash` |
| Rust crate, licence | `blake3` 1.8, CC0-1.0 / Apache-2.0 | `xxhash-rust`, BSL-1.0 | `sha2` 0.11, MIT / Apache-2.0 |

**Recommendation: BLAKE3 stored, SHA-256 on export only.** Reading a photo library is limited by the disk (100–500 MB/s for HDDs, NAS and USB; a few GB/s for NVMe), so xxh3's extra CPU speed buys almost nothing. Its weaker guarantee does matter here, though: content identity across machines and backups (moves, duplicates, diff) should not be fooled by crafted or unlucky collisions. BLAKE3 is as fast as the disk, pure Rust, and has a clear licence. SHA-256 stays available for archives that must be checkable with built-in OS tools. Every stored hash is tagged (`"b3:<64 hex>"`), so a later switch keeps old entries readable.

Whole-file bytes are hashed: an edit to EXIF metadata counts as a change. A "pixel-only" hash is out of scope.

## 5. Store: format and location

**Format (recommended): SQLite** via `rusqlite` with the bundled SQLite. One file, WAL journal (crash-safe, a scan killed half way loses nothing already written), indexes on both path and hash, and it handles 100,000+ files without loading everything into memory. Readable with any `sqlite3` tool.

Tables (schema version in `PRAGMA user_version`, starting at 1):
- `roots(id, label, path_hint, created_at)`: a root has a stable random **library ID**, also written to the store, not to the photo folder. The label defaults to the folder name; `path_hint` is the last absolute path on this machine.
- `files(root_id, rel_path, size, mtime_ns, hash, first_seen, last_seen, last_verified, state)` with `state` ∈ live, missing, damaged?. `rel_path` is root-relative with `/`, Unicode NFC (macOS file names are NFD).
- `events(root_id, at, kind, rel_path, other_path, hash)`: new, edited, moved, missing, damaged?, verified-ok, kept for `moves` and history.
- `scans(root_id, kind, started_at, finished_at, resume_cursor)`.

**Alternative:** one JSON file (no new C dependency; serde_json is already used), rewritten atomically. That's simple, but it is slow and memory-heavy past ~50k files, and a killed scan loses all its work unless it is checkpointed. See open choice 2.

**Location** (an app-data folder, never the photo folder):
- macOS: `~/Library/Application Support/Unswiper/hashes.sqlite`, next to the Mac app's support folder.
- Linux: `$XDG_DATA_HOME/unswiper/hashes.sqlite` (default `~/.local/share/unswiper/`). The Flatpak app would see it as `~/.var/app/io.github.rranslam.Unswiper/data/unswiper/`; the CLI is not sandboxed, so if the app ever reads the store (M5) the path is chosen with `--store` or a symlink. That's an open choice for M5, not M1.
- Windows (later): `%APPDATA%\Unswiper\hashes.sqlite`.
- `--store <file>` or `UNSWIPER_HASH_STORE` overrides it, e.g. a store on a NAS next to the photos' share (but not inside it).

## 6. Moves, renames, edits and duplicates

On each `scan`, for each root:
1. Walk the files. A file with a known path and the same size and mtime is **unchanged**: not read.
2. Known path with a different size or mtime: read and hash. Different hash: **edited**. Same hash: just touched (mtime updated, no event).
3. Unknown path: read and hash. If a known file with the **same hash and size** is now missing, it's a **move** (old → new path): the entry keeps its history and first-seen date. If several missing files match, pair by file name, then by folder, then report as a duplicate move.
4. Known paths not seen: **missing** (kept, not deleted; `gc` removes them on request).

`verify` re-reads all live files: same size and mtime but a different hash → **damaged?**. The stored good hash is kept so a restored backup copy can be checked against it. **Duplicates** are files with one hash at two or more live paths. They are reported only; nothing is changed.

Case-insensitive file systems (APFS, NTFS by default): a rename that only changes case is matched as a move by hash, so it needs no special rule.

## 7. Across machines

Stored paths are root-relative, and the library ID ties a root together across machines, so:
- **Manifest export/import:** `export` writes `unswiper-hashes-<label>-<date>.json` (`{format: 1, algo: "b3", library_id, label, created_at, files: [{path, size, hash}]}`, about 120 bytes per file, ~12 MB for 100k files; optionally gzip). Copy it to the other machine and run `diff manifest.json ~/Pictures` there. `scan --import manifest.json` seeds a new machine's store, so its first `verify` is a full comparison instead of a first-time scan.
- **Backups:** `diff ~/Pictures /Volumes/Backup/Pictures` (two roots) or against a manifest kept with the backup.
- **One store on a shared disk** works too (`--store`), but SQLite on network file systems (SMB, NFS) is not reliable with two writers. The docs will say: one writer at a time, or use manifests.
- Mtimes are not trusted across machines (sync tools rewrite them); content hashes decide.

## 8. How the Unswiper apps use it

- **M1–M4: not at all.** It ships as a separate CLI in the same repo and releases. The apps and library.json are unchanged, as approved in §18.9.
- **M5 (optional, later, needs Rob's go):** the apps **read** the store with the `unswiper-hash` library crate (Linux, Rust) or `unswiper-hash status --json` (Mac, bundled like the blur helper), to carry Keep/Reject decisions and blur scores across a detected move, and to show a "possibly damaged" badge. A "Verify Library…" menu item would run the CLI. Still no hashes in library.json, except at most one optional `contentHash` reference per item if carry-over needs it (open choice 6).

## 9. Crate layout

```
crates/unswiper-hash/          ← new; lib + bin "unswiper-hash"
  src/lib.rs                   ← public API: Store, scan, verify, diff, export
  src/store.rs                 ← SQLite schema and migrations
  src/walk.rs                  ← walks a root (reuses unswiper-core's media-extension list and NFC paths)
  src/hash.rs                  ← BLAKE3 streaming (64 KiB buffers; mmap + rayon for files > 64 MiB), SHA-256 for export
  src/manifest.rs              ← JSON manifest and B3SUMS / SHA256SUMS
  src/main.rs                  ← CLI (same hand-written argument style as unswiper-cli)
  tests/                       ← integration tests on temp folders
```
Dependencies: `unswiper-core` (media types, path normalising), `blake3`, `rusqlite` (`bundled`), `sha2`, plus the existing `serde`, `serde_json`, `chrono`. No async runtime. A worker pool uses `std::thread` (or `rayon` if blake3's `rayon` feature is used for big files).

Packaging: added to the AppImage and Flatpak as `unswiper-hash` (next to `unswiper-cli`) and to the Mac dmg as a command-line tool inside the app bundle (`Contents/MacOS/unswiper-hash`, with a Help note on adding it to PATH). Flatpak `cargo-sources.json` is regenerated.

## 10. MSRV and licences

- **MSRV:** checked on 26 Oct 2026. `blake3` 1.8.7, `rusqlite` 0.41.0 (`rust-version` 1.89; bundled `libsqlite3-sys` 0.39.1) and `sha2` 0.11.1 build with **Rust 1.92** (v0.6) and **1.89** (main) in a scratch crate. Bundled SQLite needs a C compiler (`cc`), which the CI images and Flatpak SDK have.
- **THIRD_PARTY_NOTICES** additions: BLAKE3 (CC0-1.0 or Apache-2.0; we take Apache-2.0), SQLite (public domain; the bundled amalgamation), rusqlite and libsqlite3-sys (MIT), sha2 / digest and their RustCrypto helpers (MIT or Apache-2.0), arrayvec, constant_time_eq, hashlink, hashbrown, fallible-iterator (MIT or Apache-2.0). `cargo about` / `cargo deny` keeps the list honest; nothing GPL.

## 11. Testing

- Unit: official BLAKE3 and SHA-256 test vectors; streaming vs one-shot gives the same hash; NFC path normalising; store migrations from an empty and a v1 store.
- Integration on temp folders (synthetic files only, never personal media):
  - new, edited, missing;
  - move and rename, including a rename that only changes case;
  - corruption: flip a byte and restore the mtime with `filetime` → `damaged?` and exit 1;
  - duplicate groups;
  - excluded and hidden files; symlinks not followed.
- Resume: kill `verify` midway (a test hook stops after N files) and resume; WAL crash-safety.
- Manifests: export → diff round trip; `b3sum -c` / `sha256sum -c` accept our checksum files (CI installs b3sum).
- Cross-machine: the same tree under two different absolute roots diffs as identical.
- CLI: golden `--json` outputs and exit codes.
- CI: Linux (x86_64, aarch64) and macOS jobs, `cargo +1.92` MSRV job; a timed scan of ~10k small synthetic files as a smoke benchmark.

## 12. Open choices for Rob (with recommendations)

1. **Algorithm.** BLAKE3 stored with SHA-256 on export, BLAKE3 only, SHA-256 only, or xxh3. *Recommend: BLAKE3 stored, SHA-256 export on request* (§4).
2. **Store format.** SQLite (bundled) or a single JSON file. *Recommend: SQLite.* It's crash-safe and resumable and scales to large libraries; the cost is one C dependency.
3. **Store location.** Per-user app-data folder (§5), or a store next to the library on the same disk (not inside the photo folders), so it travels with an external drive. *Recommend: app-data folder by default, `--store` for drives and NAS.*
4. **Which files.** Media files only (same list as Unswiper), or every file under the root (sidecars, `.AAE`, XMP, Takeout JSON). *Recommend: media + known sidecars (`.AAE`, `.XMP`, `.json` from Takeout), behind `--all-files` for everything.*
5. **Verify scheduling.** On request only, or a ready-made weekly timer (launchd plist / systemd timer printed by `unswiper-hash schedule --print`, never installed silently). *Recommend: on request; ship the `--print` helper in M4.*
6. **App integration (M5).** None, read-only (badge + carry decisions across moves), or read-only plus a `contentHash` field in library.json. *Recommend: read-only, no library.json field, after v0.6 ships; decide when scheduled.*
7. **Carry-over on moves (if M5).** Automatic or ask. *Recommend: automatic when exactly one missing file matches, ask when ambiguous.*
8. **Where it ships.** Inside each release (AppImage, Flatpak, dmg) or as separate downloads / `cargo install`. *Recommend: inside the release bundles plus `cargo install --path crates/unswiper-hash`.*
9. **Release timing.** With v0.6, v0.7, or on its own tag. *Recommend: after v0.6 is released, as part of v0.7, before the Windows port.*
10. **`damaged?` handling.** Report only, or also write a list for restore from backup (`--out damaged.txt`). *Recommend: report plus optional list; never touch the file.*

## 13. Milestones (4–5 days)

| Milestone | Scope | Estimate |
|---|---|---|
| **M1** Store and scan | Crate, SQLite schema v1, walk + incremental BLAKE3 scan, `status`, `--json`, exit codes, store locations, unit tests | 1.5 days |
| **M2** Verify, moves, duplicates | `verify` (resumable, low priority, throttle), `damaged?`, move and rename matching, `moves`, `dups`, `forget`/`gc`, integration tests | 1–1.5 days |
| **M3** Across machines | Manifests, `export` (JSON, B3SUMS, SHA256SUMS), `diff` (manifest or root), `scan --import`, round-trip tests | 1 day |
| **M4** Ship | AppImage, Flatpak (cargo-sources), dmg; THIRD_PARTY_NOTICES; README, help page and man-style `--help`; CI jobs incl. MSRV; `schedule --print` | 0.5–1 day |
| **M5** (optional, later) App integration | Read-only store access, move carry-over, "possibly damaged" badge, "Verify Library…" | 2 days |
