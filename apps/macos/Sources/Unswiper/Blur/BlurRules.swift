// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// Blur-suggestion rules, a direct port of `unswiper_core::quality` (plan §15.3) so Mac and Linux
/// read and write the same library.json keys the same way:
/// suggestions only touch Undecided items, the user's decision always wins, and nothing moves
/// until the visit is committed.
enum BlurRules {
    static let sourceAutoBlur = "auto-blur"

    enum Sensitivity: String, CaseIterable, Identifiable {
        case low, normal, high
        var id: String { rawValue }
        var label: String {
            switch self {
            case .low: return "Low (only very blurry)"
            case .normal: return "Normal"
            case .high: return "High (more suggestions)"
            }
        }
    }

    static func enabled(_ c: AppConfig) -> Bool { c.blurDetection ?? true }

    static func setEnabled(_ c: inout AppConfig, _ on: Bool) {
        c.blurDetection = on ? nil : false
    }

    static func sensitivity(_ c: AppConfig) -> Sensitivity {
        c.blurSensitivity.flatMap(Sensitivity.init(rawValue:)) ?? .normal
    }

    static func setSensitivity(_ c: inout AppConfig, _ s: Sensitivity) {
        c.blurSensitivity = s == .normal ? nil : s.rawValue
    }

    /// The item's Reject is an unconfirmed blur suggestion.
    static func isSuggested(_ i: MediaItem) -> Bool {
        i.decision == .reject && i.decisionSource == sourceAutoBlur
    }

    static func isDismissed(_ i: MediaItem) -> Bool { i.blurDismissed ?? false }

    /// A decision made by the user. Clears the suggestion marker; clearing or keeping a suggestion
    /// records `blurDismissed` so rescans leave the photo alone. Returns whether anything changed.
    @discardableResult
    static func setManualDecision(_ i: inout MediaItem, _ d: Decision) -> Bool {
        let wasSuggested = isSuggested(i)
        var changed = i.decision != d
        if i.decisionSource != nil {
            i.decisionSource = nil
            changed = true
        }
        if wasSuggested, d != .reject, !isDismissed(i) {
            i.blurDismissed = true
            changed = true
        }
        i.decision = d
        return changed
    }

    struct ApplyReport: Equatable {
        var suggested = 0
        var withdrawn = 0
    }

    /// Brings suggestions in line with `flagged` (paths the detector calls blurry). `scored`
    /// limits withdrawals to items the detector looked at.
    @discardableResult
    static func apply(_ items: inout [MediaItem], flagged: Set<String>, scored: Set<String>, enabled: Bool) -> ApplyReport {
        var r = ApplyReport()
        for k in items.indices {
            if isSuggested(items[k]) {
                let keep = enabled && (flagged.contains(items[k].path) || !scored.contains(items[k].path))
                if !keep {
                    items[k].decision = .undecided
                    items[k].decisionSource = nil
                    r.withdrawn += 1
                }
            } else if enabled, items[k].decision == .undecided, !isDismissed(items[k]),
                      flagged.contains(items[k].path) {
                items[k].decision = .reject
                items[k].decisionSource = sourceAutoBlur
                r.suggested += 1
            }
        }
        return r
    }

    /// Withdraws every suggestion (setting turned off). Dismissals are kept.
    @discardableResult
    static func withdrawAll(_ items: inout [MediaItem]) -> Int {
        apply(&items, flagged: [], scored: [], enabled: false).withdrawn
    }

    /// Re-attaches suggestion state after a re-index (which only carries plain decisions).
    static func carryOver(from old: [MediaItem], to new: inout [MediaItem]) {
        let byPath = Dictionary(old.map { ($0.path, $0) }, uniquingKeysWith: { a, _ in a })
        for k in new.indices {
            guard let o = byPath[new[k].path] else { continue }
            if isDismissed(o) { new[k].blurDismissed = true }
            if isSuggested(o), new[k].decision == .reject { new[k].decisionSource = sourceAutoBlur }
        }
    }

    /// Confirms every suggestion among `paths` as the user's own Reject.
    @discardableResult
    static func confirm(_ items: inout [MediaItem], paths: Set<String>) -> Int {
        var n = 0
        for k in items.indices where paths.contains(items[k].path) && isSuggested(items[k]) {
            items[k].decisionSource = nil
            n += 1
        }
        return n
    }

    /// Clears every suggestion among `paths` (Undecided + dismissed, like pressing U).
    @discardableResult
    static func dismiss(_ items: inout [MediaItem], paths: Set<String>) -> Int {
        var n = 0
        for k in items.indices where paths.contains(items[k].path) && isSuggested(items[k]) {
            setManualDecision(&items[k], .undecided)
            n += 1
        }
        return n
    }
}
