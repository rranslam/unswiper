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
        /// Entries of an interrupted commit whose file never left its original place.
        var alreadyInPlace: Int = 0
    }

    /// Moves the rejected items (and their sidecars) to the Trash or quarantine.
    ///
    /// Safety order: validate every set first; write a `pending` manifest listing every planned
    /// move *before* touching any file; update the manifest after each move; mark it `complete`.
    /// If the app dies part-way, `LibraryStore.pendingManifestURLs()` finds it on next launch.
    ///
    /// - Parameter stopAfterMoves: test hook that simulates a crash after N file moves
    ///   (returns without finalizing the manifest).
    static func commitRejects(
        items: [MediaItem],
        mode: CommitMode,
        libraryRoot: URL?,
        now: Date = Date(),
        stopAfterMoves: Int? = nil
    ) throws -> Result {
        let rejects = items.filter { $0.decision == .reject }
        guard !rejects.isEmpty else {
            return Result(moved: 0, failed: [], bytes: 0, dryRun: mode == .dryRun)
        }

        if mode == .dryRun {
            let bytes = rejects.reduce(Int64(0)) { $0 + $1.totalBytes }
            return Result(moved: 0, failed: [], bytes: bytes, dryRun: true, wouldMove: rejects.count)
        }

        var failed: [(String, String)] = []
        let fm = FileManager.default
        let stamp = ISO8601DateFormatter().string(from: now).replacingOccurrences(of: ":", with: "-")

        let quarantineBase: URL = {
            if let root = libraryRoot {
                return root.appendingPathComponent("_Rejected/Unswiper", isDirectory: true)
            }
            return LibraryStore.quarantineRoot
        }()
        let destDir = quarantineBase.appendingPathComponent(stamp, isDirectory: true)

        // 1. Validate every set before moving anything.
        typealias FileRef = (path: String, size: Int64, modifiedAt: Date?)
        var plannedSets: [(item: MediaItem, entryRange: Range<Int>)] = []
        var entries: [CommitManifest.Entry] = []
        var plannedNames = Set<String>()
        for item in rejects {
            let files: [FileRef] = [(item.path, item.sizeBytes, item.modifiedAt)]
                + item.sidecars.map { ($0.path, $0.sizeBytes, $0.modifiedAt) }
            if let problem = files.lazy.compactMap({ validate(path: $0.path, size: $0.size, modifiedAt: $0.modifiedAt, root: libraryRoot) }).first {
                failed.append((item.path, problem))
                continue
            }
            let start = entries.count
            for file in files {
                var planned = ""
                if mode == .quarantine {
                    var name = URL(fileURLWithPath: file.path).lastPathComponent
                    if plannedNames.contains(name.lowercased()) {
                        name = "\(UUID().uuidString.prefix(8))-\(name)"
                    }
                    plannedNames.insert(name.lowercased())
                    planned = destDir.appendingPathComponent(name).path
                }
                entries.append(.init(originalPath: file.path, resultPath: planned, sizeBytes: file.size, movedAt: nil, state: .pending))
            }
            plannedSets.append((item, start..<entries.count))
        }

        guard !entries.isEmpty else {
            return Result(moved: 0, failed: failed, bytes: 0, dryRun: false)
        }

        // 2. Journal first.
        var manifest = CommitManifest(status: .pending, id: stamp, createdAt: now, mode: mode, libraryRoot: libraryRoot?.path, entries: entries)
        let manifestURL = LibraryStore.commitsDir.appendingPathComponent("\(stamp).json")
        try LibraryStore.writeManifest(manifest, to: [manifestURL])
        if mode == .quarantine {
            try fm.createDirectory(at: destDir, withIntermediateDirectories: true)
        }

        // 3. Move, recording each result.
        var moved = 0
        var bytes: Int64 = 0
        var movesDone = 0
        for set in plannedSets {
            var setFailed = false
            for i in set.entryRange {
                if let limit = stopAfterMoves, movesDone >= limit {
                    return Result(moved: moved, failed: failed, bytes: bytes, dryRun: false, manifestURL: manifestURL)
                }
                let src = URL(fileURLWithPath: manifest.entries[i].originalPath)
                do {
                    if mode == .quarantine {
                        let dest = URL(fileURLWithPath: manifest.entries[i].resultPath)
                        try fm.moveItem(at: src, to: dest)
                    } else {
                        var resulting: NSURL?
                        try fm.trashItem(at: src, resultingItemURL: &resulting)
                        manifest.entries[i].resultPath = (resulting as URL?)?.path ?? ""
                    }
                    manifest.entries[i].state = .moved
                    manifest.entries[i].movedAt = Date()
                    bytes += manifest.entries[i].sizeBytes
                    movesDone += 1
                } catch {
                    manifest.entries[i].state = .failed
                    manifest.entries[i].error = error.localizedDescription
                    failed.append((src.path, error.localizedDescription))
                    setFailed = true
                }
                do {
                    try LibraryStore.writeManifest(manifest, to: [manifestURL])
                } catch {
                    failed.append(("manifest.json", "Could not update manifest: \(error.localizedDescription)"))
                }
            }
            if !setFailed { moved += 1 }
        }

        // 4. Finalize.
        manifest.status = .complete
        let anyMoved = manifest.entries.contains { $0.effectiveState == .moved }
        if !anyMoved {
            try? fm.removeItem(at: manifestURL)
            if mode == .quarantine { try? fm.removeItem(at: destDir) }
            return Result(moved: moved, failed: failed, bytes: bytes, dryRun: false)
        }
        var urls = [manifestURL]
        if mode == .quarantine { urls.append(destDir.appendingPathComponent("manifest.json")) }
        do {
            try LibraryStore.writeManifest(manifest, to: urls)
        } catch {
            failed.append(("manifest.json", "Could not write manifest: \(error.localizedDescription)"))
        }
        return Result(moved: moved, failed: failed, bytes: bytes, dryRun: false, manifestURL: manifestURL)
    }

    /// Returns a failure reason, or nil if the file is safe to move.
    static func validate(path: String, size: Int64, modifiedAt: Date?, root: URL?) -> String? {
        guard let root else { return "No library folder; refusing to move." }
        let rootPath = PathUtil.canonicalPath(root)
        let rawURL = URL(fileURLWithPath: path)
        if (try? FileManager.default.destinationOfSymbolicLink(atPath: rawURL.path)) != nil {
            return "Is a symbolic link; not moved."
        }
        let fileURL = URL(fileURLWithPath: PathUtil.canonicalPath(rawURL))
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

    /// Moves every moved file in the manifest back to its original path.
    /// Works for complete manifests and for `pending` ones left by an interrupted commit.
    static func restore(manifest: CommitManifest) -> RestoreResult {
        let fm = FileManager.default
        var result = RestoreResult(restored: 0, failed: [])
        for entry in manifest.entries {
            let dest = URL(fileURLWithPath: entry.originalPath)
            let state = entry.effectiveState
            if state == .failed {
                continue // never moved
            }
            if state == .pending {
                // Interrupted before/while moving this file.
                if fm.fileExists(atPath: dest.path) {
                    result.alreadyInPlace += 1
                    continue
                }
                if entry.resultPath.isEmpty {
                    result.failed.append((entry.originalPath, "Couldn't locate this file in the Trash. Open the Trash in Finder and use Put Back."))
                    continue
                }
            }
            if entry.resultPath.isEmpty {
                let msg = manifest.mode == .trash
                    ? "Couldn't locate this file in the Trash (macOS didn't report where it went). Open the Trash in Finder and use Put Back."
                    : "No record of where this file was moved."
                result.failed.append((entry.originalPath, msg))
                continue
            }
            let src = URL(fileURLWithPath: entry.resultPath)
            guard fm.fileExists(atPath: src.path) else {
                let msg = manifest.mode == .trash
                    ? "Couldn't locate this file in the Trash (was the Trash emptied?)."
                    : "Moved file not found at \(entry.resultPath)."
                result.failed.append((entry.originalPath, msg))
                continue
            }
            guard !fm.fileExists(atPath: dest.path) else {
                result.failed.append((entry.originalPath, "A file already exists at the original path."))
                continue
            }
            do {
                try fm.createDirectory(at: dest.deletingLastPathComponent(), withIntermediateDirectories: true)
                try fm.moveItem(at: src, to: dest)
                result.restored += 1
            } catch {
                result.failed.append((entry.originalPath, error.localizedDescription))
            }
        }
        return result
    }
}

enum ByteFormat {
    static func string(_ bytes: Int64) -> String {
        let f = ByteCountFormatter()
        f.countStyle = .file
        return f.string(fromByteCount: bytes)
    }
}
