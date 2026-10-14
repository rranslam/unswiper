# Restore Last Commit

Changed your mind? In the main menu (**☰**) choose **Restore Last Commit**. Unswiper moves every file from your most recent commit back to where it came from.

- Every commit keeps a record of where each file came from, so files go back to the right folders.
- If you used **Move to Trash**, restoring only works while the files are still in the Trash. If you emptied the Trash, those files are gone for good.
- If you used **Quarantine folder**, restore works as long as you haven't moved or deleted the `_Rejected/Unswiper` folder.
- Only the **last** commit can be restored with this menu item. It is greyed out when there's nothing to restore.

## If Unswiper quit in the middle of a commit

Unswiper writes its record *before* it moves anything and updates it after every file. If the app crashes, the computer loses power, or you force-quit while files are moving, the next time you open Unswiper you'll see **"A commit was interrupted"**:

- **Restore Files** puts back every file that was already moved.
- **Leave As Is** keeps them in the Trash or quarantine folder. You can still choose **Restore Last Commit** later.

If a file can't be found in the Trash, Unswiper tells you. Open **Files**, go to **Trash**, right-click the file and choose **Restore From Trash**.
