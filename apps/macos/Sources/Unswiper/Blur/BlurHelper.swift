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

    static func arguments(library: URL, sensitivity: BlurRules.Sensitivity, onlyList: URL?, progress: Bool = false) -> [String] {
        var a = ["scan", library.path, "--json", "--sensitivity", sensitivity.rawValue, "--write-cache"]
        if let onlyList { a += ["--only", onlyList.path] }
        if progress { a.append("--progress") }
        return a
    }

    /// `progress DONE TOTAL` (stderr with `--progress`) → (done, total).
    static func parseProgress(_ line: Substring) -> (Int, Int)? {
        let parts = line.split(separator: " ")
        guard parts.count == 3, parts[0] == "progress", let d = Int(parts[1]), let t = Int(parts[2]) else { return nil }
        return (d, t)
    }

    /// Splits the helper's stderr into progress updates and other text (kept for errors).
    final class StderrLines: @unchecked Sendable {
        private let lock = NSLock()
        private var partial = ""
        private var text = ""
        private let onProgress: ((Int, Int) -> Void)?

        init(onProgress: ((Int, Int) -> Void)?) { self.onProgress = onProgress }

        func feed(_ chunk: Data) {
            lock.lock()
            partial += String(decoding: chunk, as: UTF8.self)
            var lines = partial.split(separator: "\n", omittingEmptySubsequences: false)
            partial = String(lines.removeLast())
            var last: (Int, Int)?
            for l in lines {
                if let pr = BlurHelper.parseProgress(l) { last = pr } else { text += l + "\n" }
            }
            lock.unlock()
            if let last { onProgress?(last.0, last.1) }
        }

        /// Everything that was not a progress line.
        func finish() -> String {
            lock.lock(); defer { lock.unlock() }
            if BlurHelper.parseProgress(Substring(partial)) == nil { text += partial }
            partial = ""
            return text
        }
    }

    /// The helper process of a running scan, so it can be paused, resumed or stopped.
    final class Control: @unchecked Sendable {
        private let lock = NSLock()
        private var process: Process?
        private var stopped = false

        func attach(_ p: Process) {
            lock.lock(); defer { lock.unlock() }
            process = p
            if stopped { p.terminate() }
        }
        func pause() { lock.lock(); defer { lock.unlock() }; _ = process?.suspend() }
        func resume() { lock.lock(); defer { lock.unlock() }; _ = process?.resume() }
        /// Ends the helper. Scores it already saved are kept (it saves every 30 s).
        func stop() {
            lock.lock(); defer { lock.unlock() }
            stopped = true
            if let p = process, p.isRunning {
                _ = p.resume() // a suspended process must run to handle SIGTERM
                p.terminate()
            }
        }
    }

    static func parse(_ data: Data) throws -> Verdict {
        let rows = try JSONDecoder().decode([Row].self, from: data)
        return Verdict(flagged: Set(rows.filter(\.blurry).map(\.path)), scored: Set(rows.map(\.path)))
    }

    /// Scores (cached photos are not decoded again) and returns the verdict for every photo with
    /// a score. `only` limits the new scoring to those paths, e.g. the open visit. Blocking: call
    /// it off the main thread.
    static func run(helper: URL, library: URL, sensitivity: BlurRules.Sensitivity, only: [String]?,
                    control: Control? = nil, onProgress: ((Int, Int) -> Void)? = nil) throws -> Verdict {
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
        p.arguments = arguments(library: library, sensitivity: sensitivity, onlyList: list, progress: onProgress != nil)
        let out = Pipe(), err = Pipe()
        p.standardOutput = out
        p.standardError = err
        // stderr is read as it comes (progress lines); everything else is kept for errors.
        let errs = StderrLines(onProgress: onProgress)
        err.fileHandleForReading.readabilityHandler = { h in
            let chunk = h.availableData
            if !chunk.isEmpty { errs.feed(chunk) }
        }
        try p.run()
        control?.attach(p)
        // Read before waiting so a large JSON answer can't fill the pipe and stall the helper.
        let data = out.fileHandleForReading.readDataToEndOfFile()
        p.waitUntilExit()
        err.fileHandleForReading.readabilityHandler = nil
        errs.feed(err.fileHandleForReading.readDataToEndOfFile())
        let errAll = errs.finish()
        if p.terminationReason == .uncaughtSignal { throw CancellationError() }
        guard p.terminationStatus == 0 else {
            let msg = errAll.trimmingCharacters(in: .whitespacesAndNewlines)
            throw HelperError.failed(msg.isEmpty ? "exit code \(p.terminationStatus)" : msg)
        }
        return try parse(data)
    }
}
