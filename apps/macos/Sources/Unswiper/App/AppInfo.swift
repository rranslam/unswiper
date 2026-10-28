// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// Name, version and links shown in About Unswiper (the same as the Linux About dialog).
enum AppInfo {
    static let name = "Unswiper"
    /// Used when there's no Info.plist (`swift run`). Keep equal to `version` in the root
    /// Cargo.toml: `cargo test -p unswiper-core --test mac_version` fails otherwise, and
    /// build-dmg.sh stamps Info.plist with the Cargo.toml version.
    static let fallbackVersion = "0.6.1"
    static let developer = "Rob Ranslam"
    static let copyright = "© 2026 Rob Ranslam"
    static let comments = "Sort trip photos into visits, keep the best, set aside the rest."
    static let license = "MIT License"
    static let website = URL(string: "https://github.com/rranslam/unswiper")!
    static let issues = URL(string: "https://github.com/rranslam/unswiper/issues")!

    /// The version in Info.plist (the .app), else `fallbackVersion`.
    static func version(info: [String: Any]?) -> String {
        if let v = info?["CFBundleShortVersionString"] as? String, !v.isEmpty { return v }
        return fallbackVersion
    }

    static var version: String { version(info: Bundle.main.infoDictionary) }

    /// Plain-text credits; `AboutPanel` turns the URLs into links.
    static func creditsLines(notices: String) -> [String] {
        [comments, "", "\(copyright). \(license).", "Developer: \(developer)",
         website.absoluteString, "Report an issue: \(issues.absoluteString)", "", notices]
    }
}
