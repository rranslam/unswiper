// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import XCTest
@testable import Unswiper

final class GroupingTests: XCTestCase {
    private func item(_ path: String, _ t: TimeInterval?, folder: String? = "", bytes: Int64 = 10, kind: MediaKind = .photo) -> MediaItem {
        MediaItem(path: path, kind: kind, sizeBytes: bytes,
                  takenAt: t.map { Date(timeIntervalSince1970: $0) } ?? .distantPast,
                  relativeFolder: folder?.split(separator: "/").first.map(String.init) ?? "",
                  decision: .undecided, isUndated: t == nil,
                  dateSource: t == nil ? DateSource.unknown : .capture, folderPath: folder)
    }

    let t0: TimeInterval = 1_700_000_000
    let h: TimeInterval = 3600

    func testGapExactlyAtThresholdStaysTogether() {
        let groups = VisitGrouper.group(items: [item("/l/a", t0), item("/l/b", t0 + 36 * h)], gapHours: 36)
        XCTAssertEqual(groups.count, 1)
        XCTAssertEqual(groups[0].itemCount, 2)
    }

    func testGapJustOverThresholdSplits() {
        let groups = VisitGrouper.group(items: [item("/l/a", t0), item("/l/b", t0 + 36 * h + 1)], gapHours: 36)
        XCTAssertEqual(groups.count, 2)
    }

    func testInputOrderDoesNotMatter() {
        let items = [item("/l/c", t0 + 100 * h), item("/l/a", t0), item("/l/b", t0 + h)]
        let groups = VisitGrouper.group(items: items, gapHours: 36)
        XCTAssertEqual(groups.count, 2)
        let first = groups.first { $0.itemPaths.contains("/l/a") }!
        XCTAssertEqual(first.itemPaths, ["/l/a", "/l/b"])
        XCTAssertEqual(first.startDate, Date(timeIntervalSince1970: t0))
        XCTAssertEqual(first.endDate, Date(timeIntervalSince1970: t0 + h))
    }

    func testVisitsSortedOldestFirstNotBySize() {
        let items = [item("/l/mid", t0 + 200 * h, bytes: 50), item("/l/small", t0, bytes: 1), item("/l/big", t0 + 100 * h, bytes: 1000)]
        let groups = VisitGrouper.group(items: items, gapHours: 36)
        XCTAssertEqual(groups.map(\.itemPaths), [["/l/small"], ["/l/big"], ["/l/mid"]])
    }

    func testStableIDPinnedValue() {
        // Cross-language oracle for the Rust port (plan §10.1): FNV-1a 64 of "<firstPath>|<unix seconds>".
        XCTAssertEqual(VisitGrouper.stableGroupID(firstPath: "/lib/a.jpg", start: Date(timeIntervalSince1970: 1_700_000_000)),
                       "visit-fc0f80072fd6ccbc")
    }

    func testIDStableWhenItemsRemoved() {
        let full = VisitGrouper.group(items: [item("/l/a", t0), item("/l/b", t0 + h), item("/l/c", t0 + 2 * h)], gapHours: 36)
        let fewer = VisitGrouper.group(items: [item("/l/a", t0), item("/l/b", t0 + h)], gapHours: 36)
        XCTAssertEqual(full[0].id, fewer[0].id)
        XCTAssertEqual(full[0].id, VisitGrouper.stableGroupID(firstPath: "/l/a", start: Date(timeIntervalSince1970: t0)))
    }

    func testCountsAndBytes() {
        let items = [item("/l/a", t0, bytes: 5), item("/l/v", t0 + 1, bytes: 7, kind: .video)]
        let g = VisitGrouper.group(items: items, gapHours: 36)[0]
        XCTAssertEqual(g.photoCount, 1)
        XCTAssertEqual(g.videoCount, 1)
        XCTAssertEqual(g.totalBytes, 12)
    }

    // MARK: Undated rule (plan §7)

    func testRootUndatedGoesToUndatedGroup() {
        let groups = VisitGrouper.group(items: [item("/l/a", t0), item("/l/x", nil, folder: "")], gapHours: 36)
        XCTAssertEqual(groups.last?.id, "undated")
        XCTAssertEqual(groups.last?.itemPaths, ["/l/x"])
    }

    func testLegacyUndatedWithoutFolderPathGoesToUndated() {
        let groups = VisitGrouper.group(items: [item("/l/x", nil, folder: nil)], gapHours: 36)
        XCTAssertEqual(groups.map(\.id), ["undated"])
    }

    func testUndatedJoinsVisitOfDatedSiblings() {
        let items = [
            item("/l/Lisbon/a", t0, folder: "Lisbon"),
            item("/l/Lisbon/b", t0 + h, folder: "Lisbon"),
            item("/l/Other/c", t0 + 500 * h, folder: "Other"),
            item("/l/Lisbon/u", nil, folder: "Lisbon"),
        ]
        let groups = VisitGrouper.group(items: items, gapHours: 36)
        XCTAssertEqual(groups.count, 2)
        let lisbon = groups.first { $0.itemPaths.contains("/l/Lisbon/a") }!
        XCTAssertTrue(lisbon.itemPaths.contains("/l/Lisbon/u"))
        XCTAssertEqual(lisbon.endDate, Date(timeIntervalSince1970: t0 + h), "undated sibling must not change dates")
        XCTAssertEqual(lisbon.id, VisitGrouper.stableGroupID(firstPath: "/l/Lisbon/a", start: Date(timeIntervalSince1970: t0)))
    }

    func testUndatedJoinsMajorityVisitOfSiblings() {
        let items = [
            item("/l/F/a", t0, folder: "F"),
            item("/l/F/b", t0 + 500 * h, folder: "F"),
            item("/l/F/c", t0 + 501 * h, folder: "F"),
            item("/l/F/u", nil, folder: "F"),
        ]
        let groups = VisitGrouper.group(items: items, gapHours: 36)
        let majority = groups.first { $0.itemPaths.contains("/l/F/b") }!
        XCTAssertTrue(majority.itemPaths.contains("/l/F/u"))
    }

    func testFolderWithoutDatedFilesBecomesNamedVisit() {
        let items = [
            item("/l/a", t0),
            item("/l/Zoo/x", nil, folder: "Zoo"),
            item("/l/Trips/Alps/y", nil, folder: "Trips/Alps"),
            item("/l/Trips/Alps/z", nil, folder: "Trips/Alps"),
            item("/l/r", nil, folder: ""),
        ]
        let groups = VisitGrouper.group(items: items, gapHours: 36)
        XCTAssertEqual(groups.map(\.title).suffix(3), ["Alps", "Zoo", "Undated"], "named folders sort by folder name, Undated last")
        let alps = groups.first { $0.title == "Alps" }!
        XCTAssertEqual(alps.itemCount, 2)
        XCTAssertEqual(alps.id, "folder-" + VisitGrouper.fnv1a("Trips/Alps"))
        XCTAssertEqual(alps.startDate, .distantPast)
    }

    func testSiblingMatchUsesExactFolderNotParent() {
        // Dated file in "Trips", undated in "Trips/Alps": no dated sibling in the same folder.
        let items = [item("/l/Trips/a", t0, folder: "Trips"), item("/l/Trips/Alps/u", nil, folder: "Trips/Alps")]
        let groups = VisitGrouper.group(items: items, gapHours: 36)
        XCTAssertEqual(groups.count, 2)
        XCTAssertTrue(groups.contains { $0.title == "Alps" })
    }
}
