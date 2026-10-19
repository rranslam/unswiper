// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import XCTest
@testable import Unswiper

/// The "Import from Google Photos" assistant model and the indexer's use of Google's dates.
@MainActor
final class TakeoutSetupTests: XCTestCase {
    private func defaults() -> UserDefaults {
        let name = "unswiper-takeout-tests-\(UUID().uuidString)"
        let d = UserDefaults(suiteName: name)!
        d.removePersistentDomain(forName: name)
        return d
    }

    /// The guide from the repository (independent of the resource bundle).
    private func guide() throws -> TakeoutGuide {
        try TakeoutGuide.decode(Data(contentsOf: TakeoutHelperTests.repoRoot
            .appendingPathComponent("apps/macos/Sources/Unswiper/Resources/Takeout/takeout-guide.json")))
    }

    func testGuideIsBundled() {
        let g = TakeoutGuide.shared
        XCTAssertEqual(g.steps.count, 8, "Resources/Takeout/takeout-guide.json not in the bundle")
        guard g.steps.count == 8 else { return }
        XCTAssertEqual(g.steps[5].action, "choose_folders")
        XCTAssertEqual(g.takeoutUrl, "https://takeout.google.com/settings/takeout/custom/photos")
    }

    func testRemembersStepFoldersAndOptions() throws {
        let d = defaults()
        let m = TakeoutSetupModel(guide: try guide(), defaults: d, helper: { nil })
        XCTAssertEqual(m.saved.step, 0)
        XCTAssertEqual(m.stepLabel, "Step 1 of 8")
        m.go(1)
        m.go(1)
        m.saved.archivesPath = "/Users/me/Downloads"
        m.saved.libraryPath = "/Users/me/Pictures/GooglePhotosLibrary"
        m.setOptions(.init(deleteArchives: true, keepDuplicates: false))
        let again = TakeoutSetupModel(guide: try guide(), defaults: d, helper: { nil })
        XCTAssertEqual(again.saved.step, 2)
        XCTAssertEqual(again.saved.archivesPath, "/Users/me/Downloads")
        XCTAssertTrue(again.saved.options.deleteArchives)
        XCTAssertFalse(again.saved.options.keepDuplicates)
        again.go(-5)
        XCTAssertEqual(again.saved.step, 0, "clamped")
        again.go(50)
        XCTAssertEqual(again.saved.step, 7, "clamped")
    }

    func testNextNeedsAFittingDryRunAndAFinishedUnpack() throws {
        let m = TakeoutSetupModel(guide: try guide(), defaults: defaults(), helper: { nil })
        m.saved.step = 5
        XCTAssertFalse(m.canGoNext, "no folders yet")
        XCTAssertEqual(m.figures, ["Choose the archives folder."])
        m.saved.archivesPath = "/a"
        m.saved.libraryPath = "/b"
        m.check()
        XCTAssertFalse(m.canGoNext)
        XCTAssertTrue(m.figures.first?.contains("helper is missing") ?? false, "\(m.figures)")
        m.saved.step = 6
        XCTAssertFalse(m.canGoNext, "not unpacked yet")
        m.handle(.done(.init(extracted: 1, alreadyThere: 0, archivesSkipped: 0, bytesWritten: 3, refused: [],
                             conflicts: [], otherProducts: [], previouslyImported: 0, duplicatesSkipped: 0,
                             archivesDeleted: []), lines: ["Unpacked 1 file (3 bytes)."]))
        XCTAssertTrue(m.canGoNext)
        XCTAssertEqual(m.reportLines, ["Unpacked 1 file (3 bytes)."])
        m.handle(.error(kind: "cancelled", message: "stopped"))
        XCTAssertTrue(m.problem?.contains("Continue Unpacking") ?? false)
        m.saved.step = 7
        XCTAssertFalse(m.canGoNext, "last step has no Next")
        XCTAssertEqual(m.finish()?.path, "/b")
        XCTAssertEqual(m.saved.step, 5, "reopens on Choose the folders for the next export")
    }

    func testUnpackNeedsKnownAndEnoughFreeSpace() throws {
        let m = TakeoutSetupModel(guide: try guide(), defaults: defaults(), helper: { nil })
        m.saved.archivesPath = "/a"
        m.saved.libraryPath = "/b"
        m.saved.step = 5
        func plan(free: UInt64?, fits: Bool) -> TakeoutHelper.Event {
            .plan(.init(archives: [], archiveBytes: 10, items: 1, photosBytes: 10, newItems: 1, newBytes: 10,
                        freeBytes: free, neededKeep: 10, neededDelete: 10, sameDisk: true,
                        secondsMin: 1, secondsMax: 1),
                  fits: fits, needed: 10, lines: ["Free space on the library disk: can't be measured, so Unswiper won't start."])
        }
        m.handle(plan(free: nil, fits: false))
        XCTAssertFalse(m.canGoNext, "unknown free space blocks Next")
        XCTAssertFalse(m.canUnpack, "unknown free space blocks Unpack")
        m.handle(plan(free: 1 << 40, fits: true))
        XCTAssertTrue(m.canGoNext)
        XCTAssertTrue(m.canUnpack)
        // The helper refuses too; its message is shown.
        m.handle(.error(kind: "space_unknown", message: "can't measure the free space in /b"))
        XCTAssertEqual(m.problem, "can't measure the free space in /b")
    }

    func testProgressUpdatesTheBar() throws {
        let m = TakeoutSetupModel(guide: try guide(), defaults: defaults(), helper: { nil })
        m.handle(.progress(.init(archive: 1, archives: 2, files: 5, bytes: 50, bytesTotal: 100)))
        XCTAssertEqual(m.fraction, 0.5)
        XCTAssertTrue(m.progressText.hasPrefix("Part 1 of 2 · 5 files"), m.progressText)
    }

    func testIndexerUsesGoogleDatesOnlyWithoutOwnDate() throws {
        let root = try Fixture.tempDir("takeout-index")
        defer { try? FileManager.default.removeItem(at: root) }
        let year = root.appendingPathComponent("Photos from 2019", isDirectory: true)
        let undated = try Fixture.jpeg(year.appendingPathComponent("Screenshot.jpg"))
        let dated = try Fixture.jpeg(year.appendingPathComponent("IMG_1.jpg"), exifDate: "2024:05:01 10:20:30")
        let google = Date(timeIntervalSince1970: 1_561_190_459)
        let fallback = [
            undated.resolvingSymlinksInPath().path: google,
            dated.resolvingSymlinksInPath().path: google,
        ]
        let r = try LocalTreeIndexer.index(root: root, visitGapHours: 36, existingDecisions: [:],
                                           fallbackDates: fallback, onProgress: { _ in })
        let byName = Dictionary(uniqueKeysWithValues: r.items.map { ($0.fileName, $0) })
        XCTAssertEqual(byName["Screenshot.jpg"]?.takenAt, google)
        XCTAssertEqual(byName["Screenshot.jpg"]?.dateSource, .capture)
        XCTAssertEqual(byName["IMG_1.jpg"]?.takenAt, Fixture.localDate(2024, 5, 1, 10, 20, 30), "the file's own date wins")
        // Without Google's dates the screenshot is undated ("Photos from 2019" isn't a folder date).
        let plain = try LocalTreeIndexer.index(root: root, visitGapHours: 36, existingDecisions: [:], onProgress: { _ in })
        XCTAssertEqual(plain.items.first { $0.fileName == "Screenshot.jpg" }?.dateSource, .unknown)
    }
}
