// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! The guided "Import from Google Photos" setup (plan §19.4): the step texts, shared by both apps.
//! The Mac app has a byte-identical copy of `takeout-guide.json` in its resources (a test checks).

use serde::Deserialize;

/// The guide as shipped (`src/takeout-guide.json`).
pub const GUIDE_JSON: &str = include_str!("takeout-guide.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Guide {
    pub version: u32,
    pub title: String,
    /// Takeout with Google Photos preselected (`custom/photos`, the path Google Photos' own
    /// Settings ▸ Export link uses). The 2-month repeat can't be preselected (Rob checked
    /// `frequency=2_months` on 19 Oct 2026), so the guide tells the user to pick it by hand.
    pub takeout_url: String,
    /// Plain takeout.google.com if the prefilled link ever stops working.
    pub fallback_url: String,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Step {
    pub id: String,
    pub title: String,
    /// What the step's controls do: `open_takeout`, `choose_folders`, `unpack`, `index` or `none`.
    pub action: String,
    /// Markdown: paragraphs, `1.` lists and `**bold**`.
    pub body: String,
}

/// The parsed guide.
pub fn guide() -> Guide {
    serde_json::from_str(GUIDE_JSON).expect("takeout-guide.json is valid (tested)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_steps_in_order() {
        let g = guide();
        let ids: Vec<_> = g.steps.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "open-takeout",
                "choose-photos",
                "choose-options",
                "wait-email",
                "download",
                "choose-folders",
                "unpack",
                "index"
            ]
        );
        for s in &g.steps {
            assert!(
                ["open_takeout", "choose_folders", "unpack", "index", "none"]
                    .contains(&s.action.as_str()),
                "{}",
                s.id
            );
            assert!(!s.body.is_empty() && !s.title.is_empty());
        }
    }

    #[test]
    fn beginner_wording_names_googles_options() {
        let all: String = guide().steps.iter().map(|s| s.body.clone()).collect();
        for must in [
            "Deselect all",
            "Google Photos",
            "Next step",
            "Send download link via email",
            "Export every 2 months for 1 year",
            "Export once",
            "by hand",
            ".zip",
            "50 GB",
            "Create export",
            "7 days",
            "Delete each archive after unpacking",
            "Also unpack album copies",
        ] {
            assert!(all.contains(must), "guide misses {must:?}");
        }
    }

    #[test]
    fn links() {
        let g = guide();
        assert_eq!(
            g.takeout_url,
            "https://takeout.google.com/settings/takeout/custom/photos"
        );
        assert!(
            !g.takeout_url.contains('?'),
            "no URL parameters: they aren't honoured"
        );
        assert_eq!(g.fallback_url, "https://takeout.google.com/");
    }

    #[test]
    fn mac_copy_is_identical() {
        let mac = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../apps/macos/Sources/Unswiper/Resources/Takeout/takeout-guide.json"
        );
        match std::fs::read_to_string(mac) {
            Ok(m) => assert_eq!(m, GUIDE_JSON, "copy src/takeout-guide.json to {mac}"),
            Err(e) => panic!("{mac}: {e}"),
        }
    }
}
