// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// Grouping by place through the bundled Rust helper (plan §20.6, M2):
/// `unswiper-blur group --library library.json`. The helper reads the saved library, reads GPS
/// for items without a location, groups with the same code as Linux (offline place names) and
/// prints `{"groups":[…],"locations":{path: location}}`. It never writes library.json.
enum GroupHelper {
    struct Answer: Decodable {
        var groups: [VisitGroup]
        var locations: [String: GeoLocation]
    }

    enum GroupError: LocalizedError {
        case failed(String)
        var errorDescription: String? {
            switch self {
            case .failed(let s): return "Grouping by place failed: \(s)"
            }
        }
    }

    static func arguments(library: URL, mode: GroupMode) -> [String] {
        ["group", "--library", library.path, "--mode", mode.rawValue]
    }

    static func parse(_ data: Data) throws -> Answer {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return try decoder.decode(Answer.self, from: data)
    }

    static func run(helper: URL, library: URL, mode: GroupMode) throws -> Answer {
        let p = Process()
        p.executableURL = helper
        p.arguments = arguments(library: library, mode: mode)
        let out = Pipe(), err = Pipe()
        p.standardOutput = out
        p.standardError = err
        try p.run()
        // Read before waiting so a large answer can't fill the pipe and stall the helper.
        let data = out.fileHandleForReading.readDataToEndOfFile()
        let errData = err.fileHandleForReading.readDataToEndOfFile()
        p.waitUntilExit()
        guard p.terminationStatus == 0 else {
            let msg = String(data: errData, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            throw GroupError.failed(msg.isEmpty ? "exit code \(p.terminationStatus)" : msg)
        }
        return try parse(data)
    }

    /// Items with the helper's new locations filled in (existing locations are kept).
    static func apply(_ locations: [String: GeoLocation], to items: [MediaItem]) -> [MediaItem] {
        guard !locations.isEmpty else { return items }
        return items.map { item in
            var item = item
            if item.location == nil, let l = locations[item.path] { item.location = l }
            return item
        }
    }
}
