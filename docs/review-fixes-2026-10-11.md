# Review fixes — macOS MVP (2026-10-11)

Scope: `apps/macos/Sources/VisitMediaReduce`. Ordered by priority. Each item: problem → fix → acceptance.

## P0 — Safety (before running on a real library) — ✅ Done (`03de743`)

### 1. Validate every file before commit
- **Problem:** `CommitService.commitRejects` moves paths straight from `library.json` with no check that the file is unchanged or inside the library root.
- **Fix:** Before each move: resolve symlinks, require path to be under `libraryRoot`; compare current size + modification date to values captured at index time (add `modifiedAt` to `MediaItem`). Skip and report mismatches as failures.
- **Accept:** Renamed/replaced/outside-root files are never moved; they show in the failed list.

### 2. Commit manifest + undo
- **Problem:** Quarantine flattens files into one folder; Trash results are discarded. No way to restore.
- **Fix:** Write `manifest.json` (original path, destination/`resultingItemURL`, size, timestamp) per commit, in the quarantine folder and in Application Support. Add a "Restore last commit" action.
- **Accept:** A commit can be fully restored to original paths.

### 3. Prevent accidental commit
- **Problem:** Cmd+Return opens Commit and Return triggers the default action — two keypresses move files.
- **Fix:** Remove `.keyboardShortcut(.defaultAction)` from the commit button, or require a confirmation dialog showing count and bytes.
- **Accept:** No key sequence moves files without an explicit confirm.

### 4. Pair Live Photos / sidecars
- **Problem:** HEIC+MOV pairs (and .AAE/.XMP sidecars) are separate items; rejecting one orphans the other.
- **Fix:** Group by same directory + base name; decisions and moves apply to the whole set.
- **Accept:** Rejecting a Live Photo moves both files.

## P1 — Correctness — ✅ Done (`e13324c`)

### 5. Video capture dates
- Use `AVURLAsset` `creationDate` / QuickTime metadata for videos instead of file mtime.

### 6. Undated files
- Don't fall back to `Date()`; put them in an "Undated" group.

### 7. Commit off the main thread
- Run `CommitService` in `Task.detached` like indexing; show progress; disable navigation while running.

### 8. Debounce persistence
- `persist()` rewrites all of `library.json` on every decision. Debounce (~1s) and save on quit.

## P2 — Cleanup — ✅ Done (`59b0fef`)
- `updateDecision`: update in place by index; don't rebuild from a Dictionary (scrambles order, traps on duplicate keys).
- Balance `startAccessingSecurityScopedResource` with `stopAccessing…`; start once.
- Stable group IDs (e.g. hash of first item path + start time), not including count.
- Keep stepper should set a per-visit target, not the global default.
- Dry run: say "Would move", don't populate `moved`.
- Docs: update README/STATUS — app code now exists.
