# visit-media-reduce

Local **desktop** app (macOS or Linux) to pare down photos and videos by **visit / place / time groups** — not unrelated left/right swipes. Same group-first reduce concept; mobile (Android / iPhone) later.

**Status:** Design docs only (no app code yet). Refactored 2026-10-11 to desktop-first + dual media sources.

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
├── config/examples/          ← sample config for Google + local tree
├── docs/
│   ├── architecture.md
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

- Implement shared domain core + desktop shell skeleton, or  
- Wire `LocalTreeSource` indexer first (fastest path to reduce UX), or  
- Complete Google Cloud OAuth desktop client setup using `docs/sources-and-setup.md`
