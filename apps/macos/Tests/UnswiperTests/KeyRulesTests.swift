// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import XCTest
@testable import Unswiper

final class KeyRulesTests: XCTestCase {
    private func cmd(_ c: String, _ code: UInt16 = 0, mods: Bool = false, editing: Bool = false,
                     viewer: Bool = false) -> KeyCommand? {
        KeyRules.command(characters: c, keyCode: code, hasModifiers: mods, editingText: editing, viewerOpen: viewer)
    }

    func testLettersInGridAndViewer() {
        for viewer in [false, true] {
            XCTAssertEqual(cmd("k", 40, viewer: viewer), .keep)
            XCTAssertEqual(cmd("r", 15, viewer: viewer), .reject)
            XCTAssertEqual(cmd("u", 32, viewer: viewer), .clear)
            XCTAssertEqual(cmd("i", 34, viewer: viewer), .info)
            XCTAssertNil(cmd("x", 7, viewer: viewer))
        }
    }

    func testSpaceEscapeArrows() {
        XCTAssertEqual(cmd(" ", KeyRules.spaceKey), .view)
        XCTAssertEqual(cmd(" ", KeyRules.spaceKey, viewer: true), .close)
        XCTAssertNil(cmd("\u{1b}", KeyRules.escapeKey), "Esc without the large photo is left to the page")
        XCTAssertEqual(cmd("\u{1b}", KeyRules.escapeKey, viewer: true), .close)
        XCTAssertEqual(cmd("", KeyRules.leftKey), .previous, "←/→ move the grid selection too")
        XCTAssertEqual(cmd("", KeyRules.rightKey), .next)
        XCTAssertEqual(cmd("", KeyRules.leftKey, viewer: true), .previous)
        XCTAssertEqual(cmd("", KeyRules.rightKey, viewer: true), .next)
    }

    func testTypingAndModifiersAreLeftAlone() {
        XCTAssertNil(cmd("k", 40, editing: true), "typing in a text field (Blur Cutoffs) isn't taken")
        XCTAssertNil(cmd(" ", KeyRules.spaceKey, editing: true, viewer: true))
        XCTAssertNil(cmd("r", 15, mods: true), "⇧⌘R is Re-index")
        XCTAssertNil(cmd("i", 34, mods: true), "⌘I is the menu item")
    }

    func testUnbundledLaunch() {
        XCTAssertTrue(LaunchRules.isUnbundled(bundleURL: URL(fileURLWithPath: "/src/apps/macos/.build/debug")))
        XCTAssertFalse(LaunchRules.isUnbundled(bundleURL: URL(fileURLWithPath: "/Applications/Unswiper.app")))
        XCTAssertFalse(LaunchRules.isUnbundled(bundleURL: URL(fileURLWithPath: "/tmp/Unswiper.APP")))
    }

    func testGridKeys() {
        XCTAssertEqual(cmd("", KeyRules.upKey), .up)
        XCTAssertEqual(cmd("", KeyRules.downKey), .down)
        XCTAssertEqual(cmd("", KeyRules.pageUpKey), .pageUp)
        XCTAssertEqual(cmd("", KeyRules.pageDownKey), .pageDown)
        XCTAssertEqual(cmd("", KeyRules.homeKey), .home)
        XCTAssertEqual(cmd("", KeyRules.endKey), .end)
        for code in [KeyRules.upKey, KeyRules.downKey, KeyRules.pageUpKey, KeyRules.pageDownKey,
                     KeyRules.homeKey, KeyRules.endKey] {
            XCTAssertNil(cmd("", code, viewer: true), "the large photo leaves rows and pages alone")
            XCTAssertNil(cmd("", code, editing: true), "text fields keep their keys")
            XCTAssertNil(cmd("", code, mods: true))
        }
    }

    func testGridColumnsAndPages() {
        XCTAssertEqual(GridNav.columns(width: 0), 1)
        XCTAssertEqual(GridNav.columns(width: 32 + 110), 1)
        XCTAssertEqual(GridNav.columns(width: 32 + 110 * 2 + 8), 2)
        XCTAssertEqual(GridNav.columns(width: 32 + 110 * 2 + 7), 1)
        XCTAssertEqual(GridNav.columns(width: 800), 6)
        XCTAssertGreaterThanOrEqual(GridNav.rowsPerPage(width: 800, height: 600), 4)
        XCTAssertEqual(GridNav.rowsPerPage(width: 800, height: 10), 1)
    }

    func testGridTargets() {
        func t(_ from: Int?, _ c: KeyCommand, count: Int = 20) -> Int? {
            GridNav.target(from: from, count: count, columns: 5, rowsPerPage: 2, command: c)
        }
        XCTAssertEqual(t(nil, .next), 0, "nothing selected: start on the first photo")
        XCTAssertEqual(t(nil, .end), 19)
        XCTAssertEqual(t(7, .next), 8)
        XCTAssertEqual(t(7, .previous), 6)
        XCTAssertEqual(t(0, .previous), 0, "no wrap")
        XCTAssertEqual(t(19, .next), 19)
        XCTAssertEqual(t(7, .down), 12)
        XCTAssertEqual(t(7, .up), 2)
        XCTAssertEqual(t(2, .up), 0, "clamped to the first photo")
        XCTAssertEqual(t(17, .down), 19, "clamped to the last photo")
        XCTAssertEqual(t(3, .pageDown), 13)
        XCTAssertEqual(t(13, .pageUp), 3)
        XCTAssertEqual(t(9, .home), 0)
        XCTAssertEqual(t(9, .end), 19)
        XCTAssertNil(t(9, .keep))
        XCTAssertNil(t(nil, .down, count: 0))
    }
}
