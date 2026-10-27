# Large-library stress test (Linux app)

Synthetic photos only; never point this at a personal library.

1. `python3 docs/stress/gen_library.py 20000 /tmp/lib20k` writes 20,000 tiny JPEGs
   (64×48, every tenth one blurred) with EXIF dates in bursts of 3–25, about 1,400 visits.
   Add `--burst 6000` for one extra visit of 6,000 photos (hard links, no extra space).
   Standard library only, so it runs on the Mac too (`python3` from Xcode's tools).
2. Linux: `docs/stress/run_probe.sh target/release/unswiper /tmp/lib20k /tmp/support-1`
   indexes with a fresh support folder (cold blur cache), opens the first (oldest) visit,
   waits for the blur check, then three times leaves and reopens that visit and marks all
   of it Keep and back, and quits.
3. Mac: choose the generated folder as the library in the app and check it as described in
   "Mac checks" below.

`UNSWIPER_PROBE=1` makes the Linux app print `probe:` lines to stderr: every main-loop gap
of 100 ms or more (`stall_ms`), every update that took 50 ms or more, milestones, and
`max_stall_ms` at the end. It never prints file names.

Results on the box (8 cores, Ubuntu, Xvfb, page cache dropped before each run; the last
`stall_ms` at quit is the final save and teardown and is not counted):

| Library | Version | Index | Blur done | Max stall | Opening the 6,000-photo visit |
|---|---|---|---|---|---|
| 20k photos | 0.6.0 | 1.0 s | 2.7 s | 2,188 ms | – |
| 20k photos | list fixes | 0.6 s | 1.4 s | 124 ms | – |
| 20k photos | + virtual grid | 0.5 s | 1.3 s | 123 ms | – |
| 20k + 6,000 in one visit | 0.6.0 | 1.2 s | 4.4 s | 10,184 ms | 2,038 ms, then a 10 s freeze on Keep |
| 20k + 6,000 in one visit | list fixes | 0.5 s | 1.7 s | 1,546 ms | 54 ms, then a 1.5 s layout stall |
| 20k + 6,000 in one visit | + virtual grid | 0.6 s | 1.7 s | 136 ms | under 50 ms each reopen, no stall over 100 ms |

The one stall over 100 ms left (about 120–140 ms) is the first time the main window is
drawn after indexing, with or without a large visit.

## Mac checks

With the generated library open: the indexing sheet shows a progress bar, counts, the time
left and Cancel; the window stays responsive while marking photos in the 6,000-photo visit;
the visits list updates its counts right away; quitting right after a change still saves
(reopen the app and check the change is there).
