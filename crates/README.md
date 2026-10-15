# Unswiper Rust crates (Phase 1 of the Linux port)

See `docs/linux-port-plan.md` (rev 3). MIT licensed.

| Crate | What |
|---|---|
| `unswiper-core` | Live folder watching, model + `library.json` v1 (byte-compatible with the Swift app), indexer (Live Photo/sidecar pairing, EXIF/QuickTime/folder dates, Undated folder-context rule), visit grouping (same FNV-1a IDs as Swift), store, commit validation + pending journal, quarantine / freedesktop Trash, restore |
| `unswiper-cli` | `unswiper-cli index <folder> [--json]`, `groups <library.json>`, `commit --dry-run <library.json>`, `restore <manifest.json>` |

```sh
cargo test --workspace                  # unit + integration + property tests
cargo llvm-cov --package unswiper-core  # coverage (CI gate: 85% core, 80% overall)
cargo run -p unswiper-cli -- index ~/Pictures
```

## Where data lives

- **Linux:** `$XDG_DATA_HOME/unswiper/` (default `~/.local/share/unswiper/`), lowercase, decided 13 Oct 2026: `library.json`, `commits/`, `Quarantine/`.
- **macOS:** `~/Library/Application Support/Unswiper/` (unchanged from the Swift app).

## Live folder watching (Phase 1b)

`unswiper_core::watch`: `FolderWatcher` (notify: inotify/FSEvents, polling fallback) → debounced `WatchBatch` (500 ms quiet, 5 s max; overflow → `Rescan`) → `LiveIndex::apply` re-reads only the affected pairing sets/folders and regroups. `LiveIndex::suppress` hides the app's own commit/restore moves.

The macOS app is unchanged; it switches to this core in Phase 6 (UniFFI).
