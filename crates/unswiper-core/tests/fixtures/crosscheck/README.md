# Swift ↔ Rust cross-check fixtures

`spec.json` describes a small library tree. Both sides build it in a temp folder and index it:

- **Swift (oracle):** `apps/macos/Tests/UnswiperTests/CrossCheckFixtureTests.swift` writes
  `swift/library.json` (via the app's own `LibraryStore.save`) and `swift/table.json`
  (per-item dates/dateSource, visits with IDs and dates, FNV-1a samples). Run on a Mac:

  ```sh
  cd apps/macos
  UNSWIPER_WRITE_CROSSCHECK=1 TZ=America/Los_Angeles swift test --filter CrossCheckFixtureTests
  ```

- **Rust:** `crates/unswiper-core/tests/crosscheck.rs` compares against `swift/` when it exists
  (otherwise it prints a skip note). Run with `TZ=America/Los_Angeles cargo test --test crosscheck`.

Commit the generated `swift/` folder so CI keeps checking it.
