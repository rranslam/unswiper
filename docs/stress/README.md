# Large-library stress test (Linux app)

Synthetic photos only; never point this at a personal library.

1. `python3 docs/stress/gen_library.py 20000 /tmp/lib20k` writes 20,000 small JPEGs
   (160×120, every tenth one blurred) with EXIF dates in bursts of 3–25, which gives
   about 1,400 visits. Needs Pillow.
2. Optional large visit: hard-link one dated JPEG 6,000 times into a folder of a copy.
3. `docs/stress/run_probe.sh target/release/unswiper /tmp/lib20k /tmp/support-1`
   indexes with a fresh support folder (cold blur cache), opens the first visit, waits
   for the blur check, makes 12 Keep/Undecided changes on the whole visit, then quits.

`UNSWIPER_PROBE=1` makes the app print `probe:` lines to stderr: every main-loop gap of
200 ms or more (`stall_ms`), every update that took 50 ms or more, milestones, and
`max_stall_ms` at the end. It never prints file names.

Results on the box (8 cores, Ubuntu, Xvfb, page cache dropped before each run):

| Library | Version | Index | Blur done | Max stall | 12 changes |
|---|---|---|---|---|---|
| 20k photos, 1,382 visits | 0.6.0 | 1.2 s | 4.5 s | 2,076 ms | 9.5 s (0.8 s each) |
| 20k photos, 1,382 visits | this branch | 0.5 s | 3.2 s | 128 ms | under 0.05 s each |
| + one 6,000-photo visit | 0.6.0 | 1.3 s | 6.8 s | 5,811 ms | 15.3 s |
| + one 6,000-photo visit | this branch | 0.5 s | 5.0 s | 1,353 ms | 0.6 s total |
