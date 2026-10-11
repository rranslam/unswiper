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

struct MediaItem: Identifiable, Codable, Hashable {
    var id: String { path }
    var path: String
    var kind: MediaKind
    var sizeBytes: Int64
    var takenAt: Date
    var relativeFolder: String
    var decision: Decision

    var fileURL: URL { URL(fileURLWithPath: path) }
    var fileName: String { fileURL.lastPathComponent }
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

    static let `default` = AppConfig(
        rootPath: nil,
        keepTargetDefault: 10,
        visitGapHours: 36,
        commitMode: .trash,
        bookmarkData: nil
    )
}

struct LibrarySnapshot: Codable {
    var config: AppConfig
    var items: [MediaItem]
    var groups: [VisitGroup]
    var lastIndexedAt: Date?
}
