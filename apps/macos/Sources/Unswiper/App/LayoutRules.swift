// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// Room for the visits list, the photos and Photo Info side by side.
enum LayoutRules {
    static let sidebarMin: Double = 280
    /// The photo grid (or large photo) never gets narrower than this.
    static let contentMin: Double = 420
    static let inspectorIdeal: Double = 300

    /// Whether list + photos + Photo Info fit in a window this wide with usable photos.
    static func roomForAll(windowWidth: Double) -> Bool {
        windowWidth >= sidebarMin + contentMin + 140 + inspectorIdeal  // 140: a few more thumbnails
    }

    /// When Photo Info opens in a window too narrow for all three, the visits list is hidden;
    /// when it closes, the list comes back if Unswiper hid it. Runs on the open/close click,
    /// never during layout. Returns (show the list, hidden by Unswiper).
    static func sidebar(inspectorOpen: Bool, windowWidth: Double,
                        sidebarShown: Bool, autoHidden: Bool) -> (show: Bool, autoHidden: Bool) {
        if inspectorOpen {
            if sidebarShown, !roomForAll(windowWidth: windowWidth) { return (false, true) }
            return (sidebarShown, autoHidden)
        }
        return autoHidden ? (true, false) : (sidebarShown, false)
    }
}
