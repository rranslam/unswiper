// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import XCTest
@testable import Unswiper

private func photo(_ path: String, _ d: Decision = .undecided) -> MediaItem {
    MediaItem(path: path, kind: .photo, sizeBytes: 1, takenAt: Date(), relativeFolder: "", decision: d)
}

private func visit(_ id: String, _ paths: [String]) -> VisitGroup {
    VisitGroup(id: id, title: id, startDate: Date(), endDate: Date(), itemPaths: paths,
               photoCount: paths.count, videoCount: 0, totalBytes: Int64(paths.count), folderHint: "")
}

final class VisitCountsTests: StoreIsolatedTestCase {
    func testOnePassMatchesTheOldCount() {
        let items = [photo("/a", .keep), photo("/b", .reject), photo("/c"), photo("/d", .reject), photo("/e")]
        let groups = [visit("v1", ["/a", "/b", "/c"]), visit("v2", ["/d", "/e", "/missing"]), visit("v3", [])]
        let counts = VisitCounts(items: items, groups: groups)
        for g in groups {
            let old = g.progress(in: items)
            XCTAssertEqual(counts[g.id], VisitCounts.Count(kept: old.kept, rejected: old.rejected, left: old.left), g.id)
        }
        XCTAssertEqual(counts["nope"], VisitCounts.Count())
    }

    func testUpdateMovesOnlyThatPhoto() {
        let items = [photo("/a"), photo("/b"), photo("/c")]
        // "/b" is in two visits (place grouping can do that).
        var counts = VisitCounts(items: items, groups: [visit("v1", ["/a", "/b"]), visit("v2", ["/b", "/c"])])
        counts.update(path: "/b", from: .undecided, to: .reject)
        XCTAssertEqual(counts["v1"], VisitCounts.Count(kept: 0, rejected: 1, left: 1))
        XCTAssertEqual(counts["v2"], VisitCounts.Count(kept: 0, rejected: 1, left: 1))
        counts.update(path: "/b", from: .reject, to: .keep)
        counts.update(path: "/a", from: .undecided, to: .undecided)
        counts.update(path: "/zzz", from: .undecided, to: .keep)
        XCTAssertEqual(counts["v1"], VisitCounts.Count(kept: 1, rejected: 0, left: 1))
        XCTAssertEqual(counts["v2"], VisitCounts.Count(kept: 1, rejected: 0, left: 1))
    }

    @MainActor func testAppStateKeepsCountsInStep() {
        let state = AppState()
        state.saveDelay = .milliseconds(10)
        state.items = [photo("/a"), photo("/b"), photo("/c", .keep)]
        state.groups = [visit("v", ["/a", "/b", "/c"])]
        XCTAssertEqual(state.visitCounts["v"], VisitCounts.Count(kept: 1, rejected: 0, left: 2))
        state.updateDecision(paths: ["/a", "/b"], decision: .reject)
        XCTAssertEqual(state.visitCounts["v"], VisitCounts.Count(kept: 1, rejected: 2, left: 0))
        // Whole-library changes (re-index, blur) rebuild the counts.
        state.items[0].decision = .undecided
        XCTAssertEqual(state.visitCounts["v"], VisitCounts.Count(kept: 1, rejected: 1, left: 1))
        XCTAssertEqual(state.visitCounts, VisitCounts(items: state.items, groups: state.groups))
        XCTAssertEqual(state.items(in: state.groups[0]).map(\.path), ["/a", "/b", "/c"])
    }

    /// 78k photos in 4,000 visits: counting once and a decision change stay fast.
    func testLargeLibraryIsQuick() {
        let items = (0..<78_000).map { photo("/p/\($0).jpg") }
        let groups = (0..<4_000).map { g in visit("v\(g)", (0..<19).map { "/p/\(g * 19 + $0).jpg" }) }
        let start = Date()
        var counts = VisitCounts(items: items, groups: groups)
        for i in 0..<1_000 { counts.update(path: "/p/\(i).jpg", from: .undecided, to: .reject) }
        XCTAssertLessThan(Date().timeIntervalSince(start), 5)
        XCTAssertEqual(counts["v0"].rejected, 19)
    }
}
