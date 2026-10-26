# Visits and Grouping

A **visit** is a pile of photos and videos taken close together in time — a trip, a party, a day at the beach.

## How visits are made

- Unswiper sorts everything by the date it was taken.
- Whenever there is a gap of more than **36 hours** with no photos, a new visit starts.
- Photos use the date stored inside the photo by the camera. Videos use their own recorded date.
- A photo without that date uses the time saved with its **GPS location**, if it has one.
- Next comes a date in the **file name**, as phones and cameras write it, for example `IMG_20240612_133241.jpg` or `PXL_20240612_133241123.jpg`.
- If a file has no recorded date, Unswiper looks at its **folder names** for a date, for example `2026/01/01`, `2026-01`, `2025-07-04 Beach` or `1999 Photos`.
- If the folder name has no date either, the file joins the visit of the other photos **in the same folder** that do have dates.
- If none of the files in that folder have dates, they get their own visit named after the folder (for example **Lisbon**).
- The file's "last modified" date is **not** used, because copying a file changes it.

## Grouping by place

To group by where photos were taken instead of only by time, choose **Grouping** in the main menu (**☰**):

- **By Visit (Time)**: the default, as described above.
- **By Place**: photos taken within about 1 km of each other form a place, whenever they were taken. Each place is split into its separate trips. Photos far from any other photo go into **Other places**; photos without a location stay in date-based sets marked **No location**.
- **By Visit and Place**: each visit is split where you moved more than about 2 km, so a road trip becomes one group per town.

Place groups show a pin and the nearest town, looked up offline (no map or internet needed). Your choice is saved with your library. Changing it never changes your Keep and Reject decisions.

## The visits list

Each row shows the visit's dates, how many photos and videos it has, and your progress, for example `3 kept · 12 reject · 35 left`. Big visits are usually the best place to start.

## The "Undated" visit

Only files sitting **directly in your library folder** (not in a sub-folder) with no date at all go into a visit called **Undated**. You can reduce it like any other visit.
