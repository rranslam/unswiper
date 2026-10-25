// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Display-free rules for the media viewer (plan §16): what to play for an item (the video
//! itself, or a Live Photo's motion clip when that option is on), looping, and durations.

use std::time::Duration;
use unswiper_core::{MediaItem, MediaKind};

/// Clips shorter than this loop (plan §16.3).
pub const LOOP_UNDER: Duration = Duration::from_secs(10);

/// Video file extensions that can be a Live Photo's motion part.
const MOTION_EXTS: [&str; 3] = ["mov", "mp4", "m4v"];

/// What the viewer shows for one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerMedia {
    pub path: String,
    pub kind: MediaKind,
    /// File to play: the video itself, or a Live Photo's motion clip. `None` = still only.
    pub play: Option<String>,
    /// The played file is a Live Photo's motion part (always loops, starts muted).
    pub is_motion: bool,
}

/// The Live Photo motion clip paired with a photo (`IMG_1.HEIC` + `IMG_1.MOV`).
pub fn motion_clip(item: &MediaItem) -> Option<String> {
    if item.kind != MediaKind::Photo {
        return None;
    }
    item.sidecars
        .iter()
        .find(|s| {
            s.path
                .rsplit_once('.')
                .is_some_and(|(_, e)| MOTION_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        })
        .map(|s| s.path.clone())
}

/// What to show for `item`. Live Photo motion only plays when `live_motion` is on.
pub fn media_for(item: &MediaItem, live_motion: bool) -> ViewerMedia {
    let (play, is_motion) = match item.kind {
        MediaKind::Video => (Some(item.path.clone()), false),
        MediaKind::Photo => match motion_clip(item).filter(|_| live_motion) {
            Some(m) => (Some(m), true),
            None => (None, false),
        },
    };
    ViewerMedia {
        path: item.path.clone(),
        kind: item.kind,
        play,
        is_motion,
    }
}

/// Short clips and Live Photo motion loop; longer videos play once.
pub fn should_loop(duration: Option<Duration>, is_motion: bool) -> bool {
    is_motion || duration.is_some_and(|d| d < LOOP_UNDER)
}

/// `0:07`, `1:05`, `1:02:03`. Rounded down to whole seconds.
pub fn format_duration(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unswiper_core::time::parse_iso;
    use unswiper_core::{Decision, SidecarFile};

    fn item(path: &str, kind: MediaKind, sidecars: &[&str]) -> MediaItem {
        MediaItem {
            path: path.into(),
            kind,
            size_bytes: 1,
            taken_at: parse_iso("2026-01-01T00:00:00Z").unwrap(),
            relative_folder: String::new(),
            decision: Decision::Undecided,
            modified_at: None,
            sidecars: sidecars
                .iter()
                .map(|p| SidecarFile {
                    path: p.to_string(),
                    size_bytes: 1,
                    modified_at: None,
                    extra: Default::default(),
                })
                .collect(),
            is_undated: false,
            date_source: None,
            folder_path: None,
            location: None,
            extra: Default::default(),
        }
    }

    #[test]
    fn media_for_video_photo_and_live_photo() {
        let v = item("/a/clip.MP4", MediaKind::Video, &[]);
        let m = media_for(&v, false);
        assert_eq!(m.play.as_deref(), Some("/a/clip.MP4"));
        assert!(!m.is_motion);
        let live = item(
            "/a/IMG_1.HEIC",
            MediaKind::Photo,
            &["/a/IMG_1.AAE", "/a/IMG_1.MOV"],
        );
        assert_eq!(motion_clip(&live).as_deref(), Some("/a/IMG_1.MOV"));
        assert_eq!(media_for(&live, false).play, None, "option off by default");
        let m = media_for(&live, true);
        assert_eq!(m.play.as_deref(), Some("/a/IMG_1.MOV"));
        assert!(m.is_motion);
        let still = item(
            "/a/IMG_2.JPG",
            MediaKind::Photo,
            &["/a/IMG_2.AAE", "/a/noext"],
        );
        assert_eq!(media_for(&still, true).play, None);
        assert_eq!(motion_clip(&v), None, "videos have no motion part");
    }

    #[test]
    fn looping_and_durations() {
        assert!(should_loop(Some(Duration::from_secs(3)), false));
        assert!(!should_loop(Some(Duration::from_secs(10)), false));
        assert!(!should_loop(None, false));
        assert!(should_loop(None, true));
        assert_eq!(format_duration(Duration::from_millis(7900)), "0:07");
        assert_eq!(format_duration(Duration::from_secs(65)), "1:05");
        assert_eq!(format_duration(Duration::from_secs(3723)), "1:02:03");
    }
}
