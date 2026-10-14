# Review & Commit

When you're happy with your choices for a visit, click **Review & Commit…** at the bottom of that visit. This is the only step that moves files.

A commit only acts on the **visit that is open**: its rejected items are moved, and every other visit is left exactly as it is, even if it has rejected items too. The button is greyed out until you open a visit that has at least one rejected item.

## Choose what happens to rejected files

In the main menu (**☰**), **Commit Mode** decides where rejected files go:

- **Move to Trash** — rejected files go to your desktop's Trash (open **Files** and click **Trash** in the sidebar). You can restore them from there until you empty the Trash.
- **Quarantine folder** — rejected files are moved to a folder named `_Rejected/Unswiper` inside your photo folder, in a sub-folder named with the date and time. Nothing is deleted.
- **Dry run (decisions only)** — nothing moves. Unswiper only tells you what **would** move. Great for practice.

If your photos are on a different drive than your home folder, use **Quarantine folder**: the Trash can't take files from other drives.

## Confirming

The confirmation box names the visit and shows how many rejected items (and files, and how much space) will be moved, plus how many kept and undecided items stay. Click the red button (for example **Move to Trash**) to confirm, or **Cancel** to go back. Pressing Enter alone never moves files — it picks **Cancel**.

## Safety checks — "skipped" files

Before moving each file, Unswiper checks that:

- it is still inside your chosen photo folder, and
- it is the same file as when it was indexed (same size and same last-modified time).

If a file was renamed, edited or replaced since indexing, it is **skipped** and left alone, and the result message tells you how many were skipped. Choose **Re-index** and try again.
