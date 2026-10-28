// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import XCTest
@testable import Unswiper

final class LayoutRulesTests: XCTestCase {
    func testNarrowWindowHidesListWhileInfoIsOpen() {
        let open = LayoutRules.sidebar(inspectorOpen: true, windowWidth: 940, sidebarShown: true, autoHidden: false)
        XCTAssertEqual(open.show, false)
        XCTAssertEqual(open.autoHidden, true)
        let closed = LayoutRules.sidebar(inspectorOpen: false, windowWidth: 940, sidebarShown: false, autoHidden: true)
        XCTAssertEqual(closed.show, true, "the list comes back")
        XCTAssertEqual(closed.autoHidden, false)
    }

    func testWideWindowKeepsTheList() {
        let r = LayoutRules.sidebar(inspectorOpen: true, windowWidth: 1280, sidebarShown: true, autoHidden: false)
        XCTAssertEqual(r.show, true)
        XCTAssertEqual(r.autoHidden, false)
    }

    func testListTheUserHidHimselfStaysHidden() {
        let r = LayoutRules.sidebar(inspectorOpen: false, windowWidth: 940, sidebarShown: false, autoHidden: false)
        XCTAssertEqual(r.show, false)
        let o = LayoutRules.sidebar(inspectorOpen: true, windowWidth: 940, sidebarShown: false, autoHidden: false)
        XCTAssertEqual(o.show, false)
        XCTAssertEqual(o.autoHidden, false, "not hidden by Unswiper, so not shown again on close")
    }
}
