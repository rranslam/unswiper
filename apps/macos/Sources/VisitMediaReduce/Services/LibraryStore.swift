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
