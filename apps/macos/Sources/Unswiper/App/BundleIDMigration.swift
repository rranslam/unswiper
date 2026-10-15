// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// One-time move of preferences from the old bundle ID.
///
/// Up to 0.1.2 the app was `com.rranslam.unswiper`. From 0.2.0 it is `io.github.rranslam.Unswiper`
/// (the same ID as the Linux app). Library data lives in `~/Library/Application Support/Unswiper`,
/// which does not depend on the bundle ID, so libraries, decisions and commit records carry over
/// untouched. Only the `UserDefaults` domain (window sizes and other SwiftUI/AppKit state) is keyed
/// by the bundle ID; this copies it once.
enum BundleIDMigration {
    static let legacyID = "com.rranslam.unswiper"
    static let currentID = "io.github.rranslam.Unswiper"
    static let doneKey = "UnswiperMigratedFromLegacyBundleID"

    /// Copies keys from `legacy` into `target` without overwriting existing keys. Runs once per
    /// `target` (guarded by `doneKey`). Returns how many keys were copied.
    @discardableResult
    static func migrateDefaults(from legacy: [String: Any]?, into target: UserDefaults) -> Int {
        if target.bool(forKey: doneKey) {
            return 0
        }
        var copied = 0
        for (key, value) in legacy ?? [:] where target.object(forKey: key) == nil {
            target.set(value, forKey: key)
            copied += 1
        }
        target.set(true, forKey: doneKey)
        return copied
    }

    /// Called at launch. Does nothing in `swift run`/tests, where the bundle ID differs.
    static func runAtLaunch() {
        guard Bundle.main.bundleIdentifier == currentID else { return }
        migrateDefaults(from: UserDefaults.standard.persistentDomain(forName: legacyID), into: .standard)
    }
}
