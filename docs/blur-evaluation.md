<!-- SPDX-License-Identifier: MIT -->
# Unswiper blur / out-of-focus detection: evaluation of open-source options
_Draft, 14 Oct 2026. Sources checked by web search/fetch on that date._

## Requirements (from Rob)
Runs offline, no cloud API. Must be compatible with distributing Unswiper under MIT. Should reuse existing open-source work. Has a config setting, on by default. Scanning may be slow, so it may also exist as a standalone utility. Must handle HEIC (libheif is already a hard dependency).

## Candidates

| # | Candidate | License | Language | Maintenance | Method | Speed (est.) | Accuracy notes | HEIC |
|---|---|---|---|---|---|---|---|---|
| 1 | **Variance of Laplacian in pure Rust**: `image` + `imageproc::filter::laplacian_filter` (3×3 kernel, `_parallel` variant with rayon) — https://crates.io/crates/imageproc , https://docs.rs/imageproc/latest/imageproc/filter/fn.laplacian_filter.html | MIT | Rust | Active (imageproc depends on image 0.25.8) | Laplacian variance (Pech-Pacheco 2000) | Decoding dominates. Laplacian on a 1–2 MP gray image takes roughly 5–15 ms | Good at separating clearly sharp from clearly blurry. Depends on scene and size, so normalize size and use relative thresholds. A single global threshold (100 in PyImageSearch) is "dataset dependent" | Yes, through the core's existing libheif decode path |
| 2 | **OpenCV through the `opencv` crate** (twistedfall/opencv-rust) — https://github.com/twistedfall/opencv-rust | MIT bindings; OpenCV itself is Apache-2.0 | Rust → C++ | Active (last push Mar 2026, v0.98) | Laplacian, Sobel/Tenengrad, Hough (motion blur), contrib `quality` (BRISQUE) | Fast | Same as #1, plus more operators | No direct HEIC support. We'd still need libheif |
| 3 | **FFT high-frequency energy** (PyImageSearch 2020) — https://pyimagesearch.com/2020/06/15/opencv-fast-fourier-transform-fft-for-blur-detection-in-images-and-video-streams/ ; in Rust use `rustfft` (MIT/Apache, already optional in imageproc's `fft` feature) | Tutorial code; algorithm is free to reimplement | Python (re-implement in Rust) | n/a | Zero out low frequencies, inverse FFT, compare mean log magnitude to a threshold | About 20–40 ms at 1 MP | Called more robust than Laplacian, but still needs calibration. Noise reads as high frequency, which hurts in low light | via the core |
| 4 | **WillBrennan/BlurDetection2** — https://github.com/WillBrennan/BlurDetection2 | MIT | Python + OpenCV | Dormant (last update Feb 2023, 387★) | Laplacian variance after resizing to about 2 MP (`fix_image_size`), threshold 100 | Fast | Useful idea: **normalize to a fixed pixel count**. Otherwise trivial | No (cv2.imread) |
| 5 | **digiKam Image Quality Scanner** (GSoC 2021 blur detector) — https://community.kde.org/GSoC/2021/StatusReports/PhuocKhanhLE , https://docs.digikam.org/en/maintenance_tools/maintenance_quality.html , settings https://fossies.org/dox/digiKam-9.0.0/classDigikam_1_1ImageQualitySettings.html | GPL-2.0+, so **code can't be copied into MIT**. Ideas only | C++/Qt/OpenCV | Active (9.x) | Defocus: Laplacian edge map → log filter → blur/median smoothing → per-pixel threshold. Motion: Hough lines per block, parallel lines → motion blur. **Uses AF-point metadata** and ignores low-variance background (bokeh) regions. Maps quality to Rejected/Pending/Accepted pick labels (`lowQRejected`) | Moderate | Best open design reference. digiKam 8.6 also uses an FFT filter for face-quality blur | yes (digiKam) |
| 6 | **darktable focus peaking** — https://darktable-org.github.io/dtdocs/en/module-reference/utility-modules/shared/focus-peaking/ , https://github.com/darktable-org/darktable/blob/9a61cd65/src/common/focus_peaking.h | GPL-3.0, ideas only | C | Active | Gradient/Laplacian with statistical thresholds (2.5σ/5σ/10σ) as a heat map | Fast | The manual itself says it "does not necessarily detect whether an image is sharp" and flags blurred high-contrast edges. Good as a **review overlay** idea, not as a classifier | n/a |
| 7 | **Excire Foto** — https://excire.com/en/excire-foto/ | Proprietary | n/a | Commercial | Local AI "sharpness detection" + focus peaking + Smart Selection | n/a | UX idea: show a sharpness indicator plus peaking, and pick the best shot within a *sequence*. Method isn't documented | n/a |
| 8 | **BRISQUE / NIQE**: OpenCV contrib `quality` module; Python `brisque` (Apache-2.0, needs OpenCV) — https://github.com/rehanguha/brisque | Apache-2.0 (compatible, needs a NOTICE) | C++/Python | Python pkg active (v0.2.0) | Natural-scene-statistics no-reference IQA. Needs model/range YAML files | Slower (100+ ms) | Scores *overall* distortion (noise, JPEG, blur) rather than focus specifically. Penalizes bokeh and night shots. Weak for our purpose | via the core |
| 9 | **pyiqa / IQA-PyTorch** — https://github.com/chaofengc/iqa-pytorch | **PolyForm Noncommercial 1.0.0** + NTU S-Lab, so **not compatible** with MIT distribution | Python/PyTorch | Active (3.4k★) | Many IQA models | Slow without a GPU | Good for *offline benchmarking during development only*. Never ship it, and don't ship models taken from it | n/a |
| 10 | **Small ONNX model through `ort`** (MIT/Apache-2.0, wraps ONNX Runtime, MIT) — https://crates.io/crates/ort , or **`tract`** (pure Rust, MIT/Apache-2.0) — https://lib.rs/crates/tract | Runtimes OK; **model weights need their own licence check** (most IQA weights are research/noncommercial, e.g. pyiqa) | Rust | Active | CNN blur/IQA classifier (e.g. MobileNet-sized) | 10–50 ms on CPU | Can handle bokeh better (it learns "intentional"). But it adds tens of MB and needs a permissively licensed model or our own training. ort brings a native lib, which complicates Flatpak/AppImage. tract is easier | via the core |
| 11 | **EXIF/MakerNote AF data**: Canon `AFPointsInFocus`/`AFAreaMode`, Nikon `AFInfo2` — https://exiftool.org/TagNames/Nikon.html , https://github.com/exiftool/exiftool/blob/78ef8fcf/html/TagNames/Canon.html | Data (exiftool is Artistic/GPL, so use it as a reference or test tool only; parse with the existing Rust EXIF reader) | n/a | n/a | Points to *where* the camera focused. Score that region | Free | Vendor-specific. iPhones (Rob's main source) have no AF-point MakerNote, though Apple MakerNotes / HEIC depth or portrait flags can mark intentional bokeh. Use it as a hint, never as proof | n/a |

