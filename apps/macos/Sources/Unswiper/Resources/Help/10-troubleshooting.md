# Troubleshooting and FAQ

## macOS says Unswiper can't be opened

See **Install, Update and Uninstall ▸ Opening it the first time**. Use **Open Anyway** in System Settings ▸ Privacy & Security.

## My visits look wrong or empty after updating

Click **Re-index** (or press **⇧⌘R**). New versions sometimes store extra details that need a fresh scan.

## Some files were "skipped" during commit

They changed after indexing (renamed, edited, moved, or replaced), so Unswiper left them alone to be safe. Click **Re-index**, check your choices, and commit again.

## Restore didn't bring everything back

If you used **Move to Trash** and the Trash was emptied, those files can't be restored. Files that were moved out of the Trash or quarantine folder by hand also can't be restored automatically.

## Where are my rejected files?

- **Move to Trash:** in the macOS Trash (click the Trash icon in the Dock).
- **Quarantine folder:** inside your photo folder, in `_Rejected/Unswiper`. Older versions used `_Rejected/visit-media-reduce`.

## Will it change or compress my photos?

No. Unswiper never edits photos. It only moves the ones you reject, after you confirm.

## Something else is wrong

Choose **Help ▸ Report an Issue** to tell us on GitHub.
