# Review & Commit

When you're happy with your choices, click **Review & commit…**. This is the only step that moves files.

## Choose what happens to rejected files

At the top of the visits list (left side of the window), the **Commit** menu decides where rejected files go:

- **Move to Trash** — rejected files go to the macOS Trash (the Trash icon in the Dock). You can drag them back out until you empty the Trash.
- **Quarantine folder** — rejected files are moved to a folder named `_Rejected/Unswiper` inside your photo folder, in a sub-folder named with the date and time. Nothing is deleted.
- **Dry run (decisions only)** — nothing moves. Unswiper only tells you what **would** move. Great for practice.

## Confirming

The Review screen shows how many files and how much space will be moved. Click the button (for example **Move to Trash…**), and a confirmation box appears. Click it again to confirm, or **Cancel** to go back. Pressing Return alone never moves files.

## Safety checks — "skipped" files

Before moving each file, Unswiper checks that:

- it is still inside your chosen photo folder, and
- it is the same file as when it was indexed (same size and same last-changed time).

If a file was renamed, edited or replaced since indexing, it is **skipped** and left alone, and the result message tells you how many were skipped. Click **Re-index** and try again.
