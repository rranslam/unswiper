// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation
import AppKit

enum CommitService {
    struct Result: Sendable {
        var moved: Int
        var failed: [(path: String, error: String)]
        var bytes: Int64
        var dryRun: Bool
        /// Dry run only: how many items would be moved. `moved` stays 0 for a dry run.
        var wouldMove: Int = 0
        var manifestURL: URL? = nil
    }

    struct RestoreResult: Sendable {
        var restored: Int
        var failed: [(path: String, error: String)]
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
            let bytes = rejects.reduce(Int64(0)) { $0 + $1.totalBytes }
            return Result(moved: 0, failed: [], bytes: bytes, dryRun: true, wouldMove: rejects.count)
        }

        var moved = 0
        var failed: [(String, String)] = []
        var bytes: Int64 = 0
        var entries: [CommitManifest.Entry] = []
        let fm = FileManager.default
        let now = Date()
        let stamp = ISO8601DateFormatter().string(from: now).replacingOccurrences(of: ":", with: "-")

        let quarantineBase: URL = {
            if let root = libraryRoot {
                return root.appendingPathComponent("_Rejected/Unswiper", isDirectory: true)
            }
            return LibraryStore.quarantineRoot
        }()
        let destDir = quarantineBase.appendingPathComponent(stamp, isDirectory: true)
        if mode == .quarantine {
            try fm.createDirectory(at: destDir, withIntermediateDirectories: true)
        }

        for item in rejects {
            // Validate every file in the set before touching any of them.
            let files: [(path: String, size: Int64, modifiedAt: Date?)] =
                [(item.path, item.sizeBytes, item.modifiedAt)] + item.sidecars.map { ($0.path, $0.sizeBytes, $0.modifiedAt) }
            if let problem = files.lazy.compactMap({ validate(path: $0.path, size: $0.size, modifiedAt: $0.modifiedAt, root: libraryRoot) }).first {
                failed.append((item.path, problem))
                continue
            }

            var setFailed = false
            for file in files {
                let src = URL(fileURLWithPath: file.path)
                do {
                    let resultPath: String
                    if mode == .quarantine {
                        var dest = destDir.appendingPathComponent(src.lastPathComponent)
                        if fm.fileExists(atPath: dest.path) {
                            dest = destDir.appendingPathComponent("\(UUID().uuidString.prefix(8))-\(src.lastPathComponent)")
                        }
                        try fm.moveItem(at: src, to: dest)
                        resultPath = dest.path
                    } else {
                        var resulting: NSURL?
                        try fm.trashItem(at: src, resultingItemURL: &resulting)
                        resultPath = (resulting as URL?)?.path ?? ""
                    }
                    entries.append(.init(originalPath: file.path, resultPath: resultPath, sizeBytes: file.size, movedAt: Date()))
                    bytes += file.size
                } catch {
                    failed.append((file.path, error.localizedDescription))
                    setFailed = true
                }
            }
            if !setFailed { moved += 1 }
        }

        var manifestURL: URL?
        if !entries.isEmpty {
            let manifest = CommitManifest(id: stamp, createdAt: now, mode: mode, libraryRoot: libraryRoot?.path, entries: entries)
            let supportURL = LibraryStore.commitsDir.appendingPathComponent("\(stamp).json")
            var urls = [supportURL]
            if mode == .quarantine { urls.append(destDir.appendingPathComponent("manifest.json")) }
            do {
                try LibraryStore.writeManifest(manifest, to: urls)
                manifestURL = supportURL
            } catch {
                failed.append(("manifest.json", "Could not write manifest: \(error.localizedDescription)"))
            }
        }
        return Result(moved: moved, failed: failed, bytes: bytes, dryRun: false, manifestURL: manifestURL)
    }

    /// Returns a failure reason, or nil if the file is safe to move.
    static func validate(path: String, size: Int64, modifiedAt: Date?, root: URL?) -> String? {
        guard let root else { return "No library folder; refusing to move." }
        let rootPath = root.resolvingSymlinksInPath().standardizedFileURL.path
        let fileURL = URL(fileURLWithPath: path).resolvingSymlinksInPath().standardizedFileURL
        guard fileURL.path.hasPrefix(rootPath.hasSuffix("/") ? rootPath : rootPath + "/") else {
            return "Outside the library folder; not moved."
        }
        guard let values = try? fileURL.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey, .contentModificationDateKey]),
              values.isRegularFile == true
        else {
            return "File is missing or not a regular file; re-index."
        }
        guard Int64(values.fileSize ?? -1) == size else {
            return "File size changed since indexing; re-index."
        }
        guard let expected = modifiedAt else {
            return "No modification date recorded; re-index before committing."
        }
        // library.json stores ISO 8601 without fractional seconds, so allow 1s slack.
        guard let actual = values.contentModificationDate, abs(actual.timeIntervalSince(expected)) < 1 else {
            return "File modified since indexing; re-index."
        }
        return nil
    }

    /// Moves every file in the manifest back to its original path.
    static func restore(manifest: CommitManifest) -> RestoreResult {
        let fm = FileManager.default
        var restored = 0
        var failed: [(String, String)] = []
        for entry in manifest.entries {
            let src = URL(fileURLWithPath: entry.resultPath)
            let dest = URL(fileURLWithPath: entry.originalPath)
            guard !entry.resultPath.isEmpty, fm.fileExists(atPath: src.path) else {
                failed.append((entry.originalPath, "Moved file not found (Trash emptied?)."))
                continue
            }
            guard !fm.fileExists(atPath: dest.path) else {
                failed.append((entry.originalPath, "A file already exists at the original path."))
                continue
            }
            do {
                try fm.createDirectory(at: dest.deletingLastPathComponent(), withIntermediateDirectories: true)
                try fm.moveItem(at: src, to: dest)
                restored += 1
            } catch {
                failed.append((entry.originalPath, error.localizedDescription))
            }
        }
        return RestoreResult(restored: restored, failed: failed)
    }
}

enum ByteFormat {
    static func string(_ bytes: Int64) -> String {
        let f = ByteCountFormatter()
        f.countStyle = .file
        return f.string(fromByteCount: bytes)
    }
}
