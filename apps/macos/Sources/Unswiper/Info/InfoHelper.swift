// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// Photo info (plan §17) from the bundled helper: `unswiper-blur info` reads EXIF/QuickTime with
/// the shared Rust reader and returns rows already formatted, so the Mac and Linux apps show the
/// same wording. The helper only reads; the location goes to its stdout and nowhere else.
enum InfoHelper {
    /// One formatted row (the JSON of `unswiper_core::meta::info::Row`).
    struct Row: Codable, Equatable, Hashable, Sendable {
        var group: String
        var label: String
        var value: String
        /// Also shown in the compact right-click popover.
        var short: Bool
    }

    /// The answer for one file.
    struct Answer: Codable, Equatable, Sendable {
        var path: String
        var rows: [Row]
        /// One-line summary for the Compare strip.
        var strip: String
    }

    /// Settings that change the rows. Cached answers are only reused for the same settings.
    struct Settings: Equatable, Hashable, Sendable {
        var showLocation: Bool
        var blurEnabled: Bool
        var sensitivity: BlurRules.Sensitivity
    }

    static func arguments(files: [String], library: URL?, settings: Settings) -> [String] {
        var a = ["info"]
        if let library { a += ["--library", library.path] }
        a += ["--sensitivity", settings.sensitivity.rawValue]
        if !settings.blurEnabled { a.append("--blur-off") }
        if !settings.showLocation { a.append("--hide-location") }
        return a + files
    }

    static func parse(_ data: Data) throws -> [Answer] {
        try JSONDecoder().decode([Answer].self, from: data)
    }

    /// Runs the helper with `arguments` and returns its stdout. Blocking: call it off the main thread.
    static func runHelper(_ helper: URL, arguments: [String]) throws -> Data {
        let p = Process()
        p.executableURL = helper
        p.arguments = arguments
        let out = Pipe(), err = Pipe()
        p.standardOutput = out
        p.standardError = err
        try p.run()
        let data = out.fileHandleForReading.readDataToEndOfFile()
        let errData = err.fileHandleForReading.readDataToEndOfFile()
        p.waitUntilExit()
        guard p.terminationStatus == 0 else {
            let msg = String(data: errData, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            throw BlurHelper.HelperError.failed(msg.isEmpty ? "exit code \(p.terminationStatus)" : msg)
        }
        return data
    }

    /// Info for `files`. Blocking: call it off the main thread.
    static func run(helper: URL, files: [String], library: URL?, settings: Settings) throws -> [Answer] {
        try parse(runHelper(helper, arguments: arguments(files: files, library: library, settings: settings)))
    }
}
