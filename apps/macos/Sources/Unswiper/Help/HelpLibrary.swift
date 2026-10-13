// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation
import SwiftUI

struct HelpTopic: Identifiable, Hashable {
    let id: String       // file name without extension, e.g. "03-reducing"
    let title: String
    let markdown: String
}

enum HelpLibrary {
    static let bundleName = "Unswiper_Unswiper.bundle"
    static let issuesURL = URL(string: "https://github.com/rranslam/visit-media-reduce/issues")!
    static let releasesURL = URL(string: "https://github.com/rranslam/visit-media-reduce/releases")!

    /// Locates the SwiftPM resource bundle without using `Bundle.module`, which
    /// would crash in the packaged .app (it only looks next to the .app itself).
    static var resourceBundle: Bundle? {
        var candidates: [URL] = []
        if let r = Bundle.main.resourceURL { candidates.append(r.appendingPathComponent(bundleName)) }
        candidates.append(Bundle.main.bundleURL.appendingPathComponent(bundleName))
        if let exe = Bundle.main.executableURL {
            candidates.append(exe.deletingLastPathComponent().appendingPathComponent(bundleName))
        }
        for url in candidates where FileManager.default.fileExists(atPath: url.path) {
            if let b = Bundle(url: url) { return b }
        }
        return nil
    }

    static var helpDirectory: URL? {
        guard let b = resourceBundle else { return nil }
        let roots = [b.resourceURL, b.bundleURL, b.bundleURL.appendingPathComponent("Contents/Resources")].compactMap { $0 }
        for root in roots {
            let dir = root.appendingPathComponent("Help", isDirectory: true)
            if FileManager.default.fileExists(atPath: dir.path) { return dir }
        }
        return nil
    }

    static let topics: [HelpTopic] = {
        guard let dir = helpDirectory,
              let files = try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)
        else {
            return [HelpTopic(id: "missing", title: "Help Unavailable",
                              markdown: "# Help Unavailable\n\nThe help files could not be found. Please reinstall Unswiper, or read the guide online at https://github.com/rranslam/visit-media-reduce")]
        }
        return files.filter { $0.pathExtension == "md" }
            .sorted { $0.lastPathComponent < $1.lastPathComponent }
            .compactMap { url in
                guard let text = try? String(contentsOf: url, encoding: .utf8) else { return nil }
                let title = text.split(separator: "\n").first { $0.hasPrefix("# ") }
                    .map { String($0.dropFirst(2)) } ?? url.deletingPathExtension().lastPathComponent
                return HelpTopic(id: url.deletingPathExtension().lastPathComponent, title: title, markdown: text)
            }
    }()

    static func topicID(containing key: String) -> String? {
        topics.first { $0.id.contains(key) }?.id
    }
}

/// Shared selection so menu commands can open Help at a specific topic.
@MainActor
final class HelpRouter: ObservableObject {
    static let shared = HelpRouter()
    @Published var selection: String? = HelpLibrary.topics.first?.id
}
