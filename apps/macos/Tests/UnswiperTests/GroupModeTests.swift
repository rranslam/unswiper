// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation
import XCTest
@testable import Unswiper

/// Grouping picker (plan §20.6, M2): saved in `AppConfig.groupMode`, regroups through the helper.
final class GroupModeTests: StoreIsolatedTestCase {
    private func item(_ path: String, at seconds: TimeInterval) -> MediaItem {
        MediaItem(path: path, kind: .photo, sizeBytes: 1, takenAt: Date(timeIntervalSince1970: seconds),
                  relativeFolder: "", decision: .undecided)
    }

    func testModeStringsMatchRust() {
        XCTAssertEqual(GroupMode.allCases.map(\.rawValue), ["visit", "geo", "visit-geo"])
        XCTAssertEqual(AppConfig.default.grouping, .visit)
        XCTAssertNil(AppConfig.default.groupMode, "not written until the user picks a mode")
    }

    func testHelperArgumentsAndAnswer() throws {
        let args = GroupHelper.arguments(library: URL(fileURLWithPath: "/s/library.json"), mode: .visitGeo)
        XCTAssertEqual(args, ["group", "--library", "/s/library.json", "--mode", "visit-geo"])
        let json = """
        {"groups":[{"endDate":"2026-06-01T10:00:00Z","folderHint":"","id":"g-abc-20260601","itemPaths":["/p/a.jpg"],
          "parentVisitId":"g-abc","photoCount":1,"place":{"admin1":"","country":"PT","lat":1.5,"lon":2.5,"name":"Town"},
          "startDate":"2026-06-01T10:00:00Z","title":"Jun 1 · Town, PT","totalBytes":1,"videoCount":0}],
         "locations":{"/p/a.jpg":{"lat":1.5,"lon":2.5,"source":"exif"}}}
        """
        let answer = try GroupHelper.parse(Data(json.utf8))
        XCTAssertEqual(answer.groups.first?.parentVisitId, "g-abc")
        XCTAssertEqual(answer.groups.first?.place?.country, "PT")
        let items = GroupHelper.apply(answer.locations, to: [item("/p/a.jpg", at: 0), item("/p/b.jpg", at: 1)])
        XCTAssertEqual(items[0].location?.source, "exif")
        XCTAssertNil(items[1].location)
    }

    @MainActor func testPickerSavesModeRegroupsAndReturnsToVisit() async throws {
        let state = AppState()
        state.saveDelay = .milliseconds(10)
        state.items = [item("/p/a.jpg", at: 1_780_000_000), item("/p/b.jpg", at: 1_780_000_100)]
        state.groups = VisitGrouper.group(items: state.items, gapHours: 36)
        let visitGroups = state.groups
        var calls: [GroupMode] = []
        state.groupRunner = { library, mode in
            calls.append(mode)
            XCTAssertTrue(FileManager.default.fileExists(atPath: library.path), "library.json flushed first")
            var g = visitGroups[0]
            g.id = "g-abc-20260528"
            g.parentVisitId = "g-abc"
            g.place = Place(name: "Town", admin1: "", country: "PT", lat: 1.5, lon: 2.5)
            return GroupHelper.Answer(groups: [g], locations: ["/p/a.jpg": GeoLocation(lat: 1.5, lon: 2.5, source: "exif")])
        }
        state.setGroupMode(.geo)
        XCTAssertEqual(state.config.groupMode, "geo")
        try await waitUntil { state.groups.first?.parentVisitId == "g-abc" }
        XCTAssertEqual(calls, [.geo])
        XCTAssertEqual(state.items[0].location?.lat, 1.5)
        state.flush()
        let saved = LibraryStore.load().snapshot
        XCTAssertEqual(saved.config.grouping, .geo)
        XCTAssertEqual(saved.schemaVersion, 2)
        XCTAssertEqual(saved.groups.first?.place?.name, "Town")

        state.setGroupMode(.visit)
        try await waitUntil { state.groups == visitGroups }
        XCTAssertEqual(calls.count, 1, "visit mode never calls the helper")
        XCTAssertEqual(state.config.groupMode, "visit")
    }

    @MainActor private func waitUntil(_ cond: @escaping () -> Bool, timeout: Double = 5) async throws {
        let end = Date().addingTimeInterval(timeout)
        while !cond() {
            if Date() > end { XCTFail("timed out"); return }
            try await Task.sleep(for: .milliseconds(20))
        }
    }
}

final class AboutPanelTests: XCTestCase {
    func testNoticesFallBackToTheGeoNamesAttribution() {
        // The test bundle has no THIRD_PARTY_NOTICES.txt; the app bundle gets it from build-dmg.sh.
        let text = AboutPanel.noticesText(bundle: Bundle(for: AboutPanelTests.self))
        XCTAssertTrue(text.contains("GeoNames"))
        XCTAssertTrue(text.contains("CC BY 4.0"))
    }
}

final class GeoHelpTests: XCTestCase {
    private func page(_ name: String) throws -> String {
        try String(contentsOf: TakeoutHelperTests.repoRoot
            .appendingPathComponent("apps/macos/Sources/Unswiper/Resources/Help/\(name)"), encoding: .utf8)
    }

    func testHelpCoversGroupingAndLocationPrivacy() throws {
        let privacy = try page("09-privacy.md")
        for must in ["only in its own library file on this Mac", "group photos by place", "looked up offline"] {
            XCTAssertTrue(privacy.contains(must), must)
        }
        let visits = try page("02-visits.md")
        for must in ["View ▸ Grouping", "By Place", "By Visit and Place", "Other places", "No location"] {
            XCTAssertTrue(visits.contains(must), must)
        }
        XCTAssertTrue(try page("11-about.md").contains("GeoNames"))
    }
}
