# Architecture — desktop-first (macOS / Linux), mobile later

**Status:** Current direction  
**Prior primary target:** Android-only MVP → see `architecture-android-mvp.md`  
**Concept preserved:** staged Index → Group → Scope → Reduce → Commit

## Goal

A **locally installed** desktop app on **macOS or Linux** that indexes photos/videos from:

1. a **local directory tree**, and/or  
2. **Google** (user OAuth via Google APIs),

then groups them by visit/place/time and lets the user **reduce with context** (e.g. keep ~10 of 50 from a Mt Hood trip).

**Later:** Android and iPhone apps reuse the same domain model and, where practical, the same source adapters (local on-device library; Google OAuth).

## Core problem (unchanged)

Swipe-style tools decide on unrelated items. This product: **group first → reduce inside the group → commit.**

## Platform strategy

```
┌──────────────────────────────────────────────────────────┐
│  Shells (thin)                                           │
│  Desktop: macOS + Linux (MVP)                            │
│  Later: Android · iPhone                                 │
└────────────────────────┬─────────────────────────────────┘
                         │
┌────────────────────────▼─────────────────────────────────┐
│  Shared domain (platform-agnostic)                       │
│  Grouper · ReduceSession · Rank/similarity · Decisions   │
└────────────────────────┬─────────────────────────────────┘
                         │
┌────────────────────────▼─────────────────────────────────┐
│  Source adapters                                         │
│  LocalTreeSource · GoogleMediaSource                     │
└────────────────────────┬─────────────────────────────────┘
                         │
┌────────────────────────▼─────────────────────────────────┐
│  Local persistence                                       │
│  SQLite index · config · OAuth token store · session DB  │
└──────────────────────────────────────────────────────────┘
```

### Why desktop first

- Large libraries and dual-pane compare fit desktop better than phone swipe.  
- Local trees ( incl. Takeout / synced folders) are easy to point at.  
- OAuth desktop client is a standard Google Cloud app type.  
- Mobile becomes a second shell over the same core, not a rewrite of the product idea.

### Suggested implementation stack (MVP)

Pick one shell; keep domain free of UI frameworks:

| Layer | Option A (recommended) | Option B |
|-------|------------------------|----------|
| Desktop UI | **Tauri 2** (Rust + web UI) | **Compose Multiplatform** desktop |
| Domain | Rust or Kotlin shared lib | Kotlin shared (easier Android later) |
| DB | SQLite | SQLite |
| Thumbs | Local cache dir + image crate / Coil-analog | same |
| Config | `~/.config/visit-media-reduce/` (Linux) · `~/Library/Application Support/visit-media-reduce/` (macOS) | same |

**Compose Multiplatform** if Android follow-on is near-term. **Tauri** if you want a small native binary and web-skilled UI. Either way: **one domain crate/module**, two source adapters.

## Staged pipeline (unchanged stages, new I/O)

| Stage | Name | Desktop behavior |
|-------|------|------------------|
| **0** | Index | Walk local tree and/or page Google media APIs; store metadata in SQLite |
| **1** | Group | Time-gap + geo + folder hints → visit groups |
| **2** | Scope | User picks a group (later: place rollup) |
| **3** | Reduce | Grid + compare; keep target |
| **4** | Commit | Local: trash / quarantine folder. Google: API delete/archive **only if allowed**, else export actions |
| **5** | Roll up | Cross-visit place keep counts (later) |

## Source adapters

### `LocalTreeSource`

- Root path(s) from config (`sources.local_tree.roots`).  
- Recursive scan; respect include/exclude globs.  
- Read EXIF/QuickTime metadata (time, GPS, dimensions, duration).  
- Commit: move/copy rejects to system trash or configured `_Rejected/` (user choice).  
- Best MVP path: full control, predictable delete semantics.

### `GoogleMediaSource`

- OAuth 2.0 **desktop** client; tokens in OS keychain / encrypted local store.  
- Use Google APIs per `sources-and-setup.md` (Photos Library and/or Drive, depending on where media lives).  
- Index: media id, product URL, mime, size when available, created time, lat/lng if exposed, album membership.  
- Thumbnails: download-to-cache or URL fetch into local thumb cache (never re-upload user media to third parties).  
- Commit: constrained by API policy — design UI to **degrade gracefully** (mark decisions locally; offer “open in Google Photos” / export CSV of reject ids when delete isn’t available).

Both adapters implement one interface, e.g.:

```text
MediaSource
  listIncremental(cursor) -> MediaItem[]
  fetchThumbnail(id) -> path/bytes
  commitRejects(ids) -> CommitResult   // may be Unsupported
  capabilities() -> { canDelete, canTrash, readOnly }
```

## Domain concepts (unchanged + source id)

- `MediaItem` — includes `source` (`local` | `google`) + stable `sourceId` / path  
- `VisitGroup`, `PlaceRollup`, `ReduceSession`, `Decision` — same as before  
- `LibraryProfile` — which sources are enabled for this machine  

## Modules

| Module | Role |
|--------|------|
| `config` | Load/validate YAML/JSON; paths; Google client ids |
| `media-index` | Orchestrate adapters → SQLite |
| `local-tree` | Filesystem walker + metadata |
| `google-media` | OAuth + API client |
| `grouper` | Visit clustering |
| `reduce-session` | Keep/reject/compare state |
| `similarity` | In-group only (optional MVP+) |
| `cleanup` | Local trash/quarantine; Google commit bridge |
| `desktop-shell` | macOS/Linux UI |
| `mobile-shell` (later) | Android / iPhone |

## Desktop use-model updates (vs phone)

- **Windowed** group list + reduce grid (master/detail or push navigation).  
- **Keyboard:** j/k or arrows, K keep, R reject, Space select, Enter compare, ⌘/Ctrl+Enter commit.  
- **Compare:** true multi-pane (2–4) without leaving the group.  
- **External monitor / large grid** density settings.  
- **Paths visible** for local items (reveal in Finder/Files).  
- **Multiple roots** (e.g. `~/Pictures` + external drive).  
- Still **no** global Tinder deck as the default.

## Privacy & trust

- Runs locally; index and decisions stay on disk.  
- Google: only OAuth to Google; media bytes cached locally for thumbs/preview as needed.  
- No analytics upload of photo contents.  
- Config and secrets never committed to git (see example configs with placeholders).

## Risks

| Risk | Mitigation |
|------|------------|
| Google Photos API library/delete limits | Local tree as default commit path; Google adapter capability flags |
| Huge trees | Incremental index, ignore lists, hash only in-group |
| Dual-source duplicates | Optional content-hash link across sources later |
| Cross-platform path/EXIF quirks | Abstract metadata reader; CI on macOS + Linux |

## Build order (desktop MVP)

1. Config + `LocalTreeSource` index + visit groups + reduce grid + local trash/quarantine  
2. Desktop polish: keyboard, compare panes, multi-root  
3. Google OAuth + index (read) + decision export / best-effort commit  
4. Place rollups + in-group similarity  
5. Shared core packaging → Android / iPhone shells  

## Bottom line

Same **group-first reduce** product; **desktop Mac/Linux** is the MVP runtime; **local tree and Google** are pluggable sources; phones come later as additional shells.
