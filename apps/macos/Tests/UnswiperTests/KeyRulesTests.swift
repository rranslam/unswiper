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
        XCTAssertNil(cmd("", KeyRules.leftKey), "arrows move through the visits list")
        XCTAssertNil(cmd("", KeyRules.rightKey))
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
}
