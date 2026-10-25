// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Google Takeout import for Unswiper (plan §19).
//!
//! Google Photos can't be read or changed through Google's APIs any more (only app-created media),
//! so Unswiper imports a **Google Takeout** export instead:
//!
//! 1. [`unpack`] safely unpacks the Takeout archives (`.zip` or `.tgz`, any number of parts) into a
//!    folder the user chooses. Only the Google Photos part of the export is unpacked, Google's file
//!    names are kept exactly (the JSON sidecars are matched by name), unsafe paths are refused, and
//!    an interrupted unpack resumes where it stopped. [`plan`] is the dry run (sizes, counts,
//!    space needed), [`find_archives`] picks up every archive in a folder, and a re-import of a
//!    newer export only adds new items.
//! 2. The folder is indexed by the normal pipeline ([`unswiper_core::index::index`]) with
//!    [`TakeoutDates`] as the date reader: the file's own EXIF/QuickTime date first, then the
//!    `photoTakenTime` from Google's JSON sidecar. [`TakeoutDates::location`] does the same for
//!    the location (EXIF GPS first, then `geoData`).
//!
//! Nothing here talks to Google or writes outside the chosen folder.

pub mod cli;
pub mod dates;
pub mod guide;
pub mod safe_path;
pub mod sidecar;
pub mod unpack;

pub use dates::{library_dates, SidecarDate, TakeoutDates};
pub use sidecar::{match_sidecar, TakeoutMeta};
pub use unpack::{
    find_archives, find_parts, free_space, is_takeout_library, measure_free_space, plan, unpack,
    unpack_cancellable, unpack_with, ArchivePlan, FreeSpace, Plan, UnpackError, UnpackOptions,
    UnpackProgress, UnpackReport,
};
