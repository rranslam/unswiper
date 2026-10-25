// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// The step texts of "Import from Google Photos" (plan §19.4). `Resources/Takeout/takeout-guide.json`
/// is a byte-identical copy of crates/unswiper-takeout/src/takeout-guide.json (a Rust test
/// checks), so both apps show the same steps.
struct TakeoutGuide: Codable, Equatable {
    struct Step: Codable, Equatable, Identifiable {
        var id: String
        var title: String
        /// `open_takeout`, `choose_folders`, `unpack`, `index` or `none`.
        var action: String
        /// Markdown.
        var body: String
    }

    var version: Int
    var title: String
    var takeoutUrl: String
    var fallbackUrl: String
    var steps: [Step]

    static let fileName = "takeout-guide.json"

    static func url() -> URL? {
        guard let b = HelpLibrary.resourceBundle else { return nil }
        let roots = [b.resourceURL, b.bundleURL, b.bundleURL.appendingPathComponent("Contents/Resources")].compactMap { $0 }
        for root in roots {
            let f = root.appendingPathComponent("Takeout", isDirectory: true).appendingPathComponent(fileName)
            if FileManager.default.fileExists(atPath: f.path) { return f }
        }
        return nil
    }

    static func decode(_ data: Data) throws -> TakeoutGuide {
        try JSONDecoder().decode(TakeoutGuide.self, from: data)
    }

    /// The bundled guide, or a one-step stand-in if the resource is missing (broken install).
    static let shared: TakeoutGuide = {
        if let u = url(), let d = try? Data(contentsOf: u), let g = try? decode(d) { return g }
        return TakeoutGuide(
            version: 0, title: "Import from Google Photos",
            takeoutUrl: "https://takeout.google.com/", fallbackUrl: "https://takeout.google.com/",
            steps: [Step(id: "missing", title: "Guide unavailable", action: "none",
                         body: "The guide could not be found. Please reinstall Unswiper.")])
    }()
}
