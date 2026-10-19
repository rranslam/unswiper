// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// Google Takeout import (plan §19) through the bundled helper: `unswiper-blur takeout …` runs the
/// shared Rust core (crates/unswiper-takeout). The helper prints JSON, one object per line:
/// `plan`, then `progress` lines, then `done` or `error` (see `unswiper_takeout::cli`). Sample
/// output lives in crates/unswiper-takeout/tests/fixtures/helper (the Swift tests decode it).
enum TakeoutHelper {
    /// Written by the import into the library folder; marks a Google Photos (Takeout) library.
    static let stateFile = "unswiper-takeout-state.json"

    static func isTakeoutLibrary(_ root: URL) -> Bool {
        FileManager.default.fileExists(atPath: root.appendingPathComponent(stateFile).path)
    }

    struct Options: Codable, Equatable, Sendable {
        var deleteArchives = false
        /// Also unpack album copies (off: they're skipped as duplicates).
        var keepDuplicates = false
    }

    struct ArchivePlan: Codable, Equatable, Sendable {
        var path: String
        var archiveBytes: UInt64
        var items: Int
        var sidecars: Int
        var photosBytes: UInt64
        var newBytes: UInt64
        var newItems: Int
        var skippedItems: Int
        var exact: Bool
        var doneEarlier: Bool
    }

    struct Plan: Codable, Equatable, Sendable {
        var archives: [ArchivePlan]
        var archiveBytes: UInt64
        var items: Int
        var photosBytes: UInt64
        var newItems: Int
        var newBytes: UInt64
        var freeBytes: UInt64?
        var neededKeep: UInt64
        var neededDelete: UInt64
        var sameDisk: Bool
        var secondsMin: UInt64
        var secondsMax: UInt64
    }

    struct Progress: Codable, Equatable, Sendable {
        var archive: Int
        var archives: Int
        var files: Int
        var bytes: UInt64
        var bytesTotal: UInt64
    }

    struct Report: Codable, Equatable, Sendable {
        var extracted: Int
        var alreadyThere: Int
        var archivesSkipped: Int
        var bytesWritten: UInt64
        var refused: [String]
        var conflicts: [String]
        var otherProducts: [String]
        var previouslyImported: Int
        var duplicatesSkipped: Int
        var archivesDeleted: [String]
    }

    enum Event: Equatable, Sendable {
        /// The dry run. `lines` are the figures in words (same wording as on Linux).
        case plan(Plan, fits: Bool, needed: UInt64, lines: [String])
        case progress(Progress)
        case done(Report, lines: [String])
        /// `kind`: `not_enough_space`, `cancelled`, `no_archives`, `no_photos`, `write`, …
        case error(kind: String, message: String)
    }

    private struct Envelope: Decodable {
        var event: String
        var plan: Plan?
        var fits: Bool?
        var needed: UInt64?
        var lines: [String]?
        var report: Report?
        var kind: String?
        var message: String?
    }

    enum ParseError: LocalizedError {
        case unknown(String)
        var errorDescription: String? {
            switch self {
            case .unknown(let e): return "Unexpected answer from the import helper (\(e))."
            }
        }
    }

    static var decoder: JSONDecoder {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }

    /// One JSON object from the helper.
    static func parseEvent(_ data: Data) throws -> Event {
        let e = try decoder.decode(Envelope.self, from: data)
        switch e.event {
        case "plan":
            guard let plan = e.plan else { throw ParseError.unknown("plan without figures") }
            return .plan(plan, fits: e.fits ?? false, needed: e.needed ?? 0, lines: e.lines ?? [])
        case "progress":
            return .progress(try decoder.decode(Progress.self, from: data))
        case "done":
            guard let report = e.report else { throw ParseError.unknown("done without report") }
            return .done(report, lines: e.lines ?? [])
        case "error":
            return .error(kind: e.kind ?? "", message: e.message ?? "")
        default:
            throw ParseError.unknown(e.event)
        }
    }

    /// `takeout import <archives folder> --to <library> …`
    static func importArguments(archives: URL, library: URL, options: Options, dryRun: Bool) -> [String] {
        var a = ["takeout", "import", archives.path, "--to", library.path]
        if options.deleteArchives { a.append("--delete-archives") }
        if options.keepDuplicates { a.append("--keep-duplicates") }
        a += dryRun ? ["--dry-run", "--json"] : ["--progress-json"]
        return a
    }

    static func datesArguments(library: URL) -> [String] {
        ["takeout", "dates", library.path]
    }

    struct SidecarDate: Codable, Equatable {
        var path: String
        var taken: Int64?
        var lat: Double?
        var lon: Double?
    }

    /// Google's dates by file path (symlinks resolved, so `/private/var` and `/var` match).
    static func parseDates(_ data: Data) throws -> [String: Date] {
        let rows = try decoder.decode([SidecarDate].self, from: data)
        var out: [String: Date] = [:]
        for r in rows {
            guard let t = r.taken else { continue }
            out[URL(fileURLWithPath: r.path).resolvingSymlinksInPath().path] = Date(timeIntervalSince1970: TimeInterval(t))
        }
        return out
    }

    /// Dry run (never writes). Blocking: call it off the main thread.
    static func dryRun(helper: URL, archives: URL, library: URL, options: Options) throws -> Event {
        let out = try runCollecting(helper, arguments: importArguments(archives: archives, library: library, options: options, dryRun: true))
        guard let line = out.split(separator: UInt8(ascii: "\n")).last else {
            throw ParseError.unknown("no output")
        }
        return try parseEvent(Data(line))
    }

