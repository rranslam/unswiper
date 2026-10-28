# Status — Unswiper — 2026-10-27 (v0.6.2)

**Current direction (27 Oct 2026):** v0.6.2 released: blur score in the viewer and editable cutoffs, Mac side-by-side window, shortcut and About fixes. v0.6.1: large libraries stay responsive on macOS and Linux (background indexing and blur check with progress, virtual grid). v0.6.0: grouping by place on macOS and Linux (By Visit (Time), By Place, By Visit and Place), offline place names, date fallbacks, visits oldest first. Next: **v0.7 `unswiper-hash`**, the separate file-hashing CLI (design approved, `crates/unswiper-hash/DESIGN.md` on branch `design/unswiper-hash`). Later: a map view (plan §21). The Windows port is deferred. History below.

---
## Earlier status — 2026-10-11 (macOS MVP + review fixes)

*Formerly visit-media-reduce.* MIT licensed.

## State

| Item | Value |
|------|--------|
| Phase | **macOS MVP** built; review fixes P0/P1/P2 applied |
| Code | SwiftUI app **Unswiper** in `apps/macos` (`swift build` / `swift run`, needs Xcode) |
| Packaging | `apps/macos/scripts/build-dmg.sh` → `Unswiper.app` + `Unswiper-<ver>.dmg` (universal, ad-hoc signed, not notarized); tag `v*` → GitHub Release via `.github/workflows/release.yml` |
| Primary targets | **macOS** and **Linux** local install |
| Later | Android, iPhone (shared domain; thin clients) |
| Media sources | **Local tree** *or* **Google API** (OAuth desktop) |
| Concept (unchanged) | Group-first visit reduce, not global swipe |

## Done

1. Android-era group-first architecture + UX (snapshotted).
2. **Refactor goal** → desktop Mac/Linux local app; mobile later.
3. Dual source model: Google API configuration **or** local photo/video tree.
4. Cross-platform architecture + desktop UX updates + setup docs.
5. **macOS SwiftUI MVP** (`apps/macos`): local folder indexer, visit groups list, reduce + compare, commit to Trash / quarantine / dry run.
6. **P0 safety fixes:** validate before move (inside root, unchanged size/mtime), commit manifest + Restore Last Commit, confirm dialog (no keyboard-only commit), Live Photo + sidecar pairing.
7. **P1 fixes:** video capture dates via `AVAsset`, "Undated" group, commit/restore off main thread, debounced saves.
8. **P2 cleanup:** in-place decision updates, balanced security-scoped access, stable visit IDs, per-visit keep targets, honest dry-run reporting.

## Differentiator (unchanged)

Cleaning runs inside a **scoped reduce session** on one visit/place group, with keep targets and time to compare.

## Not started

- Automated tests
- Linux shell
- Geo-aware grouping
- Google OAuth + Photos/Drive client implementation
- Shared domain library package
- Mobile clients

## Resume checklist

- [ ] `cd apps/macos && swift build && swift run`; re-index once after upgrading
- [ ] Test commit + Restore Last Commit on a **copied** folder before the real library
- [ ] Add tests for `CommitService` validation/restore and `VisitGrouper`
- [ ] Decide Linux path (shared core vs separate shell; see architecture)
- [ ] Follow `docs/sources-and-setup.md` when adding the Google source

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