## Pitfalls and mitigations
- **Intentional bokeh / shallow depth of field.** A global Laplacian variance is low even though the subject is sharp. Fix: score as the **max (or 90th percentile) over tiles** (e.g. a 4×4 grid), so one sharp subject region passes. Also skip low-variance tiles, as digiKam does. Where available, score the AF-point region. Treat iPhone Portrait-mode files (depth data or Apple portrait flags) as "don't auto-reject."
- **Low light / night / high ISO.** Noise inflates Laplacian and FFT scores, making blurry shots look sharp. Dark scenes have few edges, making sharp shots look blurry. Fix: apply a light Gaussian blur first (σ≈1, as darktable does); estimate noise; lower confidence when mean luma is low or ISO is high, and never auto-flag in "low confidence."
- **Motion vs focus blur.** Both should usually be culled. Fix: detect directional blur (gradient-orientation anisotropy, or Hough lines like digiKam) for reporting only ("motion blur" vs "out of focus"). Same action.
- **Resolution.** Scores depend on scale. Fix: always decode to a fixed long edge (e.g. 1024 px, about 0.75 MP), using libheif/JPEG downscaled decode or the embedded thumbnail when ≥1024 px. BlurDetection2 normalizes to 2 MP for the same reason.
- **Scene content** (sky, snow, walls, fog, water) has low texture, so it looks blurry. Fix: **relative thresholds** within a visit/burst. Only flag a photo whose score is far below its burst siblings, or below an absolute floor that's *very* low.
- **Screenshots, scans, graphics**: skip non-camera images (no camera Make/Model).
- **Videos**: out of scope for v1.