    /// Google's dates for the Mac indexer. Blocking: call it off the main thread.
    static func dates(helper: URL, library: URL) throws -> [String: Date] {
        try parseDates(InfoHelper.runHelper(helper, arguments: datesArguments(library: library)))
    }

    /// Runs the helper and returns stdout, even when it exits with an error (the JSON `error` line
    /// explains it).
    static func runCollecting(_ helper: URL, arguments: [String]) throws -> Data {
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
        if data.isEmpty, p.terminationStatus != 0 {
            let msg = String(data: errData, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            throw BlurHelper.HelperError.failed(msg.isEmpty ? "exit code \(p.terminationStatus)" : msg)
        }
        return data
    }

    /// Splits streamed bytes into complete lines; keeps the incomplete rest in `buffer`.
    static func takeLines(_ buffer: inout Data) -> [Data] {
        var lines: [Data] = []
        while let nl = buffer.firstIndex(of: UInt8(ascii: "\n")) {
            let line = buffer[buffer.startIndex..<nl]
            if !line.isEmpty { lines.append(Data(line)) }
            buffer = Data(buffer[buffer.index(after: nl)...])
        }
        return lines
    }

    /// "1.5 GB" style sizes (same as Linux).
    static func human(_ bytes: UInt64) -> String {
        let b = Double(bytes)
        if b >= 1e9 { return String(format: "%.1f GB", b / 1e9) }
        if b >= 1e6 { return String(format: "%.1f MB", b / 1e6) }
        return "\(bytes) bytes"
    }

    /// "under a minute", "about 3 min", "about 2 h 10 min" (same as Linux).
    static func duration(_ secs: UInt64) -> String {
        if secs < 60 { return "under a minute" }
        let m = (secs + 59) / 60
        if secs < 3600 { return "about \(m) min" }
        return "about \(m / 60) h \(m % 60) min"
    }

    /// Progress bar fraction and text, e.g. "Part 2 of 3 · 120 files · 1.0 GB of 4.0 GB · 100.0 MB/s · under a minute left".
    static func progressText(_ p: Progress, elapsed: TimeInterval) -> (Double, String) {
        let frac = p.bytesTotal > 0 ? min(1, max(0, Double(p.bytes) / Double(p.bytesTotal))) : 0
        let rate = elapsed > 0.5 ? Double(p.bytes) / elapsed : 0
        var t = "Part \(max(p.archive, 1)) of \(p.archives) · \(p.files) files · \(human(p.bytes)) of \(human(p.bytesTotal))"
        if rate > 0 {
            t += " · \(human(UInt64(rate)))/s"
            if p.bytesTotal > p.bytes {
                t += " · \(duration(UInt64(Double(p.bytesTotal - p.bytes) / rate))) left"
            }
        }
        return (frac, t)
    }

    /// Why the folders can't be checked yet (same rules as Linux), or nil.
    static func folderProblem(archives: URL?, library: URL?) -> String? {
        guard let archives else { return "Choose the archives folder." }
        guard let library else { return "Choose the library folder." }
        let a = archives.standardizedFileURL.pathComponents
        let l = library.standardizedFileURL.pathComponents
        if a == l { return "Choose two different folders: one with the archives, one for the library." }
        if l.starts(with: a) { return nil }
        if a.starts(with: l) {
            return "The archives folder can't be inside the library folder. Choose another library folder."
        }
        return nil
    }
}

/// A running import: streams the helper's JSON lines to `onEvent` on the main thread, in order,
/// then calls `onExit` with the exit code.
final class TakeoutRun: @unchecked Sendable {
    private let process = Process()
    /// Serial: chunks are parsed in the order they were read.
    private let queue = DispatchQueue(label: "unswiper.takeout.output")
    private var buffer = Data()

    init(helper: URL, arguments: [String],
         onEvent: @escaping @MainActor (TakeoutHelper.Event) -> Void,
         onExit: @escaping @MainActor (Int32) -> Void) throws {
        process.executableURL = helper
        process.arguments = arguments
        let out = Pipe()
        let reader = out.fileHandleForReading
        process.standardOutput = out
        process.standardError = FileHandle.nullDevice
        let q = queue
        let deliver: (Data) -> Void = { [weak self] chunk in
            guard let self else { return }
            self.buffer.append(chunk)
            for line in TakeoutHelper.takeLines(&self.buffer) {
                if let ev = try? TakeoutHelper.parseEvent(line) {
                    DispatchQueue.main.async { MainActor.assumeIsolated { onEvent(ev) } }
                }
            }
        }
        reader.readabilityHandler = { h in
            let chunk = h.availableData
            guard !chunk.isEmpty else { return }
            q.async { deliver(chunk) }
        }
        process.terminationHandler = { p in
            let code = p.terminationStatus
            q.async {
                reader.readabilityHandler = nil
                let rest = reader.readDataToEndOfFile()
                deliver(rest + Data("\n".utf8))
                DispatchQueue.main.async { MainActor.assumeIsolated { onExit(code) } }
            }
        }
        try process.run()
    }

    /// Stops the import; everything finished so far is kept and the next run continues.
    func stop() {
        if process.isRunning { process.terminate() }
    }
}
