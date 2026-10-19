# Import from Google Photos

Unswiper can sort the photos you keep in **Google Photos**. Google doesn't let apps read or delete your Google Photos library, so Unswiper uses **Google Takeout**, Google's own page for downloading a copy of everything. Unswiper unpacks that copy into a folder on your computer and then works on that folder like on any other library. It **never signs in to Google**, never sees your password, and works offline.

## Start the assistant

Choose **Library ▸ Import from Google Photos…** in the menu bar, or click **Import from Google Photos…** on the start screen. The assistant shows one step at a time. Click **Next** when a step is done, or **Back** to look again. You can close it at any time: it reopens on the step where you were.

## Step 1 — Open Google Takeout

Click **Open Google Takeout**. Your web browser opens takeout.google.com with Google Photos already chosen. Sign in with the Google account that has your photos. If the page looks wrong, click **Open takeout.google.com** and do step 2 by hand.

## Step 2 — Choose only Google Photos

1. Click **Deselect all**.
2. Tick **Google Photos**. Leave **All photo albums included** as it is.
3. At the bottom of the list, click **Next step**.

## Step 3 — Choose how Google sends it

1. **Destination**: **Send download link via email**.
2. **Frequency**: Google starts with **Export once**. Change it by hand: click **Export every 2 months for 1 year**. Unswiper's link can't choose this for you. It only works when Google Photos is the only product ticked.
3. **File type**: **.zip**.
4. **File size**: **50 GB**.
5. Click **Create export**.

## Step 4 — Wait for Google's email

Preparing the export takes from minutes to a few days; usually it's ready the same day. You can quit Unswiper while you wait.

## Step 5 — Download all parts into one folder

In Google's email, click **Download your files**, then click **Download** next to each part. Put all parts in **one folder**, for example **Downloads** (Safari and Chrome save there by default). Google's links expire after 7 days, and each part can be downloaded only 5 times.

## Step 6 — Choose the folders and check the space

- **Archives folder**: the folder with the downloaded parts. Unswiper finds every .zip and .tgz file in it.
- **Library folder**: where your photos should go, for example a new folder **GooglePhotosLibrary**.

Unswiper then checks the archives **without changing anything** and shows the number of photos and videos, how much is new, the space needed, the free space and about how long it takes. **Next** only works when it fits.

### If the free space "can't be measured"

A full Google Photos export can be hundreds of GB, so Unswiper **never starts without knowing the free space** on the library disk. If the check says the free space can't be measured (this can happen with some network drives), **Next** and **Unpack** stay off. Click **Choose…** next to **Library folder** and pick a folder on a disk of this Mac, for example inside your **Pictures** folder, then click **Check Again**.

## Step 7 — Unpack

Click **Unpack**. The bar shows the part, the amount done and the time left. Click **Stop** at any time; **Continue Unpacking** picks up where it stopped. Files that are already there are never unpacked twice, and a different file with the same name is never overwritten.

- **Delete each archive after unpacking**: each part is deleted as soon as it unpacked completely and without problems. This saves a lot of space: for 120 GB of photos you need about 170 GB free instead of 240 GB.
- **Also unpack album copies**: Google puts a photo that is in an album twice in the export (in its year folder and in the album folder). Unswiper normally unpacks it once. Tick this to get complete album folders; it needs more space.

## Step 8 — Open the library

Click **Open Library**. Unswiper sorts the photos into visits. When a photo has no date or place of its own (screenshots, photos sent by messenger), Unswiper uses the date and place Google saved in the JSON file next to it.

From now on, review and commit as usual. The archives aren't needed any more.

## Deleting rejects in Google Photos

Committing moves the rejected **copies on your computer** to the Trash or the quarantine folder. **Unswiper can't delete photos from Google Photos.** Delete them in Google Photos yourself if you want them gone there too.

## Photos in Google Drive instead: no Takeout needed

If your photos are in **Google Drive** (not Google Photos) and **Google Drive for desktop** is installed on this Mac, you can skip Takeout:

1. Copy the photos you want to review into a folder in your Drive. On the Mac it appears in Finder under **Google Drive**, at `~/Library/CloudStorage/GoogleDrive-<your account>/My Drive/…`.
2. In Unswiper, choose **Library ▸ Choose Folder…** from the menu bar and pick that folder. Unswiper opens it like any other folder.

### The "Items moved" dialog when you commit

With the **Quarantine folder** option, committing moves the rejected photos into `_Rejected/Unswiper/<date and time>` inside that folder. Google Drive then shows an **Items moved** dialog. It says that the items will be visible to everyone who can see the `_Rejected` folder, and that people with access to the original folder may lose access unless the items are shared with them directly.

- **For a private folder** (only you): this is expected and harmless. Click **OK**.
- **Don't click "Move back".** It undoes the commit's moves in Drive.
- **If the folder is shared with others**, they keep or lose access to the moved photos as that dialog says.
- If you don't want to see it each time, tick **Don't show this again** before clicking **OK**.

### Rejects still use Drive space

Drive syncs the moves to the cloud, so the rejected photos stay in your Drive (and use Drive storage) until you delete the `_Rejected` folder.

## The next export

Every 2 months Google emails you again. Download the new parts into the archives folder, open **Import from Google Photos…** and click **Check Again**. Only photos you don't have yet are added. Photos you rejected and deleted are **not** brought back.

## How much space and time?

- **Space**: about the size of your Google Photos library for the library folder, plus the archives while they are kept. The check in step 6 shows the exact figures for your export.
- **Download**: 120 GB takes about 3 hours at 100 Mbit/s, or about 16 minutes at 1 Gbit/s.
- **Unpack**: a few minutes per 100 GB on a fast internal disk (SSD); up to about an hour per 100 GB on a hard disk.

## Command line

The same import works in Terminal with the helper inside the app: `/Applications/Unswiper.app/Contents/MacOS/unswiper-blur takeout import ~/Downloads --to ~/Pictures/GooglePhotosLibrary --dry-run` shows the figures, and without `--dry-run` it unpacks. Add `--delete-archives` or `--keep-duplicates` as above. If the free space can't be measured, the command stops too; `--no-space-check` skips the check, but only use it when you are sure there is enough space.
