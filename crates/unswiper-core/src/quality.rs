// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Blur suggestions in the library (plan §15.2 and §15.3).
//!
//! Scores live in a rebuildable sidecar cache (`quality-cache.json`, written by the
//! `unswiper-blur` crate). library.json only records the *suggestion state*, in optional
//! keys that older readers ignore and that are omitted when unused, so files without blur
//! data stay byte-identical to schema v1:
//!
//! * item `decisionSource: "auto-blur"`: the item's Reject came from blur detection.
//! * item `blurDismissed: true`: the user cleared a suggestion; rescans never re-flag it.
//! * config `blurDetection: false`: the setting is off (missing = on, the default).
//! * config `blurSensitivity: "low" | "normal" | "high"` (missing = normal).
//!
//! Rules: suggestions only ever touch Undecided items, manual decisions always win, and
//! nothing moves until the user commits the open visit.

use crate::model::{AppConfig, Decision, MediaItem};
use serde_json::Value;
use std::collections::HashSet;

pub const KEY_SOURCE: &str = "decisionSource";
pub const KEY_DISMISSED: &str = "blurDismissed";
pub const SOURCE_AUTO_BLUR: &str = "auto-blur";
pub const KEY_ENABLED: &str = "blurDetection";
pub const KEY_SENSITIVITY: &str = "blurSensitivity";

/// How eagerly photos are suggested as blurry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sensitivity {
    Low,
    #[default]
    Normal,
    High,
}

impl Sensitivity {
    pub const ALL: [Sensitivity; 3] = [Sensitivity::Low, Sensitivity::Normal, Sensitivity::High];

