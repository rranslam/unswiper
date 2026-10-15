# Install, Update and Uninstall

## Install (AppImage)

1. Go to **https://github.com/rranslam/unswiper/releases/latest** in your web browser.
2. Under **Assets**, click the file ending in **.AppImage** (for example `Unswiper-0.3.0-x86_64.AppImage`). Don't click "Source code".
3. Open **Files**, go to **Downloads**, right-click the file, choose **Properties**, and turn on **Executable as Program** (or run `chmod +x Unswiper-*.AppImage` in a terminal).
4. Double-click the file to start Unswiper.

Unswiper needs **libheif** for iPhone HEIC photos. On Ubuntu/Debian: `sudo apt install libheif1`. On Arch/Omarchy: `sudo pacman -S libheif`. Video thumbnails use **GStreamer**, which most desktops already have. For iPhone videos (H.264/HEVC) you also need its libav plugin: `sudo apt install gstreamer1.0-libav` or `sudo pacman -S gst-libav`. The Flatpak includes everything.

Packages for Ubuntu/Debian (.deb), Arch (AUR) and Flatpak are planned.

## Updating

1. Quit Unswiper (**Ctrl+Q**).
2. Download the new `.AppImage`, make it executable as above, and delete the old one.
3. After updating, choose **Re-index** once.

Your decisions and settings are kept.

## Uninstalling

1. Quit Unswiper and delete the `.AppImage` file.
2. To remove its saved data too, delete the folder `~/.local/share/unswiper` (in **Files**, press **Ctrl+H** to show hidden folders).

Uninstalling never touches your photos.
