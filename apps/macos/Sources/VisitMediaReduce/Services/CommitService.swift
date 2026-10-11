import Foundation
import AppKit

enum CommitService {
    struct Result {
        var moved: Int
        var failed: [(path: String, error: String)]
        var bytes: Int64
        var dryRun: Bool
    }

    static func commitRejects(
        items: [MediaItem],
        mode: CommitMode,
        libraryRoot: URL?
    ) throws -> Result {
        let rejects = items.filter { $0.decision == .reject }
        guard !rejects.isEmpty else {
            return Result(moved: 0, failed: [], bytes: 0, dryRun: mode == .dryRun)
        }

        if mode == .dryRun {
            let bytes = rejects.reduce(Int64(0)) { $0 + $1.sizeBytes }
            return Result(moved: rejects.count, failed: [], bytes: bytes, dryRun: true)
        }

        var moved = 0
        var failed: [(String, String)] = []
        var bytes: Int64 = 0
        let fm = FileManager.default

        let quarantineBase: URL = {
            if let root = libraryRoot {
                return root.appendingPathComponent("_Rejected/visit-media-reduce", isDirectory: true)
            }
            return LibraryStore.quarantineRoot
        }()

        if mode == .quarantine {
            let stamp = ISO8601DateFormatter().string(from: Date()).replacingOccurrences(of: ":", with: "-")
            let destDir = quarantineBase.appendingPathComponent(stamp, isDirectory: true)
            try fm.createDirectory(at: destDir, withIntermediateDirectories: true)
            for item in rejects {
                let src = item.fileURL
                var dest = destDir.appendingPathComponent(src.lastPathComponent)
                if fm.fileExists(atPath: dest.path) {
                    dest = destDir.appendingPathComponent("\(UUID().uuidString.prefix(8))-\(src.lastPathComponent)")
                }
                do {
                    try fm.moveItem(at: src, to: dest)
                    moved += 1
                    bytes += item.sizeBytes
                } catch {
                    failed.append((item.path, error.localizedDescription))
                }
            }
            return Result(moved: moved, failed: failed, bytes: bytes, dryRun: false)
        }

        // Trash
        for item in rejects {
            do {
                var resulting: NSURL?
                try fm.trashItem(at: item.fileURL, resultingItemURL: &resulting)
                moved += 1
                bytes += item.sizeBytes
            } catch {
                failed.append((item.path, error.localizedDescription))
            }
        }
        return Result(moved: moved, failed: failed, bytes: bytes, dryRun: false)
    }
}

enum ByteFormat {
    static func string(_ bytes: Int64) -> String {
        let f = ByteCountFormatter()
        f.countStyle = .file
        return f.string(fromByteCount: bytes)
    }
}