## Conclusion
Implement **#1 (pure-Rust tiled Laplacian variance)**, optionally combined with a cheap **gradient-energy (Tenengrad)** score from imageproc's Sobel, both MIT. Take design ideas from digiKam (#5), BlurDetection2 (#4), darktable (#6) and Excire (#7). Use AF/portrait metadata (#11) as a hint. Avoid OpenCV (heavy native dep, no gain for this), BRISQUE (wrong target), and pyiqa (licence). Keep an ONNX model through `tract` (#10) as a possible v2, but only with a permissively licensed or self-trained model.

## Real-photo calibration (15 Oct 2026)

**Set:** 85 of Rob's own phone JPEGs (2019, 3120×4160 and a few small ones), already culled by him. They are personal, so they are **not** in the repo. Only these aggregate numbers are. Nothing in the file or folder names marked a photo as blurry, so each photo was judged by eye (full view plus a 100 % centre crop):
- **4 blurry**: 2 fully defocused bokeh shots, 1 defocused wide shot, 1 soft indoor shot.
- **78 sharp**, including the tricky cases (counted as sharp, so any flag is a false positive): a plain overcast sky, rose close-ups with smooth petals, a breaker panel, seascapes with strong horizon lines, dusk, glare and through-the-windscreen shots.
- **3 left out**: two almost black frames (low confidence by design, never flagged) and a 500 px scan of an old soft print.

**Tool:** `cargo run --release -p unswiper-blur --example calibrate -- <folder> [blurry-list.txt]`. Each photo is scored on its own, without bursts. Synthetic recall comes from adding blur to the 78 sharp photos at the 1024 px working size: Gaussian σ 2 and σ 4, and a 15 px and a 31 px box shake, alternating horizontal and vertical. A full `unswiper-blur scan` with burst grouping gave the same flags.

**Before** (floor 8 / 15 / 30, shake anisotropy ≥ 0.5):

| Sensitivity | False positives on sharp | Real blurry found | Synthetic σ2 / σ4 / shake 15 / shake 31 |
|---|---|---|---|
| Low | 5 / 78 (6.4 %) | 2 / 4 | 59 / 71 / 46 / 69 of 78 |
| Normal | 11 / 78 (14.1 %) | 2 / 4 | 69 / 71 / 61 / 71 |
| High | 18 / 78 (23.1 %) | 2 / 4 | 70 / 71 / 69 / 71 |

Most false positives came from the **shake rule**. Sharp seascapes and panels with long straight edges have gradient anisotropy 0.45 to 0.66 (one at 0.80), while synthetic shake that the floor doesn't already catch is mostly at 0.7 or above.

**Change:** shake anisotropy 0.5 → **0.7**. Floors 8/15/30 → **7/13/25**. The burst factors are unchanged. These were picked on a small grid (floors × anisotropy 0.5–0.75 × floor factor 2–4) as the setting with the fewest false positives at Normal that still kept synthetic recall.

**After:**

| Sensitivity | False positives on sharp | Real blurry found | Synthetic σ2 / σ4 / shake 15 / shake 31 |
|---|---|---|---|
| Low | 1 / 78 (1.3 %) | 2 / 4 | 52 / 71 / 33 / 66 of 78 |
| Normal | 5 / 78 (6.4 %) | 2 / 4 | 67 / 71 / 49 / 70 |
| High | 10 / 78 (12.8 %) | 2 / 4 | 69 / 71 / 64 / 71 |

The synthetic accuracy gate (`tests/accuracy.rs`) still passes. The 7 sharp photos never flagged even at σ 4 are low-confidence (dark or almost no texture), as intended.

