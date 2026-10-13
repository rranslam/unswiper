// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

enum LibraryStore {
    /// Tests point this at a temporary folder. Nil = ~/Library/Application Support/Unswiper.
    nonisolated(unsafe) static var supportDirOverride: URL?

    static var supportDir: URL {
        if let dir = supportDirOverride {
            try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            return dir
        }
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let dir = base.appendingPathComponent("Unswiper", isDirectory: true)
        migrateLegacySupportDir(base: base, to: dir)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// One-time migration from the pre-rename folder ("visit-media-reduce").
    /// The old folder is moved and a symlink is left in its place so paths
    /// recorded in older commit manifests still resolve for Restore.
    private static func migrateLegacySupportDir(base: URL, to dir: URL) {
        let fm = FileManager.default
        let legacy = base.appendingPathComponent("visit-media-reduce", isDirectory: true)
        guard !fm.fileExists(atPath: dir.path) else { return }
        var isDir: ObjCBool = false
        guard fm.fileExists(atPath: legacy.path, isDirectory: &isDir), isDir.boolValue,
              (try? fm.destinationOfSymbolicLink(atPath: legacy.path)) == nil else { return }
        do {
            try fm.moveItem(at: legacy, to: dir)
            try? fm.createSymbolicLink(at: legacy, withDestinationURL: dir)
        } catch {
            // Fall back to a fresh folder; the legacy data stays untouched.
        }
    }

    static var snapshotURL: URL {
        supportDir.appendingPathComponent("library.json")
    }

    static var quarantineRoot: URL {
        supportDir.appendingPathComponent("Quarantine", isDirectory: true)
    }

    static var commitsDir: URL {
        let dir = supportDir.appendingPathComponent("commits", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// Most recent manifest that has not been restored yet.
    static func latestManifestURL() -> URL? {
        manifestURLs().last
    }

    /// All unrestored manifests, oldest first.
    static func manifestURLs() -> [URL] {
        let files = (try? FileManager.default.contentsOfDirectory(at: commitsDir, includingPropertiesForKeys: nil)) ?? []
        return files.filter { $0.pathExtension == "json" }
            .sorted { $0.lastPathComponent < $1.lastPathComponent }
    }

    /// Manifests left in `pending` state by a commit that was interrupted (crash, power loss, force quit).
    static func pendingManifestURLs() -> [URL] {
        manifestURLs().filter { (try? readManifest(at: $0))?.isPending == true }
    }

    static func writeManifest(_ manifest: CommitManifest, to urls: [URL]) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        encoder.dateEncodingStrategy = .iso8601
        let data = try encoder.encode(manifest)
        for url in urls {
            try data.write(to: url, options: .atomic)
        }
    }

    static func readManifest(at url: URL) throws -> CommitManifest {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return try decoder.decode(CommitManifest.self, from: Data(contentsOf: url))
    }

    struct LoadResult {
        var snapshot: LibrarySnapshot
        /// Set when library.json existed but could not be read; describes what happened.
        var problem: String?
        /// Where the unreadable file was copied to.
        var backupURL: URL?
    }

    static var emptySnapshot: LibrarySnapshot {
        LibrarySnapshot(config: .default, items: [], groups: [], lastIndexedAt: nil)
    }

    /// Set when library.json could not be read *and* could not be backed up. While set, `save`
    /// refuses to overwrite it so the user's decisions are never silently lost.
    nonisolated(unsafe) static var saveBlockedReason: String?

    /// Loads library.json. A missing file is a fresh start. A file that exists but can't be read
    /// or decoded (or comes from a newer version) is copied to `library.json.corrupt-<stamp>`
    /// first and the problem is reported, instead of silently starting empty.
    static func load() -> LoadResult {
        saveBlockedReason = nil
        let url = snapshotURL
        guard FileManager.default.fileExists(atPath: url.path) else {
            return LoadResult(snapshot: emptySnapshot)
        }
        let reason: String
        do {
            let data = try Data(contentsOf: url)
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            let snap = try decoder.decode(LibrarySnapshot.self, from: data)
            let version = snap.schemaVersion ?? 1
            if version <= LibrarySnapshot.currentSchemaVersion {
                return LoadResult(snapshot: snap)
            }
            reason = "it was saved by a newer version of Unswiper (format \(version))"
        } catch let error as DecodingError {
            reason = "it is damaged (\(describe(error)))"
        } catch {
            reason = "it could not be read (\(error.localizedDescription))"
        }

        do {
            let backup = try backupSnapshot()
            return LoadResult(
                snapshot: emptySnapshot,
                problem: "Your saved library couldn't be opened because \(reason). A copy was saved as \(backup.lastPathComponent) in \(backup.deletingLastPathComponent().path). Unswiper started with an empty library; re-index your folder to continue.",
                backupURL: backup
            )
        } catch {
            saveBlockedReason = "library.json couldn't be backed up (\(error.localizedDescription)); not overwriting it."
            return LoadResult(
                snapshot: emptySnapshot,
                problem: "Your saved library couldn't be opened because \(reason), and a backup copy couldn't be made. Unswiper won't overwrite it. Quit and copy \(url.path) somewhere safe, then remove it."
            )
        }
    }

    /// Copies library.json to `library.json.corrupt-<stamp>` next to it.
    @discardableResult
    static func backupSnapshot(now: Date = Date()) throws -> URL {
        let stamp = ISO8601DateFormatter().string(from: now).replacingOccurrences(of: ":", with: "-")
        var backup = supportDir.appendingPathComponent("library.json.corrupt-\(stamp)")
        var n = 1
        while FileManager.default.fileExists(atPath: backup.path) {
            n += 1
            backup = supportDir.appendingPathComponent("library.json.corrupt-\(stamp)-\(n)")
        }
        try FileManager.default.copyItem(at: snapshotURL, to: backup)
        return backup
    }

    private static func describe(_ error: DecodingError) -> String {
        if case .dataCorrupted(let c) = error { return c.debugDescription }
        if case .keyNotFound(let key, _) = error { return "missing \(key.stringValue)" }
        if case .typeMismatch(_, let c) = error { return "bad value at \(c.codingPath.map(\.stringValue).joined(separator: "."))" }
        if case .valueNotFound(_, let c) = error { return "missing value at \(c.codingPath.map(\.stringValue).joined(separator: "."))" }
        return "unknown format"
    }

    enum StoreError: LocalizedError {
        case saveBlocked(String)
        var errorDescription: String? {
            switch self { case .saveBlocked(let r): return r }
        }
    }

    static func save(_ snapshot: LibrarySnapshot) throws {
        if let reason = saveBlockedReason { throw StoreError.saveBlocked(reason) }
        var snapshot = snapshot
        snapshot.schemaVersion = LibrarySnapshot.currentSchemaVersion
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        encoder.dateEncodingStrategy = .iso8601
        let data = try encoder.encode(snapshot)
        try data.write(to: snapshotURL, options: .atomic)
    }
}
