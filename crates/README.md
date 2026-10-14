# Unswiper Rust crates (Phase 1 of the Linux port)

See `docs/linux-port-plan.md` (rev 3). MIT licensed.

| Crate | What |
|---|---|
| `unswiper-core` | Model + `library.json` v1 (byte-compatible with the Swift app), indexer (Live Photo/sidecar pairing, EXIF/QuickTime/folder dates, Undated folder-context rule), visit grouping (same FNV-1a IDs as Swift), store, commit validation + pending journal, quarantine / freedesktop Trash, restore |
| `unswiper-cli` | `unswiper-cli index <folder> [--json]`, `groups <library.json>`, `commit --dry-run <library.json>`, `restore <manifest.json>` |

```sh
cargo test --workspace                  # unit + integration + property tests
cargo llvm-cov --package unswiper-core  # coverage (CI gate: 85% core, 80% overall)
cargo run -p unswiper-cli -- index ~/Pictures
```

The macOS app is unchanged; it switches to this core in Phase 6 (UniFFI).
