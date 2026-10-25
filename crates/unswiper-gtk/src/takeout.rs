// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! The guided "Import from Google Photos" setup (plan §19.4), display-free part: what the
//! assistant remembers between runs, the words it shows, and the date reader for Takeout
//! libraries. The window itself is in [`crate::takeout_ui`].

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use unswiper_core::index::{DateReader, MetadataDates};
pub use unswiper_takeout::cli::report_lines;
use unswiper_takeout::cli::{duration, human};
use unswiper_takeout::{is_takeout_library, TakeoutDates, UnpackOptions, UnpackProgress};

/// Remembered in the support folder, so the assistant reopens where the user left it (for
/// example while waiting for Google's email).
pub const SETUP_FILE: &str = "takeout-setup.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Setup {
    /// 0-based step of the guide.
    pub step: usize,
    pub archives_dir: Option<PathBuf>,
    pub library_dir: Option<PathBuf>,
    pub delete_archives: bool,
    /// Also unpack album copies (off: they are skipped as duplicates).
    pub keep_duplicates: bool,
}

impl Setup {
    pub fn load(support: &Path) -> Setup {
        std::fs::read(support.join(SETUP_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, support: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(support)?;
        let tmp = support.join(format!("{SETUP_FILE}.tmp"));
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).unwrap_or_default())?;
        std::fs::rename(tmp, support.join(SETUP_FILE))
    }

    pub fn options(&self) -> UnpackOptions {
        UnpackOptions {
            delete_archives: self.delete_archives,
            skip_duplicates: !self.keep_duplicates,
            ..Default::default()
        }
    }

    /// Why the folders can't be checked yet, or `None` when they can.
    pub fn folder_problem(&self) -> Option<&'static str> {
        match (&self.archives_dir, &self.library_dir) {
            (None, _) => Some("Choose the archives folder."),
            (_, None) => Some("Choose the library folder."),
            (Some(a), Some(l)) if a == l => {
                Some("Choose two different folders: one with the archives, one for the library.")
            }
            (Some(a), Some(l)) if l.starts_with(a) && l != a => None,
            (Some(a), Some(l)) if a.starts_with(l) => Some(
                "The archives folder can't be inside the library folder. Choose another library folder.",
            ),
            _ => None,
        }
    }
}

/// "Step 3 of 8".
pub fn step_label(step: usize, steps: usize) -> String {
    format!("Step {} of {steps}", step + 1)
}

/// Progress bar fraction and text: "Part 2 of 3 · 12.0 GB of 40.0 GB · 95.0 MB/s · about 7 min left".
pub fn progress_text(p: &UnpackProgress, elapsed: Duration) -> (f64, String) {
    let frac = if p.bytes_total > 0 {
        (p.bytes as f64 / p.bytes_total as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let secs = elapsed.as_secs_f64();
    let rate = if secs > 0.5 {
        p.bytes as f64 / secs
    } else {
        0.0
    };
    let mut t = format!(
        "Part {} of {} · {} files · {} of {}",
        p.archive.max(1),
        p.archives,
        p.files,
        human(p.bytes),
        human(p.bytes_total)
    );
    if rate > 0.0 {
        t.push_str(&format!(" · {}/s", human(rate as u64)));
        if p.bytes_total > p.bytes {
            t.push_str(&format!(
                " · {} left",
                duration(((p.bytes_total - p.bytes) as f64 / rate) as u64)
            ));
        }
    }
    (frac, t)
}

/// The date reader for `root`: Takeout libraries also use Google's JSON sidecars.
pub fn dates_for(root: &Path) -> Box<dyn DateReader + Send + Sync> {
    if is_takeout_library(root) {
        Box::new(TakeoutDates::new(MetadataDates))
    } else {
        Box::new(MetadataDates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_between_runs() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(Setup::load(d.path()), Setup::default());
        let s = Setup {
            step: 5,
            archives_dir: Some("/a".into()),
            library_dir: Some("/b".into()),
            delete_archives: true,
            keep_duplicates: false,
        };
        s.save(d.path()).unwrap();
        assert_eq!(Setup::load(d.path()), s);
        std::fs::write(d.path().join(SETUP_FILE), b"not json").unwrap();
        assert_eq!(Setup::load(d.path()), Setup::default());
    }

    #[test]
    fn options_and_folder_checks() {
        let mut s = Setup::default();
        assert!(s.options().skip_duplicates && !s.options().delete_archives);
        s.keep_duplicates = true;
        s.delete_archives = true;
        assert!(!s.options().skip_duplicates && s.options().delete_archives);
        assert_eq!(s.folder_problem(), Some("Choose the archives folder."));
        s.archives_dir = Some("/home/rob/Downloads".into());
        assert_eq!(s.folder_problem(), Some("Choose the library folder."));
        s.library_dir = Some("/home/rob/Downloads".into());
        assert!(s.folder_problem().unwrap().contains("two different"));
        s.library_dir = Some("/home/rob/Downloads/GooglePhotosLibrary".into());
        assert_eq!(
            s.folder_problem(),
            None,
            "a library inside Downloads is fine"
        );
        s.library_dir = Some("/home/rob".into());
        assert!(s.folder_problem().unwrap().contains("inside the library"));
        s.library_dir = Some("/data/GooglePhotosLibrary".into());
        assert_eq!(s.folder_problem(), None);
        assert_eq!(step_label(0, 8), "Step 1 of 8");
    }

    #[test]
    fn progress_words() {
        let p = UnpackProgress {
            archive: 2,
            archives: 3,
            files: 120,
            bytes: 1_000_000_000,
            bytes_total: 4_000_000_000,
        };
        let (f, t) = progress_text(&p, Duration::from_secs(10));
        assert_eq!(f, 0.25);
        assert_eq!(
            t,
            "Part 2 of 3 · 120 files · 1.0 GB of 4.0 GB · 100.0 MB/s · under a minute left"
        );
        let (f, t) = progress_text(&UnpackProgress::default(), Duration::ZERO);
        assert_eq!(f, 0.0);
        assert_eq!(t, "Part 1 of 0 · 0 files · 0 bytes of 0 bytes");
    }

    #[test]
    fn report_words() {
        use unswiper_takeout::UnpackReport;
        let r = UnpackReport {
            extracted: 10,
            bytes_written: 2_000_000_000,
            duplicates_skipped: 2,
            previously_imported: 3,
            conflicts: vec!["Trip/IMG.jpg".into()],
            ..Default::default()
        };
        let l = report_lines(&r).join("\n");
        assert!(l.contains("Unpacked 10 files (2.0 GB)."));
        assert!(l.contains("3 files were already imported"));
        assert!(l.contains("2 album copies"));
        assert!(l.contains("Trip/IMG.jpg"));
    }

    #[test]
    fn takeout_libraries_use_sidecars() {
        let d = tempfile::tempdir().unwrap();
        let y = d.path().join("Photos from 2019");
        std::fs::create_dir(&y).unwrap();
        std::fs::write(y.join("IMG.png"), b"not really a png").unwrap();
        std::fs::write(
            y.join("IMG.png.supplemental-metadata.json"),
            br#"{"photoTakenTime":{"timestamp":"1561190459"}}"#,
        )
        .unwrap();
        assert_eq!(dates_for(d.path()).photo_date(&y.join("IMG.png")), None);
        std::fs::write(d.path().join(unswiper_takeout::unpack::STATE_FILE), b"{}").unwrap();
        assert_eq!(
            dates_for(d.path())
                .photo_date(&y.join("IMG.png"))
                .map(|t| t.timestamp()),
            Some(1_561_190_459)
        );
    }
}
