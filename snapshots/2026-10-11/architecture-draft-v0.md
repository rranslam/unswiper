# Architecture draft v0 — flat scan / rank / review (superseded)

**Status:** Historical. Kept for restart context only.  
**Superseded by:** `architecture.md` (group-first staged MVP).

## Goal

Help users reclaim storage by finding low-value media and deleting (or archiving) it safely — mostly on-device, with a fast review loop.

## Product shape

**Native Android app** (Kotlin + Jetpack). Avoid cloud upload by default.

Core loop:

1. Scan library (MediaStore)  
2. Rank candidates (size, type, similarity, age, screenshots)  
3. Review in batches (keep / delete / skip)  
4. Delete via MediaStore trash when possible  

## Layers

```
UI (Compose) → Domain (ScanSession, Candidate, Decision) → Data (MediaStore, Room, WorkManager)
```

## Modules

| Module | Responsibility |
|--------|----------------|
| `media-index` | Query images/videos metadata |
| `signals` | Screenshots, large files, short clips, old/burst |
| `similarity` | Perceptual hash / near-duplicates |
| `ranker` | Score → review queue |
| `review` | Persist keep/delete/skip |
| `cleanup` | Trash; report freed bytes |

## v1 detectors (this draft)

1. Largest files (videos first)  
2. Screenshots / screen recordings  
3. Exact duplicates  
4. Near-duplicates (conservative)  

## Why superseded

User feedback: existing left/right tools decide on **unrelated** items. Product must **group first** by type/location/time (visits), then reduce inside the group with context (e.g. keep top 10 of 50 on a Mt Hood trip). See `architecture.md`.
