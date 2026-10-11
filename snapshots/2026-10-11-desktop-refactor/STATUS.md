# Status — 2026-10-11 (desktop refactor)

## State

| Item | Value |
|------|--------|
| Phase | MVP **design** — desktop-first refactor |
| Code | None — docs + example config only |
| Primary targets | **macOS** and **Linux** local install |
| Later | Android, iPhone (shared domain; thin clients) |
| Media sources | **Local tree** *or* **Google API** (OAuth desktop) |
| Concept (unchanged) | Group-first visit reduce, not global swipe |

## Done

1. Android-era group-first architecture + UX (snapshotted).
2. **Refactor goal** → desktop Mac/Linux local app; mobile later.
3. Dual source model: Google API configuration **or** local photo/video tree.
4. Cross-platform architecture + desktop UX updates + setup docs.

## Differentiator (unchanged)

Cleaning runs inside a **scoped reduce session** on one visit/place group, with keep targets and time to compare.

## Not started

- App binary / UI shell
- Local tree indexer implementation
- Google OAuth + Photos/Drive client implementation
- Shared domain library package
- Mobile clients

## Resume checklist

- [ ] Re-read `SESSION.md` (grouping intent + this desktop refactor turn)
- [ ] Choose default MVP source: **local tree** recommended; Google as optional adapter
- [ ] Pick desktop shell (see architecture: Tauri or Compose Multiplatform preferred)
- [ ] Follow `docs/sources-and-setup.md` for config paths and Google Cloud steps
- [ ] Open `/Users/rranslam/Projects/visit-media-reduce` as workspace when coding

## Defaults (current)

| Topic | Choice |
|-------|--------|
| Platforms MVP | macOS + Linux desktop, runs locally |
| Mobile | Later (Android / iPhone) |
| Sources | `local_tree` and/or `google` via config |
| Commit (local) | Move rejects to trash / `_Rejected` folder; never silent nuke |
| Commit (Google) | Only where API permits; otherwise export decision list / open in browser |
| Privacy | Local processing; Google tokens stay on machine; no third-party upload of media |
| Home UX | Visit **groups** list (sort by size) |
| Grouping | Time-gap visits; geo when present; folder hints as fallback |