**What's left:**
- At Normal, the remaining false positives are low-texture subjects that a Laplacian floor can't tell from defocus: the plain sky, three rose close-ups and a breaker panel.
- The two missed blurry photos are full-frame bokeh of point lights. The bright discs keep the sharpness score at about 31 to 37.
- Both need content cues, for example a "too little structure to judge" confidence based on edge density, or EXIF/portrait hints, and they're left for later.
- 85 photos with 4 real blurry ones is too small a set to fit thresholds tightly. Re-run the tool on a larger set (ideally including Rob's rejects) before 1.0.

## Reading the score

(Added 26 Oct 2026, with the user-set cutoffs.)

**What it measures.** The photo is turned to grey, shrunk so its long edge is at most 1024 px (smaller photos aren't enlarged), and smoothed slightly (Gaussian σ 1). It's cut into a 4 × 4 grid; flat tiles (almost no texture) are skipped and faint tiles are boosted as if they had normal contrast. In each tile the **Laplacian variance** is measured: how strongly brightness changes from a pixel to its neighbours, which is high for crisp edges and fine texture and low when they're smeared. The score is the 90th percentile over the tiles, so one sharp area (a subject in front of a soft background) is enough. **Higher = sharper.** It has no upper limit.

**Cutoffs.** A photo is "blurry" when its score is below the cutoff of the chosen sensitivity: **Low 7, Normal 13, High 25** by default (calibrated on 85 real photos, above). Since 26 Oct 2026 they can be changed in the apps (Linux: ☰ ▸ Blurry Photos ▸ Blur Sensitivity ▸ Cutoffs…; Mac: Blurry Photos ▸ Blur Cutoffs…), stored as `config.blurCutoffs` in library.json. Two more rules use the cutoff: **shake** (gradient anisotropy ≥ 0.7 and score below 4 × the cutoff) and **burst** (below 0.25 / 0.35 / 0.45 × the sharpest photo of a burst of 3+ photos within 3 s; this factor does not change with the cutoffs). Photos with confidence below 0.5 (mean brightness under 40/255, or fewer than 2 textured tiles) are never flagged.

**Typical values** (synthetic scenes, `cargo run --release -p unswiper-blur --example scale`, 12 seeds, median and range):

| Scene at 1024 × 768 | Score |
|---|---|
| sharp | 1,124 (522 – 3,591) |
| defocus σ 1 | 43 (22 – 207) |
| defocus σ 1.5 | 4.6 (3.4 – 5.9) |
| defocus σ 2 | 2.1 (1.4 – 3.0) |
| defocus σ 4 | 0.4 (0.3 – 0.5) |
| shake 9 / 15 / 31 px | 19.5 / 14.6 / 15.5 |

**Size matters through the shrink.** The score describes sharpness at the 1024 px working size, not at 100 %. A 12 MP phone photo (4160 px) is shrunk 4×, so slight softness at full size disappears (σ 2 at 4000 px is σ 0.5 after the shrink), but so does detail that is only a pixel or two wide. In the synthetic scenes, whose texture is pixel-sized noise, the sharp scene scores 1,124 at 1024 px but only 16 (10 – 22) when generated at 2048 or 4000 px and shrunk. Real photos have detail at many scales and keep much more of it, but very large photos whose detail is mostly fine (distant foliage, gravel, fabric) can land near the Normal and High cutoffs. A photo smaller than 1024 px is scored as it is.

**Reading the low end.** From the synthetic and real numbers: below about 5 is clearly defocused; 5 – 15 is soft or shaken *or* a sharp photo of a plain or very fine-textured subject; above 25 is almost always sharp. The false positives in the real-photo calibration (plain sky, petals, a breaker panel) sat in that middle band.

**Printing the scores.** `unswiper-blur scan <folder or library.json>` prints every scored photo, softest first: score, verdict (`ok`, `BLURRY`, `BLURRY (burst)`, `BLURRY (shake)`) and path, with the cutoff in use in the first line. With the app's library.json it uses the saved sensitivity and cutoffs and the scores the app already cached. `--cutoffs L,N,H` and `--sensitivity` try other values without changing anything; `--json` adds shake, brightness and confidence.
