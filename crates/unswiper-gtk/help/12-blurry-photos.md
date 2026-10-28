# Blurry Photos

Unswiper can spot photos that are probably out of focus or shaken, and mark them **Reject** for you so they're easy to drop. It's **on** when you first start Unswiper.

**Nothing is moved or deleted automatically.** A blurry photo is only *marked* Reject. It moves to the Trash (or Quarantine) only when **you** open its visit and click **Review & Commit…**, exactly like any photo you rejected yourself.

## How you see them

- After indexing, Unswiper quietly checks each photo. The visit you have open is checked first. While it works, the title bar shows a progress bar and **Checking sharpness 120 of 900** with the time left. You can keep working: marks appear as results come in. The pause button waits, the stop button stops for now. Photos already checked are remembered, so the next check (or **Check for Blurry Photos Now** in the menu) only looks at the rest.
- A photo it thinks is blurry gets an orange **Blurry?** badge and is dimmed, like a rejected photo.
- A short message at the bottom tells you how many photos were marked.
- To see only those photos, click **Blurry** in the row of buttons at the top right (next to **All**, **Photos**, **Videos**).
- When you click **Review & Commit…**, the confirmation box tells you how many of the rejected items were suggested as blurry, so you can press **Cancel** and look at them first.

## Changing a suggestion

Your choice always wins over Unswiper's guess:

- **Keep it:** click the photo, then press **K** (or click **Keep**).
- **Not sure / not blurry:** click the photo, then press **U** (or click **Clear**). Unswiper remembers this and won't mark that photo again.
- **Agree it's blurry:** press **R**. It becomes your own Reject.

For a whole visit at once, open the main menu (**☰**, top right) and look under **Blurry Photos**:

- **Not Blurry: Clear Suggestions in This Visit** — clears every **Blurry?** mark in the open visit.
- **Reject All Blurry Suggestions in This Visit** — turns them into your own Rejects.

## Settings

All in the main menu (**☰**) under **Blurry Photos**:

- **Detect Blurry Photos** — click to turn it off or on. Turning it off removes every **Blurry?** mark (photos go back to undecided). Your own Keep/Reject choices are never touched.
- **Blur Sensitivity** — **Low** marks only very blurry photos, **Normal** is the default, **High** marks more photos (and is more likely to be wrong).
- **Blur Sensitivity** ▸ **Cutoffs…** — the score below which a photo counts as blurry at **Low**, **Normal** and **High** (defaults 7, 13 and 25). Lower numbers mark fewer photos; Low must stay below Normal, and Normal below High. **Reset to Defaults** puts the original numbers back. Saving re-applies the marks right away from the scores already saved; nothing is checked again.
- **Check for Blurry Photos Now** — checks again, for example after you added photos. New photos are also checked automatically when they appear in your folder.

## The sharpness score

Open a photo in the large view (double-click it) to see its score next to its name: **Sharpness 42.7**, or **Blurry? · 4.2** on a photo marked as blurry. Higher means sharper.

- The score measures how strongly the finest detail changes from pixel to pixel (the "Laplacian variance"), on a copy of the photo at most 1024 pixels wide, so it describes how sharp the photo looks at screen size, not at 100 %.
- Clearly blurry photos usually score in single digits. Sharp photos usually score in the tens or more, detailed ones in the hundreds or thousands. Very large photos with only fine detail (distant foliage, gravel) can score lower, because that detail disappears in the 1024-pixel copy. Plain subjects (sky, a wall, smooth petals) score low even when they're in focus, which is the usual reason for a wrong **Blurry?**.
- A photo can also be marked when it looks shaken (streaks in one direction and a score below 4 × the cutoff) or when it's much softer than the best shot of a burst taken within a few seconds. Then the score can be above the cutoff.
- No number is shown for videos, screenshots, photos not checked yet, and photos too dark or plain to judge.

## What it never marks

- Videos, screenshots (PNG) and GIFs.
- Very dark photos (night shots): they're too dark to judge.
- Photos with almost no detail (a plain sky, a white wall).
- A photo you already decided on, or one you cleared with **U**.

Photos with a soft, blurred background but a sharp subject (portraits) are usually left alone, because Unswiper only needs a sharp spot somewhere in the photo.

## Privacy

Everything happens on your computer. No photo is uploaded anywhere. The sharpness scores are saved in a file named `quality-cache.json` next to your decisions (see **Install & Update** for the folder). Deleting that file is safe: Unswiper just checks the photos again.
