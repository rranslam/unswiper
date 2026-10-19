# Troubleshooting and FAQ

## macOS says Unswiper can't be opened

See **Install, Update and Uninstall ▸ Opening it the first time**. Use **Open Anyway** in System Settings ▸ Privacy & Security.

## My visits look wrong or empty after updating

Click **Re-index** (or press **⇧⌘R**). New versions sometimes store extra details that need a fresh scan.

## "Saved library couldn't be opened"

Unswiper couldn't read its saved file (`library.json`), for example because it was damaged. It made a copy named `library.json.corrupt-<date>` in `~/Library/Application Support/Unswiper` before doing anything else, so nothing is lost. Click **Re-index** to rebuild your visits. If you want your old Keep/Reject choices back, use **Help ▸ Report an Issue** and mention the copy.

To open that folder: in Finder choose **Go ▸ Go to Folder…** (⇧⌘G), paste `~/Library/Application Support/Unswiper`, and press Return.

## My visits changed after updating to 0.1.2

Files without a recorded date are now grouped by their folder instead of their "last changed" date. See **Visits and Grouping**. Click **Re-index** once.

## Some files were "skipped" during commit

They changed after indexing (renamed, edited, moved, or replaced), so Unswiper left them alone to be safe. Click **Re-index**, check your choices, and commit again.

## Restore didn't bring everything back

If you used **Move to Trash** and the Trash was emptied, those files can't be restored. Files that were moved out of the Trash or quarantine folder by hand also can't be restored automatically.

## Where are my rejected files?

- **Move to Trash:** in the macOS Trash (click the Trash icon in the Dock).
- **Quarantine folder:** inside your photo folder, in `_Rejected/Unswiper`. Older versions used `_Rejected/visit-media-reduce`.

## Google Drive shows "Items moved" after a commit

That happens when your photo folder is in Google Drive (`~/Library/CloudStorage/GoogleDrive-…`) and you commit to the quarantine folder. For a private folder click **OK**; don't click **Move back**, which undoes the commit's moves in Drive. Details: **Import from Google Photos ▸ Photos in Google Drive instead**.

## Will it change or compress my photos?

No. Unswiper never edits photos. It only moves the ones you reject, after you confirm.

## Something else is wrong

Choose **Help ▸ Report an Issue** to tell us on GitHub.
