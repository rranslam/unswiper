# MVP UX model — group-first reduce

**Status:** Current direction (approved to proceed after architecture)  
**Default path:** see groups → open one → reduce with context → commit. No global swipe deck.

## Navigation map

```
Home (Groups)
  ├─ Group detail / Reduce session
  │    ├─ Compare (2–4 items)
  │    └─ Commit summary
  ├─ Filters / sort (sheet)
  └─ Settings (keep targets, gap, units)
```

One primary job per screen. Escape hatches (Screenshots, Huge videos) post-MVP or secondary later.

---

## 1. Home — Groups

**Job:** Pick a pile worth cleaning.

**Content**

- Library summary (secondary): e.g. `1,240 items · 38 GB`
- List of **visit groups**, default sort: **largest first**
- Each row:
  - Title: place if known, else date + source — `Mt Hood` or `May 12–14 · Camera`
  - Meta: `42 photos · 8 videos · 1.6 GB`
  - Sub: date range
  - Optional later: `Messy` chip for bursty groups

**Actions**

- Tap row → Reduce session  
- Sort: Size | Newest | Most items  
- Filter chips (optional MVP+): Photos only | Videos only | Has location  

**Empty / partial**

- No permission → CTA to grant media access  
- Partial access → banner + Manage  

**Not on Home:** thumbnail-first library, swipe cards, stats dashboards.

---

## 2. Reduce session — Group detail

**Job:** Keep a sensible set from *this* visit; compare with time.

**Header (always visible)**

- Group title + dates  
- `50 items · 1.6 GB`  
- Keep target: `Keep 10` (editable) · `Kept 3 · Reject 12 · Left 35`

**Layout**

- Grid of thumbnails (primary)  
- Chips: `All | Photos | Videos`  
- Sort: Time | Size | Type  

**Item states:** undecided | Keep | Reject | video duration badge  

**Bottom bar:** Keep | Reject | Compare (2–4 selected) | Review & delete (≥1 reject)

**Keep target**

- Default from settings (e.g. 10 or ~20% capped)  
- At target: nudge to reject rest or raise target  
- Never auto-reject remainder without confirmation  

**Selection:** tap toggle; long-press multi-select; select all undecided / clear  

**Persistence:** decisions auto-save; return to Home keeps session (“35 undecided”).

**Avoid:** forced left/right on every item; losing marks when leaving the group.

---

## 3. Compare

**Job:** Shortlist side-by-side before Keep/Reject.

- Entry: 2–4 selected → Compare  
- Phone: pager or 2-up; landscape 2×2 for 4  
- Per pane: preview, time, size, dimensions/duration; Keep | Reject | Unmark  
- Exit: Done → grid with marks  
- Only items from current group  

---

## 4. Commit summary

**Job:** Confirm trash; show impact.

- `Delete 40 items · ~1.2 GB`  
- Keep strip; warn if undecided remain  
- Primary: **Move to trash** · Secondary: Cancel  
- After: snackbar + Undo; update Home group row  

---

## 5. Settings (MVP-thin)

- Default keep target (number or %)  
- Visit gap (e.g. 36h) — advanced, preferably hidden  
- Units GB/MB · theme follow system  
- No accounts / cloud / AI toggles  

---

## Key flows

**Happy path:** Home → open Mt Hood visit → set keep 10 → Keep / Compare / Reject → Review & delete → trash → Home  

**Resume:** Home shows progress → reopen same marks  

**Video-heavy:** filter Videos then Photos  

**No geo:** title by date + folder; same reduce UX  

---

## Motion

1. Home → group push / shared title  
2. Keep/Reject brief mark on thumb  
3. Commit progress then list settle  

## Copy

- Visits and counts (“50 on this trip”), not “memories”  
- Reject / Move to trash until commit  
- “Aim to keep 10,” not “must keep 10”  

## MVP out of scope (UX)

- Global Tinder-style deck  
- Auto-delete  
- Place rollup across years  
- In-group near-dupe stacks (after grid+compare)  
- Widgets / heavy onboarding  

## Screen checklist

| Screen | Priority |
|--------|----------|
| Permission / empty | P0 |
| Home groups list | P0 |
| Reduce session grid | P0 |
| Keep target + counters | P0 |
| Commit summary | P0 |
| Compare 2–4 | P1 |
| Settings | P1 |
| Sort/filter sheets | P1 |
