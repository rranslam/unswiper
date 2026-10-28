// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import XCTest
@testable import Unswiper

final class NavigationTests: XCTestCase {
    func testVisitIDAndFullWindow() {
        XCTAssertNil(AppPath.home.visitID)
        XCTAssertNil(AppPath.setup.visitID)
        XCTAssertEqual(AppPath.reduce("a").visitID, "a")
        XCTAssertEqual(AppPath.commit("a").visitID, "a", "the visit stays highlighted while committing")
        XCTAssertEqual(AppPath.compare("a", ["x", "y"]).visitID, "a")
        XCTAssertTrue(AppPath.setup.isFullWindow)
        for p in [AppPath.home, .reduce("a"), .commit("a"), .compare("a", [])] {
            XCTAssertFalse(p.isFullWindow, "\(p) sits beside the visits list")
        }
    }

    func testSelectingAVisit() {
        XCTAssertEqual(AppPath.home.selecting("a"), .reduce("a"))
        XCTAssertEqual(AppPath.reduce("a").selecting("b"), .reduce("b"), "another visit opens on its grid")
        XCTAssertEqual(AppPath.commit("a").selecting("b"), .reduce("b"))
        XCTAssertEqual(AppPath.compare("a", ["x"]).selecting("b"), .reduce("b"))
        XCTAssertEqual(AppPath.commit("a").selecting("a"), .commit("a"), "the open visit keeps its page")
        XCTAssertEqual(AppPath.reduce("a").selecting(nil), .home, "no selection shows the placeholder")
        XCTAssertEqual(AppPath.setup.selecting(nil), .setup)
    }

    func testRegroupKeepsOrClosesThePage() {
        let ids: Set<String> = ["a", "b"]
        XCTAssertEqual(AppPath.reduce("a").keeping(visits: ids), .reduce("a"))
        XCTAssertEqual(AppPath.commit("b").keeping(visits: ids), .commit("b"))
        XCTAssertEqual(AppPath.reduce("gone").keeping(visits: ids), .home)
        XCTAssertEqual(AppPath.compare("gone", ["x"]).keeping(visits: ids), .home)
        XCTAssertEqual(AppPath.setup.keeping(visits: []), .setup)
        XCTAssertEqual(AppPath.home.keeping(visits: []), .home)
    }
}
