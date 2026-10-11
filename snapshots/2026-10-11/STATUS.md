# Status — 2026-10-11 snapshot

## State

| Item | Value |
|------|--------|
| Phase | MVP **design** complete through UX model |
| Code | None — docs only |
| Decision | Group-first staged approach approved as MVP direction |
| Pause reason | User shifting goals; snapshot retained for later restart |

## Done in this planning pass

1. Initial architecture for Android photo/video paring (flat scan/rank/review).
2. **Refactor** to staged pipeline with **grouping first** (visits by time/location/type).
3. Problem framing vs existing left/right swipe tools (unrelated items).
4. MVP UX model: Home groups → Reduce session → Compare → Commit → thin Settings.

## Explicit MVP differentiator

Cleaning always happens inside a **scoped reduce session** on one visit (or later place rollup), with keep targets and time to compare — not a global swipe deck.

## Not started

- Wireframe-level component inventory
- Android repo / Gradle skeleton
- Room schema / grouper algorithm implementation
- Place rollups across years (post-MVP / stage 5)

## Resume checklist

- [ ] Re-read `SESSION.md` (especially user messages on grouping + Mt Hood example)
- [ ] Confirm MVP still = time-gap visit groups + in-group grid reduce + trash
- [ ] Choose next: components, data model + grouper, or app skeleton
- [ ] Re-open this folder as the Cursor workspace when coding starts

## Suggested defaults (locked for MVP unless changed)

- Platform: native Android, Kotlin, Jetpack Compose
- Privacy: on-device only; no photo upload
- Delete: MediaStore trash + commit summary; no silent auto-delete
- Home: visit **groups** list (sort by size)
- Primary grouping: time-gap visits; geo titles when available; fallback date + folder
