// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

enum LibraryStore {
    private static var supportDir: URL {
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
        let files = (try? FileManager.default.contentsOfDirectory(at: commitsDir, includingPropertiesForKeys: nil)) ?? []
        return files.filter { $0.pathExtension == "json" }
            .sorted { $0.lastPathComponent < $1.lastPathComponent }
            .last
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

    static func load() -> LibrarySnapshot {
        guard FileManager.default.fileExists(atPath: snapshotURL.path),
              let data = try? Data(contentsOf: snapshotURL)
        else {
            return LibrarySnapshot(config: .default, items: [], groups: [], lastIndexedAt: nil)
        }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return (try? decoder.decode(LibrarySnapshot.self, from: data))
            ?? LibrarySnapshot(config: .default, items: [], groups: [], lastIndexedAt: nil)
    }

    static func save(_ snapshot: LibrarySnapshot) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        encoder.dateEncodingStrategy = .iso8601
        let data = try encoder.encode(snapshot)
        try data.write(to: snapshotURL, options: .atomic)
    }
}
