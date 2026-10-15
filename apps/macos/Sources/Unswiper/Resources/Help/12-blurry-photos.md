# Blurry Photos

Unswiper can spot photos that are probably out of focus or shaken, and mark them **Reject** for you so they're easy to drop. It's **on** when you first start Unswiper.

**Nothing is moved or deleted automatically.** A blurry photo is only *marked* Reject. It moves to the Trash (or Quarantine) only when **you** open its visit and click **Review & commit…**, exactly like any photo you rejected yourself.

## How you see them

- After indexing, Unswiper quietly checks each photo. The visit you have open is checked first. While it works, a line at the bottom of the window says **Checking sharpness…**. You can keep working.
- A photo it thinks is blurry gets an orange **Blurry?** badge and is dimmed, like a rejected photo.
- A short message at the bottom tells you how many photos were marked.
- To see only those photos, click **Blurry** in the row of buttons at the top (next to **All**, **Photos**, **Videos**).
- When you click **Review & commit…**, the confirmation box tells you how many of the rejected items were suggested as blurry, so you can press **Cancel** and look at them first.

## Changing a suggestion

Your choice always wins over Unswiper's guess:

- **Keep it:** click the photo, then press **K** (or click **Keep**).
- **Not sure / not blurry:** click the photo, then press **U** (or click **Clear**, or right-click it and choose **Clear (Not Blurry)**). Unswiper remembers this and won't mark that photo again.
- **Agree it's blurry:** press **R**. It becomes your own Reject.

For a whole visit at once, use the **Blurry Photos** menu at the top of the screen (or the **Blurry?** button above the photos):

- **Not Blurry: Clear Suggestions in This Visit** — clears every **Blurry?** mark in the open visit.
- **Reject All Blurry Suggestions in This Visit** — turns them into your own Rejects.

## Settings

All in the **Blurry Photos** menu at the top of the screen:

- **Detect Blurry Photos** — click to turn it off or on (a check mark shows it's on). Turning it off removes every **Blurry?** mark (photos go back to undecided). Your own Keep/Reject choices are never touched.
- **Blur Sensitivity** — **Low** marks only very blurry photos, **Normal** is the default, **High** marks more photos (and is more likely to be wrong).
- **Check for Blurry Photos Now** — checks again, for example after you added photos. New photos are also checked automatically when they appear in your folder.

## What it never marks

- Videos, screenshots (PNG) and GIFs.
- Very dark photos (night shots): they're too dark to judge.
- Photos with almost no detail (a plain sky, a white wall).
- A photo you already decided on, or one you cleared with **U**.

Photos with a soft, blurred background but a sharp subject (portraits) are usually left alone, because Unswiper only needs a sharp spot somewhere in the photo.

## Privacy

Everything happens on your computer. No photo is uploaded anywhere. The sharpness scores are saved in a file named `quality-cache.json` next to your decisions, in `~/Library/Application Support/Unswiper`. Deleting that file is safe: Unswiper just checks the photos again.
