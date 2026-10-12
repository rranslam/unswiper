# visit-media-reduce

Local **desktop** app (macOS or Linux) to pare down photos and videos by **visit / place / time groups** — not unrelated left/right swipes. Same group-first reduce concept; mobile (Android / iPhone) later.

**Status:** macOS SwiftUI MVP in [`apps/macos`](apps/macos) (local tree source). Review fixes P0/P1/P2 applied 2026-10-11 — see [`docs/review-fixes-2026-10-11.md`](docs/review-fixes-2026-10-11.md). Linux, Google source, and mobile not started.

## Build & run (macOS)

Requires full **Xcode** (not just Command Line Tools) with the license accepted.

```
cd apps/macos
swift build
swift run
```

Try it on a **copy** of a small photo folder first. Re-index after upgrading (new index fields, stable visit IDs).

## MVP features

- Pick a local folder; background indexer (EXIF dates for photos, `AVAsset` creation date for videos)
- Visit groups by time gap, sorted by size; "Undated" group for files with no usable date
- Reduce screen with per-visit keep target, keep/reject, side-by-side compare
- Commit rejects to Trash or a `_Rejected/` quarantine folder, or dry run (nothing moves)
- Safety: files validated (inside library, unchanged size/mtime) before moving; confirm dialog; Live Photo + sidecar pairing; per-commit manifest with **Restore Last Commit**
- Commit/restore off the main thread; debounced saves

Formerly: `android-visit-media-reduce` (stub left at that path).

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
visit-media-reduce/
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
