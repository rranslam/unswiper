// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// Runs the bundled `unswiper-blur` command-line helper (the same Rust detector as Linux) on
/// library.json and reads its verdict. The helper only reads photos and writes its score cache
/// (`quality-cache.json` next to library.json); it never moves files or edits library.json.
enum BlurHelper {
    struct Row: Decodable, Equatable {
        var path: String
        var blurry: Bool
        var reason: String?
    }

    struct Verdict: Equatable {
        var flagged: Set<String>
        var scored: Set<String>
    }

    enum HelperError: LocalizedError {
        case notFound
        case failed(String)
        var errorDescription: String? {
            switch self {
            case .notFound: return "The blur checker (unswiper-blur) is missing from Unswiper.app."
            case .failed(let s): return "The blur checker failed: \(s)"
            }
        }
    }

    /// Where the helper is: `UNSWIPER_BLUR_HELPER`, then next to the app's executable
    /// (Unswiper.app/Contents/MacOS/unswiper-blur), then a `target/release` build in a source
    /// checkout above the executable (for `swift run` during development).
    static func locate(environment: [String: String] = ProcessInfo.processInfo.environment,
                       executable: URL? = Bundle.main.executableURL) -> URL? {
        let fm = FileManager.default
        if let p = environment["UNSWIPER_BLUR_HELPER"], fm.isExecutableFile(atPath: p) {
            return URL(fileURLWithPath: p)
        }
        guard let exe = executable?.resolvingSymlinksInPath() else { return nil }
        let sibling = exe.deletingLastPathComponent().appendingPathComponent("unswiper-blur")
        if fm.isExecutableFile(atPath: sibling.path) { return sibling }
        var dir = exe.deletingLastPathComponent()
        for _ in 0..<8 {
            let dev = dir.appendingPathComponent("target/release/unswiper-blur")
            if fm.isExecutableFile(atPath: dev.path) { return dev }
            dir = dir.deletingLastPathComponent()
        }
        return nil
    }

    static func arguments(library: URL, sensitivity: BlurRules.Sensitivity, onlyList: URL?) -> [String] {
        var a = ["scan", library.path, "--json", "--sensitivity", sensitivity.rawValue, "--write-cache"]
        if let onlyList { a += ["--only", onlyList.path] }
        return a
    }

    static func parse(_ data: Data) throws -> Verdict {
        let rows = try JSONDecoder().decode([Row].self, from: data)
        return Verdict(flagged: Set(rows.filter(\.blurry).map(\.path)), scored: Set(rows.map(\.path)))
    }

    /// Scores (cached photos are not decoded again) and returns the verdict for every photo with
    /// a score. `only` limits the new scoring to those paths, e.g. the open visit. Blocking: call
    /// it off the main thread.
    static func run(helper: URL, library: URL, sensitivity: BlurRules.Sensitivity, only: [String]?) throws -> Verdict {
        var list: URL?
        if let only {
            let url = FileManager.default.temporaryDirectory
                .appendingPathComponent("unswiper-blur-only-\(UUID().uuidString).txt")
            try only.joined(separator: "\n").write(to: url, atomically: true, encoding: .utf8)
            list = url
        }
        defer { if let list { try? FileManager.default.removeItem(at: list) } }
        let p = Process()
        p.executableURL = helper
        p.arguments = arguments(library: library, sensitivity: sensitivity, onlyList: list)
        let out = Pipe(), err = Pipe()
        p.standardOutput = out
        p.standardError = err
        try p.run()
        // Read before waiting so a large JSON answer can't fill the pipe and stall the helper.
        let data = out.fileHandleForReading.readDataToEndOfFile()
        let errData = err.fileHandleForReading.readDataToEndOfFile()
        p.waitUntilExit()
        guard p.terminationStatus == 0 else {
            let msg = String(data: errData, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            throw HelperError.failed(msg.isEmpty ? "exit code \(p.terminationStatus)" : msg)
        }
        return try parse(data)
    }
}
