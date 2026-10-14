# Troubleshooting and FAQ

## HEIC photos show no thumbnail

Install **libheif** (`sudo apt install libheif1`, or `sudo pacman -S libheif`) and restart Unswiper.

## Videos show a film icon instead of a thumbnail

Video thumbnails come from **GStreamer**. iPhone videos (H.264 and HEVC) need its libav plugin: `sudo apt install gstreamer1.0-libav`, or `sudo pacman -S gst-libav`. If you use the Flatpak, run `flatpak update` so the codecs extension is installed. Without a decoder, Unswiper still works and only the thumbnail is missing.

## My visits look wrong or empty

Choose **Re-index** (or press **Ctrl+Shift+R**).

## "Saved library couldn't be opened"

Unswiper couldn't read its saved file (`library.json`), for example because it was damaged. It made a copy named `library.json.corrupt-<date>` in `~/.local/share/unswiper` before doing anything else, so nothing is lost. Choose **Re-index** to rebuild your visits. If you want your old Keep/Reject choices back, report an issue and mention the copy.

## "Move to Trash" fails for photos on another drive

Your desktop's Trash only takes files from the drive your home folder is on. Switch **Commit Mode** to **Quarantine folder**.

## Some files were "skipped" during commit

They changed after indexing (renamed, edited, moved, or replaced), so Unswiper left them alone to be safe. Choose **Re-index**, check your choices, and commit again.

## Restore didn't bring everything back

If you used **Move to Trash** and the Trash was emptied, those files can't be restored. Files that were moved out of the Trash or quarantine folder by hand also can't be restored automatically.

## Where are my rejected files?

- **Move to Trash:** in your Trash (**Files ▸ Trash**).
- **Quarantine folder:** inside your photo folder, in `_Rejected/Unswiper`.

## Will it change or compress my photos?

No. Unswiper never edits photos. It only moves the ones you reject, after you confirm.

## Something else is wrong

Report it at https://github.com/rranslam/unswiper/issues
