// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import XCTest
@testable import Unswiper

final class BundleIDMigrationTests: XCTestCase {
    private var suiteName = ""
    private var defaults: UserDefaults!

    override func setUp() {
        super.setUp()
        suiteName = "unswiper-tests-\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suiteName)
    }

    override func tearDown() {
        defaults.removePersistentDomain(forName: suiteName)
        defaults = nil
        super.tearDown()
    }

    func testIDs() {
        XCTAssertEqual(BundleIDMigration.legacyID, "com.rranslam.unswiper")
        XCTAssertEqual(BundleIDMigration.currentID, "io.github.rranslam.Unswiper")
    }

    func testCopiesLegacyKeysOnce() {
        let legacy: [String: Any] = ["NSWindow Frame Main": "10 10 900 640", "SomeFlag": true]
        XCTAssertEqual(BundleIDMigration.migrateDefaults(from: legacy, into: defaults), 2)
        XCTAssertEqual(defaults.string(forKey: "NSWindow Frame Main"), "10 10 900 640")
        XCTAssertTrue(defaults.bool(forKey: "SomeFlag"))
        XCTAssertTrue(defaults.bool(forKey: BundleIDMigration.doneKey))
        // Second run is a no-op even if the old domain changed.
        XCTAssertEqual(BundleIDMigration.migrateDefaults(from: ["Other": 1], into: defaults), 0)
        XCTAssertNil(defaults.object(forKey: "Other"))
    }

    func testDoesNotOverwriteNewerValues() {
        defaults.set("new", forKey: "Key")
        XCTAssertEqual(BundleIDMigration.migrateDefaults(from: ["Key": "old", "Extra": 3], into: defaults), 1)
        XCTAssertEqual(defaults.string(forKey: "Key"), "new")
        XCTAssertEqual(defaults.integer(forKey: "Extra"), 3)
    }

    func testNoLegacyDomainStillMarksDone() {
        XCTAssertEqual(BundleIDMigration.migrateDefaults(from: nil, into: defaults), 0)
        XCTAssertTrue(defaults.bool(forKey: BundleIDMigration.doneKey))
    }

    func testLaunchHookIsInertOutsideTheApp() {
        // Under `swift test` the bundle ID is not the app's, so this must not touch .standard.
        XCTAssertNotEqual(Bundle.main.bundleIdentifier, BundleIDMigration.currentID)
        BundleIDMigration.runAtLaunch()
    }
}
