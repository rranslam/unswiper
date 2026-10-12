import Foundation

enum LibraryStore {
    private static var supportDir: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let dir = base.appendingPathComponent("visit-media-reduce", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
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
