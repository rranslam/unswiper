// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import XCTest
@testable import Unswiper

final class AppInfoTests: XCTestCase {
    func testVersionFromInfoPlistElseFallback() {
        XCTAssertEqual(AppInfo.version(info: ["CFBundleShortVersionString": "9.9.9"]), "9.9.9")
        XCTAssertEqual(AppInfo.version(info: ["CFBundleShortVersionString": ""]), AppInfo.fallbackVersion)
        XCTAssertEqual(AppInfo.version(info: nil), AppInfo.fallbackVersion, "swift run has no Info.plist")
    }

    func testCreditsMatchLinuxAbout() {
        let lines = AppInfo.creditsLines(notices: "GeoNames CC BY 4.0")
        XCTAssertEqual(lines.first, "Sort trip photos into visits, keep the best, set aside the rest.")
        XCTAssertTrue(lines.contains("© 2026 Rob Ranslam. MIT License."))
        XCTAssertTrue(lines.contains("https://github.com/rranslam/unswiper"))
        XCTAssertTrue(lines.contains { $0.hasSuffix("/issues") })
        XCTAssertEqual(lines.last, "GeoNames CC BY 4.0")
    }
}
