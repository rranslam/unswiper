// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

enum MediaKind: String, Codable, Hashable, CaseIterable {
    case photo
    case video
}

enum Decision: String, Codable, Hashable {
    case undecided
    case keep
    case reject
}

enum CommitMode: String, Codable, CaseIterable, Identifiable {
    case trash
    case quarantine
    case dryRun = "dry_run"

    var id: String { rawValue }

    var label: String {
        switch self {
        case .trash: return "Move to Trash"
        case .quarantine: return "Quarantine folder"
        case .dryRun: return "Dry run (decisions only)"
        }
    }
}

/// Where an item's grouping date came from (see the Undated rule in docs/linux-port-plan.md §7).
enum DateSource: String, Codable, Hashable {
    /// EXIF / TIFF / QuickTime capture date.
    case capture
    /// A date parsed from the folder path (e.g. `2026/01/01`, `2025-07-04 Beach`).
    case folder
    /// No date of its own; grouping uses the dated files in the same folder, or the folder name.
    case unknown
}

/// A file that travels with a primary item (Live Photo MOV, .AAE/.XMP sidecar).
struct SidecarFile: Codable, Hashable {
    var path: String
    var sizeBytes: Int64
    var modifiedAt: Date?
}

struct MediaItem: Identifiable, Codable, Hashable {
    var id: String { path }
    var path: String
    var kind: MediaKind
    var sizeBytes: Int64
    var takenAt: Date
    var relativeFolder: String
    var decision: Decision
    /// File modification date captured at index time (nil for libraries indexed before this field existed).
    var modifiedAt: Date? = nil
    /// Paired files (Live Photo video, edit sidecars). Decisions and moves apply to the whole set.
    var sidecars: [SidecarFile] = []
    /// True when no capture date or usable file date was found; `takenAt` is then a placeholder.
    var isUndated: Bool = false
    /// How `takenAt` was determined (nil for libraries indexed before v0.1.2).
    var dateSource: DateSource? = nil
    /// Parent folder relative to the library root ("" = the root itself). Nil for older libraries.
    var folderPath: String? = nil

    var fileURL: URL { URL(fileURLWithPath: path) }
    var fileName: String { fileURL.lastPathComponent }
    var totalBytes: Int64 { sidecars.reduce(sizeBytes) { $0 + $1.sizeBytes } }
    var fileCount: Int { 1 + sidecars.count }

    enum CodingKeys: String, CodingKey {
        case path, kind, sizeBytes, takenAt, relativeFolder, decision, modifiedAt, sidecars, isUndated, dateSource, folderPath
    }
}

extension MediaItem {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        path = try c.decode(String.self, forKey: .path)
        kind = try c.decode(MediaKind.self, forKey: .kind)
        sizeBytes = try c.decode(Int64.self, forKey: .sizeBytes)
        takenAt = try c.decode(Date.self, forKey: .takenAt)
        relativeFolder = try c.decode(String.self, forKey: .relativeFolder)
        decision = try c.decode(Decision.self, forKey: .decision)
        modifiedAt = try c.decodeIfPresent(Date.self, forKey: .modifiedAt)
        sidecars = try c.decodeIfPresent([SidecarFile].self, forKey: .sidecars) ?? []
        isUndated = try c.decodeIfPresent(Bool.self, forKey: .isUndated) ?? false
        dateSource = try c.decodeIfPresent(DateSource.self, forKey: .dateSource)
        folderPath = try c.decodeIfPresent(String.self, forKey: .folderPath)
    }
}

/// Record of one commit, written so it can be restored.
///
/// Since v0.1.2 the manifest is written with `status: pending` *before* any file moves and is
/// updated after every move, so an interrupted commit can still be restored. Manifests from
/// older versions have no `status`/`state` and are treated as complete / moved.
struct CommitManifest: Codable {
    enum Status: String, Codable, Sendable { case pending, complete }
    enum EntryState: String, Codable, Sendable { case pending, moved, failed }

    struct Entry: Codable, Sendable {
        var originalPath: String
        /// Where the file went. For quarantine this is filled in before the move (planned path).
        /// For Trash it is empty until macOS reports the location, and may stay empty.
        var resultPath: String
        var sizeBytes: Int64
        var movedAt: Date?
        var state: EntryState? = nil
        var error: String? = nil

        var effectiveState: EntryState { state ?? .moved }
    }
    var status: Status? = nil
    var id: String
    var createdAt: Date
    var mode: CommitMode
    var libraryRoot: String?
    var entries: [Entry]

    var isPending: Bool { status == .pending }
}

struct VisitGroup: Identifiable, Codable, Hashable {
    var id: String
    var title: String
    var startDate: Date
    var endDate: Date
    var itemPaths: [String]
    var photoCount: Int
    var videoCount: Int
    var totalBytes: Int64
    var folderHint: String

    var itemCount: Int { itemPaths.count }

    func progress(in items: [MediaItem]) -> (kept: Int, rejected: Int, left: Int) {
        let set = Set(itemPaths)
        let members = items.filter { set.contains($0.path) }
        let kept = members.filter { $0.decision == .keep }.count
        let rejected = members.filter { $0.decision == .reject }.count
        let left = members.filter { $0.decision == .undecided }.count
        return (kept, rejected, left)
    }
}

struct AppConfig: Codable, Equatable {
    var rootPath: String?
    var keepTargetDefault: Int
    var visitGapHours: Double
    var commitMode: CommitMode
    var bookmarkData: Data?
    /// Per-visit keep targets keyed by group id. Optional so older library.json files still decode.
    var keepTargets: [String: Int]? = nil

    static let `default` = AppConfig(
        rootPath: nil,
        keepTargetDefault: 10,
        visitGapHours: 36,
        commitMode: .trash,
        bookmarkData: nil
    )
}

struct LibrarySnapshot: Codable {
    /// Format version of library.json. Missing in files written before v0.1.2 (treated as 1).
    /// Version 1 = absolute item paths (today's format). See docs/linux-port-plan.md §5.
    static let currentSchemaVersion = 1
    var schemaVersion: Int? = LibrarySnapshot.currentSchemaVersion
    var config: AppConfig
    var items: [MediaItem]
    var groups: [VisitGroup]
    var lastIndexedAt: Date?
}
