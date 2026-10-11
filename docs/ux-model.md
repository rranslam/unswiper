# MVP UX model — desktop group-first reduce

**Status:** Current (desktop Mac/Linux)  
**Phone UX** from the Android-era draft remains a future shell; see snapshot `2026-10-11` / `architecture-android-mvp.md`.

Default path: **choose/configure source → see groups → open one → reduce with context → commit**.

## Navigation

```
Setup (first run) / Settings
Home (Groups)
  ├─ Group detail / Reduce session
  │    ├─ Compare (2–4 panes)
  │    └─ Commit summary
  └─ Source indicator (Local · Google · Both)
```

---

## 0. Setup (first run)

**Job:** Connect a library without burying it in advanced prefs.

1. **Source:** Folder / Google / Both  
2. Local → folder picker (multi-root optional)  
3. Google → Sign in with Google → show email + “read-only” / “can delete” capability badge  
4. Defaults: keep target, commit mode (trash vs quarantine)  
5. **Start indexing**

Re-open anytime under Settings → Sources.

---

## 1. Home — Groups

**Job:** Pick a visit pile worth cleaning.

- Library summary + **source badge**  
- Visit groups, default sort: **largest first**  
- Row: title (place or date + folder), counts, size, date range  
- In-progress badge: `3 kept · 12 rejected · 35 left`  
- Sort: Size | Newest | Most items  
- Filter: Photos | Videos | Source (when both enabled)

**Empty states**

- No source configured → CTA to Setup  
- Index running → progress  
- Google connected but read-limited → info banner (decisions still local)

**Desktop extras:** denser list, resizable sidebar, Reveal library roots.

---

## 2. Reduce session

**Job:** Keep a sensible set from *this* visit.

**Header:** title, dates, `50 items · 1.6 GB`, keep target + counters, source.

**Main:** thumbnail grid; chips All | Photos | Videos; sort Time | Size | Type | Path.

**Item states:** undecided | Keep | Reject; video duration; local path tooltip.

**Actions:** Keep | Reject | Compare (2–4) | Review & commit  

**Keyboard (MVP)**

| Key | Action |
|-----|--------|
| Arrows / j k | Move focus |
| Space | Toggle select |
| K | Keep selection |
| R | Reject selection |
| C | Compare |
| ⌘/Ctrl+Enter | Review & commit |

**Keep target:** editable; nudge at quota; never auto-reject remainder.

**Persistence:** auto-save decisions per group/session.

---

## 3. Compare

2–4 panes side-by-side (desktop strength). Metadata: time, size, dimensions/duration, path or Google link. Per-pane Keep / Reject / Unmark → Done back to grid.

---

## 4. Commit summary

**Local source**

- `Move 40 items to Trash` or `Quarantine (~1.2 GB)`  
- Warn on undecided  
- Primary confirm · Cancel · dry-run note if enabled  

**Google source**

- If `canDelete`: best-effort API commit + failures list  
- If not: **Save decisions** + **Export reject list** + **Open selected in browser**  
- Never pretend cloud delete succeeded when it didn’t  

---

## 5. Settings

- Sources (local roots, Google connect/disconnect)  
- Default keep target  
- Visit gap (advanced)  
- Commit mode: trash | quarantine | dry_run  
- Thumb cache location / clear cache  
- Theme: system  

---

## Flows

**Local happy path:** Setup folder → index → open largest visit → keep 10 → quarantine rejects → done  

**Google happy path:** OAuth → index → reduce → export/commit per capabilities  

**Resume:** Home shows partial progress → reopen marks  

**No geo:** date + folder/album title; same reduce UX  

---

## MVP out of scope

- Global swipe deck  
- Auto-delete  
- Merged cross-source groups (keep separate)  
- iPhone/Android shells  
- Fancy AI “best photo”  

## Screen checklist

| Screen | Priority |
|--------|----------|
| Setup / sources | P0 |
| Permission/error for unreadable roots | P0 |
| Home groups | P0 |
| Reduce grid + keep target | P0 |
| Commit (local trash/quarantine) | P0 |
| Compare multi-pane | P1 |
| Google OAuth + capability-aware commit | P1 |
| Settings | P1 |
| Keyboard map complete | P1 |
