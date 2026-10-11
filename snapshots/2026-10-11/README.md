# android-visit-media-reduce

On-device Android tool to **pare down photos and videos by visit/place/time groups**, not unrelated left/right swipes.

**Status:** Design snapshot only (no app code yet). Paused 2026-10-11 while shifting goals. Safe to restart from this folder.

## Restart here

1. Read [`STATUS.md`](STATUS.md) — where we stopped and next options.
2. Skim [`SESSION.md`](SESSION.md) — full prompts and assistant replies from the planning chat.
3. Use current product docs:
   - [`docs/architecture.md`](docs/architecture.md) — staged, group-first MVP architecture
   - [`docs/ux-model.md`](docs/ux-model.md) — MVP UX screens and flows
4. Optional history: [`docs/architecture-draft-v0.md`](docs/architecture-draft-v0.md) — first flat “scan/rank/swipe” draft (superseded).

## Product one-liner

Form meaningful **visit groups** (type / location / time) → open one group → **reduce with context** (e.g. keep ~10 of 50 from a Mt Hood trip) → trash rejects safely.

## Tree

```
android-visit-media-reduce/
├── README.md                 ← you are here
├── STATUS.md                 ← pause point + resume checklist
├── SESSION.md                ← all prompts + chats (pick-up log)
├── docs/
│   ├── architecture.md       ← current MVP architecture
│   ├── ux-model.md           ← current MVP UX model
│   └── architecture-draft-v0.md
└── snapshots/
    └── 2026-10-11/           ← dated freeze of this planning pass
        └── MANIFEST.md
```

## Cursor chat reference

- Transcript id: `f5c9a60c-cfae-4433-bec3-3e9030987803`
- Local path: `~/.cursor/projects/Users-rranslam/agent-transcripts/f5c9a60c-cfae-4433-bec3-3e9030987803/`

## Suggested next steps (when resuming)

- Component inventory (list row, grid cell, bottom bar states), or
- Android project skeleton (Kotlin + Compose) matching the UX flow, or
- `VisitGroup` / `ReduceSession` data model + grouper algorithm defaults
