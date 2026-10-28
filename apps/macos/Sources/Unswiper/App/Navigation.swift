// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// What the window shows. `.setup` fills the window; everything else is the split view:
/// the visits list on the left, and on the right nothing (`.home`), a visit's grid or large
/// photo (`.reduce`), its Review & commit page (`.commit`) or Compare (`.compare`).
enum AppPath: Hashable {
    case home
    case setup
    case reduce(String)
    case commit(String)
    case compare(String, [String])

    /// The visit whose page is open on the right, if any (highlighted in the list).
    var visitID: String? {
        switch self {
        case .reduce(let id), .commit(let id), .compare(let id, _): return id
        case .home, .setup: return nil
        }
    }

    /// Setup replaces the whole window; the other pages sit beside the visits list.
    var isFullWindow: Bool { self == .setup }

    /// Where a click in the visits list leads: another visit opens fresh on its grid;
    /// clicking the visit already open leaves its page (commit, compare) as it is;
    /// clearing the selection shows the placeholder.
    func selecting(_ id: String?) -> AppPath {
        guard let id else { return isFullWindow ? self : .home }
        return id == visitID ? self : .reduce(id)
    }

    /// After the visits were rebuilt: a page for a visit that no longer exists closes.
    func keeping(visits ids: Set<String>) -> AppPath {
        guard let id = visitID, !ids.contains(id) else { return self }
        return .home
    }
}
