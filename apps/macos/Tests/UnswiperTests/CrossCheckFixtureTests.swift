// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation
import XCTest
@testable import Unswiper

/// Writes the Swift app's results for the Rust core cross-check (docs/linux-port-plan.md §10.1, §10.5).
/// Test-only and opt-in: skipped unless UNSWIPER_WRITE_CROSSCHECK=1.
///
///     cd apps/macos
///     UNSWIPER_WRITE_CROSSCHECK=1 TZ=America/Los_Angeles swift test --filter CrossCheckFixtureTests
///
/// Input:  crates/unswiper-core/tests/fixtures/crosscheck/spec.json
/// Output: crates/unswiper-core/tests/fixtures/crosscheck/swift/{library.json,table.json}
final class CrossCheckFixtureTests: StoreIsolatedTestCase {
    struct Spec: Decodable {
        struct File: Decodable {
            var path: String
            var exif: String?
            var tiff: String?
            var bytes: Int?
        }
        var gapHours: Double
        var files: [File]
    }

    struct Table: Encodable {
        struct Item: Encodable {
            var path: String
            var kind: String
            var dateSource: String?
            var takenAt: Date
            var isUndated: Bool
            var folderPath: String?
            var relativeFolder: String
            var sidecars: [String]
        }
        struct Group: Encodable {
            var id: String
            var title: String
            var startDate: Date
            var endDate: Date
            var itemPaths: [String]
            var folderHint: String
        }
        struct Hash: Encodable {
            var input: String
            var hash: String
        }
        var root: String
        var timeZone: String
        var locale: String
        var gapHours: Double
        var items: [Item]
        var groups: [Group]
        var fnv: [Hash]
    }

    /// Repo root, from this file's location (apps/macos/Tests/UnswiperTests/<file>).
    static var repoRoot: URL {
        var url = URL(fileURLWithPath: #filePath)
        for _ in 0..<5 { url.deleteLastPathComponent() }
        return url
    }

    func testWriteCrossCheckFixtures() throws {
        guard ProcessInfo.processInfo.environment["UNSWIPER_WRITE_CROSSCHECK"] == "1" else {
            throw XCTSkip("Set UNSWIPER_WRITE_CROSSCHECK=1 to write the Rust cross-check fixtures.")
        }
        let tz = try XCTUnwrap(TimeZone(identifier: "America/Los_Angeles"))
        let savedTZ = NSTimeZone.default
        NSTimeZone.default = tz
        defer { NSTimeZone.default = savedTZ }

        let fixtures = Self.repoRoot.appendingPathComponent("crates/unswiper-core/tests/fixtures/crosscheck", isDirectory: true)
        let spec = try JSONDecoder().decode(Spec.self, from: Data(contentsOf: fixtures.appendingPathComponent("spec.json")))

        let root = try makeRoot()
        for f in spec.files {
            let url = root.appendingPathComponent(f.path)
            if f.exif != nil || f.tiff != nil {
                try Fixture.jpeg(url, exifDate: f.exif, tiffDate: f.tiff)
            } else {
                try Fixture.file(url, bytes: f.bytes ?? 16)
            }
        }

        let (items, groups) = try LocalTreeIndexer.index(
            root: root, visitGapHours: spec.gapHours, existingDecisions: [:], onProgress: { _ in }
        )

        let out = fixtures.appendingPathComponent("swift", isDirectory: true)
        try FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)
        LibraryStore.supportDirOverride = out
        var config = AppConfig.default
        config.rootPath = root.path
        config.keepTargets = ["visit-example": 3]
        try LibraryStore.save(LibrarySnapshot(
            config: config, items: items, groups: groups,
            lastIndexedAt: Date(timeIntervalSince1970: 1_760_000_000)
        ))

        let prefix = root.path + "/"
        func rel(_ p: String) -> String { p.hasPrefix(prefix) ? String(p.dropFirst(prefix.count)) : p }
        let table = Table(
            root: root.path,
            timeZone: tz.identifier,
            locale: Locale.current.identifier,
            gapHours: spec.gapHours,
            items: items.map {
                Table.Item(path: rel($0.path), kind: $0.kind.rawValue, dateSource: $0.dateSource?.rawValue,
                           takenAt: $0.takenAt, isUndated: $0.isUndated, folderPath: $0.folderPath,
                           relativeFolder: $0.relativeFolder, sidecars: $0.sidecars.map { rel($0.path) })
            },
            groups: groups.map {
                Table.Group(id: $0.id, title: $0.title, startDate: $0.startDate, endDate: $0.endDate,
                            itemPaths: $0.itemPaths.map(rel), folderHint: $0.folderHint)
            },
            fnv: ["", "a", "foobar", "/Users/rob/Pictures/IMG_0001.HEIC|1767268800", "Lisbon/Été 2026", "Porto/old"]
                .map { Table.Hash(input: $0, hash: VisitGrouper.fnv1a($0)) }
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        encoder.dateEncodingStrategy = .iso8601
        try encoder.encode(table).write(to: out.appendingPathComponent("table.json"), options: .atomic)
        XCTAssertFalse(items.isEmpty)
    }
}
