# Architecture — Android-first MVP (historical)

**Status:** Superseded as primary target (2026-10-11). Group-first concept retained.  
**Current direction:** `architecture.md` (desktop macOS/Linux + local tree / Google sources).  
**Supersedes:** `architecture-draft-v0.md`

## Goal

Help users reclaim storage by **grouping** photos/videos into recognizable visits (type / location / time), then **reducing inside one group** with context — e.g. keep ~10 of 50 from a Mt Hood trip — not deciding on unrelated left/right swipes.

## Core problem

Existing swipe tools force keep/delete on a flat stream where consecutive items are often unrelated. Without trip/place/time context, decisions are guesswork. This product: **form a meaningful group → reduce that group → move on.**

## Product shape

**Native Android app** (Kotlin + Jetpack Compose). On-device by default. No cloud photo upload for MVP.

## Staged approach

| Stage | Name | Outcome |
|-------|------|---------|
| **0** | Index | MediaStore catalog (type, time, size, folder/bucket, lat/lng if present) |
| **1** | Group | Visit/place/time clusters (“Mt Hood · May 2024 · 50 items”) |
| **2** | Scope | User picks one group (later: place rollup across years) |
| **3** | Reduce | Compare inside the group; keep target e.g. ~10 of 50 |
| **4** | Commit | Trash rejects; bytes freed; undo window |
| **5** | Roll up (later) | Cross-visit: “already kept 40 Mt Hood — keep 5 more?” |

Stage 1 is the differentiator. Stages 3–4 only run **inside** a chosen group.

## Interaction model

**Browse groups → open group → multi-select / compare → commit**

- Group list sorted by impact (size, count, later “messy”).
- Inside group: grid + optional side-by-side compare (2–4).
- Optional within-group swipe only for near-dupes that already share context.
- Explicit keep target with live counters.

## Grouping model

```
Place / theme rollup     e.g. Mt Hood (geo + album/folder hints)
  └─ Visit               time gap cluster (e.g. gap > 36h = new visit)
       └─ Burst / scene  tight time + similarity (optional reduce aid)
```

**Signals (v1):**

1. **Time** — primary; gaps define visits  
2. **Location** — EXIF/MediaStore lat-lng; place + radius  
3. **Type** — photos vs videos as filters/sub-tabs inside a visit  
4. **Storage bucket / path** — Camera vs Screenshots vs WhatsApp (separators)

**Example:** “Mt Hood · Oregon · May 12–14 2024 · 42 photos · 8 videos · 1.6 GB”

## Pipeline

```
Index ──► Grouper ──► GroupStore ──► Group Browser (UI)
                         │
                         ▼
              ReduceSession (one group)
                         │
              Rank / similarity (optional, in-group only)
                         │
                         ▼
              Decisions ──► Trash / Commit
```

## Domain concepts

- `MediaItem` — single file  
- `VisitGroup` — place + time window + members  
- `PlaceRollup` — many visits at same place (later)  
- `ReduceSession` — active group, keepTarget, decisions  
- `Decision` — keep / reject / undecided (never auto-reject outside session)

## Modules

| Module | Role |
|--------|------|
| `media-index` | Stage 0 |
| `grouper` | Stage 1: time-gap + geo; naming |
| `group-browser` | Stage 2 UI |
| `reduce-session` | Stages 3–4 |
| `similarity` | Inside a group only |
| `cleanup` | Trash + freed bytes |

Flat global queues (largest file, all screenshots) may be escape hatches later; **default home is Groups**.

## Critical design choices

- **Privacy:** Index/hash on-device; no upload.  
- **Permissions:** `READ_MEDIA_IMAGES` / `READ_MEDIA_VIDEO`; handle partial access.  
- **Deletion:** MediaStore trash; commit summary; no silent batch hard-delete.  
- **Performance:** Metadata first; hash/similarity only within groups/candidates.  
- **False positives:** Never auto-delete similars; always human confirm in session.

## Suggested tech

- Kotlin, Compose, Navigation, Hilt  
- Room + DataStore  
- WorkManager (incremental rescan)  
- Coil (thumbnails)  
- Coroutines / Flow  

## Deliberately out of MVP

- Global left/right on whole library  
- Auto-delete without group session  
- Cloud “memories” upload  
- Perfect place names without geo (fallback: “May 12–14 · Camera · 50 items”)  
- iOS  

## Build order

1. Index + **time-gap visit groups** + group list + in-group multi-select + trash  
2. Geo / place titles + rollups  
3. In-group near-dupe clustering + compare  
4. Keep targets + cross-visit keep counts  
5. Type filters / video-heavy visits polish  

## Bottom line

Grouping is stage 1 and the product; cleaning is always a **scoped reduce session** so selection has context and time to compare.
