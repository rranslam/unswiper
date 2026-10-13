// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import XCTest
@testable import Unswiper

final class IndexerTests: StoreIsolatedTestCase {
    private func index(_ root: URL) throws -> (items: [MediaItem], groups: [VisitGroup]) {
        try LocalTreeIndexer.index(root: root, visitGapHours: 36, existingDecisions: [:], onProgress: { _ in })
    }

    // MARK: Dates

    func testExifDateOriginal() throws {
        let root = try makeRoot()
        let url = try Fixture.jpeg(root.appendingPathComponent("a.jpg"), exifDate: "2024:05:01 10:20:30")
        XCTAssertEqual(LocalTreeIndexer.exifDate(for: url), Fixture.localDate(2024, 5, 1, 10, 20, 30))
    }

    func testJpegWithoutDateHasNoCaptureDate() throws {
        // (ImageIO doesn't write a TIFF DateTime into JPEGs, so the TIFF fallback is covered by the Rust fixtures later.)
        let root = try makeRoot()
        let url = try Fixture.jpeg(root.appendingPathComponent("a.jpg"))
        XCTAssertNil(LocalTreeIndexer.exifDate(for: url))
        XCTAssertNil(LocalTreeIndexer.exifDate(for: try Fixture.file(root.appendingPathComponent("junk.jpg"))))
    }

    func testPre1980ExifDateIsKept() throws {
        // Scanned film photos can legitimately carry 1970s capture dates.
        let root = try makeRoot()
        try Fixture.jpeg(root.appendingPathComponent("scan.jpg"), exifDate: "1975:06:01 10:00:00")
        let result = try index(root)
        XCTAssertEqual(result.items.count, 1)
        XCTAssertEqual(result.items[0].takenAt, Fixture.localDate(1975, 6, 1, 10))
        XCTAssertEqual(result.items[0].dateSource, .capture)
        XCTAssertFalse(result.items[0].isUndated)
    }

    func testUsableRejectsEpochLikeFileDates() {
        XCTAssertNil(LocalTreeIndexer.usable(Date(timeIntervalSince1970: 0)))
        XCTAssertNil(LocalTreeIndexer.usable(Fixture.localDate(1979, 12, 1)))
        XCTAssertNotNil(LocalTreeIndexer.usable(Fixture.localDate(1981, 1, 1)))
        XCTAssertNil(LocalTreeIndexer.usable(nil))
    }

    func testFileModificationDateIsNotUsedForGrouping() throws {
        let root = try makeRoot()
        try Fixture.file(root.appendingPathComponent("clip.mp4"), modifiedAt: Fixture.localDate(2022, 2, 2))
        let item = try XCTUnwrap(index(root).items.first)
        XCTAssertTrue(item.isUndated)
        XCTAssertEqual(item.dateSource, .unknown)
        XCTAssertEqual(item.modifiedAt, Fixture.localDate(2022, 2, 2), "mtime still recorded for commit checks")
    }

    // MARK: Pairing

    func testLivePhotoAndSidecarPairingIsCaseInsensitive() throws {
        let root = try makeRoot()
        try Fixture.file(root.appendingPathComponent("IMG_0001.HEIC"), bytes: 100)
        try Fixture.file(root.appendingPathComponent("img_0001.mov"), bytes: 50)
        try Fixture.file(root.appendingPathComponent("IMG_0001.AAE"), bytes: 5)
        try Fixture.file(root.appendingPathComponent("IMG_0001.xmp"), bytes: 6)
        let items = try index(root).items
        XCTAssertEqual(items.count, 1)
        let live = items[0]
        XCTAssertEqual(live.fileName, "IMG_0001.HEIC")
        XCTAssertEqual(Set(live.sidecars.map { URL(fileURLWithPath: $0.path).lastPathComponent }), ["img_0001.mov", "IMG_0001.AAE", "IMG_0001.xmp"])
        XCTAssertEqual(live.totalBytes, 161)
        XCTAssertEqual(live.fileCount, 4)
    }

    func testOnlyMovPairsWithStill() throws {
        let root = try makeRoot()
        try Fixture.file(root.appendingPathComponent("IMG_0002.JPG"))
        try Fixture.file(root.appendingPathComponent("IMG_0002.MP4"))
        let items = try index(root).items
        XCTAssertEqual(items.count, 2)
        XCTAssertTrue(items.allSatisfy { $0.sidecars.isEmpty })
    }