    pub fn as_str(self) -> &'static str {
        match self {
            Sensitivity::Low => "low",
            Sensitivity::Normal => "normal",
            Sensitivity::High => "high",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Sensitivity::Low => "Low (only very blurry)",
            Sensitivity::Normal => "Normal",
            Sensitivity::High => "High (more suggestions)",
        }
    }

    pub fn parse(s: &str) -> Option<Sensitivity> {
        Sensitivity::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// Whether "Detect blurry photos" is on (default on).
pub fn enabled(c: &AppConfig) -> bool {
    c.extra
        .get(KEY_ENABLED)
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// Stores the setting; the default (on) is stored as an absent key.
pub fn set_enabled(c: &mut AppConfig, on: bool) {
    if on {
        c.extra.remove(KEY_ENABLED);
    } else {
        c.extra.insert(KEY_ENABLED.into(), Value::Bool(false));
    }
}

pub fn sensitivity(c: &AppConfig) -> Sensitivity {
    c.extra
        .get(KEY_SENSITIVITY)
        .and_then(Value::as_str)
        .and_then(Sensitivity::parse)
        .unwrap_or_default()
}

pub fn set_sensitivity(c: &mut AppConfig, s: Sensitivity) {
    if s == Sensitivity::Normal {
        c.extra.remove(KEY_SENSITIVITY);
    } else {
        c.extra
            .insert(KEY_SENSITIVITY.into(), Value::String(s.as_str().into()));
    }
}

/// The item's Reject is an (unconfirmed) blur suggestion.
pub fn is_suggested(i: &MediaItem) -> bool {
    i.decision == Decision::Reject
        && i.extra.get(KEY_SOURCE).and_then(Value::as_str) == Some(SOURCE_AUTO_BLUR)
}

pub fn is_dismissed(i: &MediaItem) -> bool {
    i.extra
        .get(KEY_DISMISSED)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// A decision made by the user. It always wins: it clears the suggestion marker, and
/// clearing a suggestion (Undecided) or keeping it records `blurDismissed` so rescans
/// leave the photo alone. Returns whether anything changed.
pub fn set_manual_decision(i: &mut MediaItem, d: Decision) -> bool {
    let was_suggested = is_suggested(i);
    let mut changed = i.decision != d;
    if i.extra.remove(KEY_SOURCE).is_some() {
        changed = true;
    }
    if was_suggested && d != Decision::Reject && !is_dismissed(i) {
        i.extra.insert(KEY_DISMISSED.into(), Value::Bool(true));
        changed = true;
    }
    i.decision = d;
    changed
}

/// What [`apply_suggestions`] changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ApplyReport {
    pub suggested: usize,
    pub withdrawn: usize,
}

/// Brings suggestions in line with `flagged` (paths the detector calls blurry).
///
/// * Undecided, not dismissed, flagged → Reject with source auto-blur.
/// * Suggested but no longer flagged (or `enabled` is false) → back to Undecided.
/// * Everything else (Keep, manual Reject, dismissed) is never touched.
///
/// `scored` limits withdrawals to items the detector actually looked at, so a partial
/// scan (open visit first) doesn't withdraw suggestions elsewhere.
pub fn apply_suggestions(
    items: &mut [MediaItem],
    flagged: &HashSet<String>,
    scored: &HashSet<String>,
    enabled: bool,
) -> ApplyReport {
    let mut r = ApplyReport::default();
    for i in items.iter_mut() {
        if is_suggested(i) {
            let keep = enabled && (flagged.contains(&i.path) || !scored.contains(&i.path));
            if !keep {
                i.decision = Decision::Undecided;
                i.extra.remove(KEY_SOURCE);
                r.withdrawn += 1;
            }
        } else if enabled
            && i.decision == Decision::Undecided
            && !is_dismissed(i)
            && flagged.contains(&i.path)
        {
            i.decision = Decision::Reject;
            i.extra
                .insert(KEY_SOURCE.into(), Value::String(SOURCE_AUTO_BLUR.into()));
            r.suggested += 1;
        }
    }
    r
}

/// Withdraws every suggestion (setting turned off). Dismissals and scores are kept.
pub fn withdraw_all(items: &mut [MediaItem]) -> usize {
    apply_suggestions(items, &HashSet::new(), &HashSet::new(), false).withdrawn
}

/// Re-attaches suggestion state after a re-index or folder-watch rebuild, which only
/// carries plain decisions: by path, the dismissal always, and the auto-blur source only
/// while the item is still rejected.
pub fn carry_over(old: &[MediaItem], new: &mut [MediaItem]) {
    let by_path: std::collections::HashMap<&str, &MediaItem> =
        old.iter().map(|i| (i.path.as_str(), i)).collect();
    for n in new.iter_mut() {
        let Some(o) = by_path.get(n.path.as_str()) else {
            continue;
        };
        if is_dismissed(o) {
            n.extra.insert(KEY_DISMISSED.into(), Value::Bool(true));
        }
        if is_suggested(o) && n.decision == Decision::Reject {
            n.extra
                .insert(KEY_SOURCE.into(), Value::String(SOURCE_AUTO_BLUR.into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MediaKind;
    use crate::time::parse_iso;
    use serde_json::Map;

    fn item(path: &str, d: Decision) -> MediaItem {
        MediaItem {
            path: path.into(),
            kind: MediaKind::Photo,
            size_bytes: 1,
            taken_at: parse_iso("2026-01-01T00:00:00Z").unwrap(),
            relative_folder: "x".into(),
            decision: d,
            modified_at: None,
            sidecars: vec![],
            is_undated: false,
            date_source: None,
            folder_path: None,
            extra: Map::new(),
        }
    }

    fn set(v: &[&str]) -> HashSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn config_defaults_and_round_trip() {
        let mut c = AppConfig::default();
        assert!(enabled(&c));
        assert_eq!(sensitivity(&c), Sensitivity::Normal);
        set_enabled(&mut c, false);
        set_sensitivity(&mut c, Sensitivity::High);
        assert!(!enabled(&c));
        assert_eq!(sensitivity(&c), Sensitivity::High);
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["blurDetection"], false);
        assert_eq!(v["blurSensitivity"], "high");
        set_enabled(&mut c, true);
        set_sensitivity(&mut c, Sensitivity::Normal);
        assert!(c.extra.is_empty(), "defaults are stored as absent keys");
        for s in Sensitivity::ALL {
            assert_eq!(Sensitivity::parse(s.as_str()), Some(s));
            assert!(!s.label().is_empty());
        }
        assert_eq!(Sensitivity::parse("max"), None);
    }

    #[test]
    fn suggestions_touch_only_undecided() {
        let mut items = vec![
            item("/u", Decision::Undecided),
            item("/k", Decision::Keep),
            item("/r", Decision::Reject),
            item("/sharp", Decision::Undecided),
        ];
        let flagged = set(&["/u", "/k", "/r"]);
        let scored = set(&["/u", "/k", "/r", "/sharp"]);
        let r = apply_suggestions(&mut items, &flagged, &scored, true);
        assert_eq!(
            r,
            ApplyReport {
                suggested: 1,
                withdrawn: 0
            }
        );
        assert!(is_suggested(&items[0]));
        assert_eq!(items[1].decision, Decision::Keep);
        assert!(!is_suggested(&items[2]), "manual reject stays manual");
        assert_eq!(items[3].decision, Decision::Undecided);
        // Idempotent.
        let r = apply_suggestions(&mut items, &flagged, &scored, true);
        assert_eq!(r, ApplyReport::default());
    }

    #[test]
    fn rescans_withdraw_only_scored_items() {
        let mut items = vec![
            item("/a", Decision::Undecided),
            item("/b", Decision::Undecided),
        ];
        apply_suggestions(&mut items, &set(&["/a", "/b"]), &set(&["/a", "/b"]), true);
        // A partial scan that saw only /a (now sharp) leaves /b alone.
        let r = apply_suggestions(&mut items, &set(&[]), &set(&["/a"]), true);
        assert_eq!(r.withdrawn, 1);
        assert_eq!(items[0].decision, Decision::Undecided);
        assert!(!items[0].extra.contains_key(KEY_SOURCE));
        assert!(is_suggested(&items[1]));
        assert_eq!(withdraw_all(&mut items), 1);
        assert!(items.iter().all(|i| i.decision == Decision::Undecided));
    }

    #[test]
    fn disabled_never_suggests() {
        let mut items = vec![item("/a", Decision::Undecided)];
        let r = apply_suggestions(&mut items, &set(&["/a"]), &set(&["/a"]), false);
        assert_eq!(r, ApplyReport::default());
        assert_eq!(items[0].decision, Decision::Undecided);
    }

    #[test]
    fn manual_decisions_win_and_dismiss() {
        let mut items = vec![
            item("/a", Decision::Undecided),
            item("/b", Decision::Undecided),
        ];
        let all = set(&["/a", "/b"]);
        apply_suggestions(&mut items, &all, &all, true);
        // U on a suggestion: cleared and remembered.
        assert!(set_manual_decision(&mut items[0], Decision::Undecided));
        assert!(is_dismissed(&items[0]));
        assert!(!set_manual_decision(&mut items[0], Decision::Undecided));
        // R confirms: becomes a manual reject, not dismissed.
        assert!(set_manual_decision(&mut items[1], Decision::Reject));
        assert!(!is_suggested(&items[1]) && !is_dismissed(&items[1]));
        assert_eq!(items[1].decision, Decision::Reject);
        // Rescan doesn't bring /a back and doesn't touch the manual reject.
        let r = apply_suggestions(&mut items, &all, &all, true);
        assert_eq!(r, ApplyReport::default());
        assert_eq!(items[0].decision, Decision::Undecided);
        // A plain manual change on an unsuggested item reports change only if different.
        let mut k = item("/k", Decision::Keep);
        assert!(!set_manual_decision(&mut k, Decision::Keep));
        assert!(set_manual_decision(&mut k, Decision::Reject));
        assert!(!is_dismissed(&k));
    }

    #[test]
    fn carry_over_keeps_state_by_path() {
        let mut old = vec![
            item("/a", Decision::Undecided),
            item("/b", Decision::Undecided),
            item("/c", Decision::Undecided),
        ];
        let all = set(&["/a", "/b", "/c"]);
        apply_suggestions(&mut old, &all, &all, true);
        set_manual_decision(&mut old[1], Decision::Undecided); // dismissed
                                                               // A rebuild carries only decisions.
        let mut new = vec![
            item("/a", Decision::Reject),
            item("/b", Decision::Undecided),
            item("/c", Decision::Keep), // changed elsewhere: no longer a suggestion
            item("/new", Decision::Undecided),
        ];
        carry_over(&old, &mut new);
        assert!(is_suggested(&new[0]));
        assert!(is_dismissed(&new[1]));
        assert!(!is_suggested(&new[2]) && !new[2].extra.contains_key(KEY_SOURCE));
        assert!(new[3].extra.is_empty());
    }

    #[test]
    fn json_keys_and_v1_compat() {
        let mut items = vec![item("/a", Decision::Undecided)];
        let plain = serde_json::to_value(&items[0]).unwrap();
        assert!(plain.get(KEY_SOURCE).is_none() && plain.get(KEY_DISMISSED).is_none());
        apply_suggestions(&mut items, &set(&["/a"]), &set(&["/a"]), true);
        let v = serde_json::to_value(&items[0]).unwrap();
        assert_eq!(v["decision"], "reject");
        assert_eq!(v["decisionSource"], "auto-blur");
        let back: MediaItem = serde_json::from_value(v).unwrap();
        assert!(is_suggested(&back));
    }
}
