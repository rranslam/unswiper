# Photo Info

Photo Info shows how a photo was taken: the camera, the settings, the date, the size and, if the photo has one, the place. It helps you choose between similar shots. Looking at the info **never changes your files**.

## Quick look: right-click

1. Open a visit (click it in the list).
2. **Right-click** (or Control-click) a photo or video and choose **Show Info**.

A small box opens next to it with the main facts: camera and lens, shutter speed, aperture, ISO, focal length, date, size, file type and how sharp Unswiper thinks the photo is. Click anywhere else to close it. Click **Show All Info** to open the full inspector for that item.

## The inspector

- Press **I** or **⌘I** to show or hide the inspector on the right. You can also click **Info** in the bottom bar, or choose **View ▸ Show Inspector** in the menu bar.
- The inspector shows the item you **selected**. If several are selected, it shows the first one and says how many more are selected.
- To copy a value (or the file name), right-click it and choose **Copy**. The text in the inspector can't be selected with the mouse.
- It also works in the viewer: it follows along when you press **←** or **→**.
- The inspector stays open or closed until you quit Unswiper.

## The settings line

- In a visit, a short line in the bottom bar sums up the selected item's settings, for example **26 mm · f/1.8 · 1/120 s · ISO 64 · Sharp**.
- In **Compare**, the line appears under **one** picture only: the one with the blue frame. Click another picture to see its settings instead.

## What the rows mean

- **Camera** and **Lens** — the camera (or iPhone) and the lens it used.
- **Shutter** — how long the picture was exposed, for example **1/120 s**. Long times such as **1/8 s** blur easily if the camera or subject moved.
- **Aperture** — the lens opening, for example **f/1.8**. A small number means a blurrier background.
- **ISO** — the sensor sensitivity. High numbers (above about 1600) mean a darker scene and more grain.
- **Focal length** — the zoom. For iPhones the "equiv." value is what it would be on a classic 35 mm camera.
- **Flash** and **Exposure compensation** — whether the flash fired, and whether the photo was set brighter (+) or darker (−).
- A **—** means the camera didn't record that value. Screenshots and edited copies often have no camera rows at all.
- **Taken** — the date and time the photo was taken, with the time zone when the camera saved it. Otherwise it says **time zone unknown**.
- **Dimensions** — megapixels and width × height. **Size** includes the motion part of a Live Photo, for example **2.1 MB + IMG_1234.MOV, 3.4 MB**.
- **Sharpness** — **Sharp**, **Soft** or **Blurry**, from Unswiper's blur check (see *Finding Blurry Photos*). **Not checked** means the check hasn't run yet, is turned off, or doesn't apply (videos, screenshots, very dark photos).

## Location

Many iPhones and cameras save **where** a photo was taken. If they did, the inspector shows the **Coordinates** (and the altitude).

- The location is read from the photo and shown **only on this Mac**. Unswiper never sends it anywhere and doesn't save it in its own records.
- **Open in Maps** opens the place on **OpenStreetMap** in your web browser. That is the only time the location leaves your Mac, and only when you click it.
- To hide locations, choose **View ▸ Show Location in Photo Info** in the menu bar so the check mark goes away. Unswiper remembers this.
- Unswiper never changes or removes the location stored in your photos.

## About this information

- Photo Info comes from **EXIF** (Exchangeable image file format), a camera-industry standard from CIPA and JEITA. Camera and phone makers follow it loosely, so some fields can be missing or inconsistent, even between two cameras or phones of the same model with different firmware.
- Many DSLRs and other cameras record no GPS location at all.
- The date and time are only as good as the camera's clock: it may have been set to the wrong time zone, or never set.
- Unswiper shows what's in the file and doesn't correct it.
- More about EXIF: https://en.wikipedia.org/wiki/Exif

## If the info doesn't appear

Photo Info is read by a small helper program inside Unswiper.app (the same one that checks for blurry photos). If you see *"Photo Info needs the helper"*, download Unswiper again from the Releases page and replace the app in your Applications folder.