    func testPngIsNotALivePhotoStill() throws {
        let root = try makeRoot()
        try Fixture.file(root.appendingPathComponent("shot.png"))
        try Fixture.file(root.appendingPathComponent("shot.MOV"))
        let items = try index(root).items
        XCTAssertEqual(items.count, 2)
        XCTAssertEqual(Set(items.map(\.kind)), [.photo, .video])
    }

    func testPairingRequiresSameFolder() throws {
        let root = try makeRoot()
        try Fixture.file(root.appendingPathComponent("a/IMG_1.HEIC"))
        try Fixture.file(root.appendingPathComponent("b/IMG_1.MOV"))
        XCTAssertEqual(try index(root).items.count, 2)
    }

    func testOrphanSidecarsAndRejectedFolderAreIgnored() throws {
        let root = try makeRoot()
        try Fixture.file(root.appendingPathComponent("lonely.xmp"))
        try Fixture.file(root.appendingPathComponent("_Rejected/Unswiper/x/old.jpg"))
        try Fixture.file(root.appendingPathComponent("notes.txt"))
        try Fixture.file(root.appendingPathComponent(".hidden.jpg"))
        XCTAssertEqual(try index(root).items.count, 0)
    }

    func testExistingDecisionsAreKept() throws {
        let root = try makeRoot()
        let a = try Fixture.file(root.appendingPathComponent("a.jpg"))
        let result = try LocalTreeIndexer.index(root: root, visitGapHours: 36, existingDecisions: [a.path: .reject], onProgress: { _ in })
        XCTAssertEqual(result.items.first?.decision, .reject)
    }

    // MARK: Undated rule end-to-end (plan §7 fixture folders)

    func testUndatedRuleFixtureTree() throws {
        let root = try makeRoot()
        try Fixture.file(root.appendingPathComponent("2026/01/01/a.jpg"))
        try Fixture.file(root.appendingPathComponent("2025-07-04 Beach/b.jpg"))
        try Fixture.jpeg(root.appendingPathComponent("Lisbon/dated.jpg"), exifDate: "2024:05:01 10:00:00")
        try Fixture.file(root.appendingPathComponent("Lisbon/undated.jpg"))
        try Fixture.file(root.appendingPathComponent("Misc/c.jpg"))
        try Fixture.file(root.appendingPathComponent("loose.jpg"))

        let (items, groups) = try index(root)
        func item(_ name: String) -> MediaItem { items.first { $0.fileName == name }! }

        XCTAssertEqual(item("a.jpg").takenAt, Fixture.localDate(2026, 1, 1, 12))
        XCTAssertEqual(item("a.jpg").dateSource, .folder)
        XCTAssertEqual(item("a.jpg").folderPath, "2026/01/01")
        XCTAssertEqual(item("b.jpg").takenAt, Fixture.localDate(2025, 7, 4, 12))
        XCTAssertEqual(item("dated.jpg").dateSource, .capture)
        XCTAssertTrue(item("undated.jpg").isUndated)
        XCTAssertEqual(item("loose.jpg").folderPath, "")

        XCTAssertEqual(groups.count, 5)
        let lisbon = groups.first { $0.itemPaths.contains(item("dated.jpg").path) }!
        XCTAssertTrue(lisbon.itemPaths.contains(item("undated.jpg").path))
        XCTAssertEqual(groups[groups.count - 2].title, "Misc")
        XCTAssertEqual(groups.last?.id, "undated")
        XCTAssertEqual(groups.last?.itemPaths, [item("loose.jpg").path])
    }

    func testRelativeFolderPath() throws {
        let root = try makeRoot()
        XCTAssertEqual(LocalTreeIndexer.relativeFolderPath(of: root, root: root), "")
        XCTAssertEqual(LocalTreeIndexer.relativeFolderPath(of: root.appendingPathComponent("a/b"), root: root), "a/b")
        XCTAssertEqual(LocalTreeIndexer.relativeFolderPath(of: URL(fileURLWithPath: "/elsewhere"), root: root), "")
    }
}
