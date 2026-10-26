// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import XCTest
@testable import Unswiper

final class LibraryStoreTests: StoreIsolatedTestCase {
    /// library.json as written by v0.1.0 (no modifiedAt, sidecars, isUndated, keepTargets, schemaVersion).
    static let v010JSON = """
    {
      "config" : { "commitMode" : "trash", "keepTargetDefault" : 10, "rootPath" : "/Users/x/Pictures", "visitGapHours" : 36 },
      "groups" : [ { "endDate" : "2026-10-01T12:00:00Z", "folderHint" : "Trip", "id" : "visit-abc", "itemPaths" : [ "/Users/x/Pictures/Trip/a.jpg" ],
                     "photoCount" : 1, "startDate" : "2026-10-01T12:00:00Z", "title" : "Oct 1 · Trip", "totalBytes" : 123, "videoCount" : 0 } ],
      "items" : [ { "decision" : "reject", "kind" : "photo", "path" : "/Users/x/Pictures/Trip/a.jpg", "relativeFolder" : "Trip",
                    "sizeBytes" : 123, "takenAt" : "2026-10-01T12:00:00Z" } ],
      "lastIndexedAt" : "2026-10-01T13:00:00Z"
    }
    """

    private func writeLibrary(_ text: String) throws {
        try text.data(using: .utf8)!.write(to: LibraryStore.snapshotURL)
    }

    private func backups() throws -> [URL] {
        try FileManager.default.contentsOfDirectory(at: supportDir, includingPropertiesForKeys: nil)
            .filter { $0.lastPathComponent.hasPrefix("library.json.corrupt-") }
    }

    func testMissingFileIsAFreshStart() {
        let result = LibraryStore.load()
        XCTAssertNil(result.problem)
        XCTAssertTrue(result.snapshot.items.isEmpty)
        XCTAssertEqual(result.snapshot.config, .default)
    }

    func testDecodesV010FormatWithDefaults() throws {
        try writeLibrary(Self.v010JSON)
        let result = LibraryStore.load()
        XCTAssertNil(result.problem)
        let snap = result.snapshot
        XCTAssertNil(snap.schemaVersion)
        XCTAssertEqual(snap.config.rootPath, "/Users/x/Pictures")
        XCTAssertNil(snap.config.keepTargets)
        XCTAssertNil(snap.config.bookmarkData)
        XCTAssertEqual(snap.items.count, 1)
        let item = snap.items[0]
        XCTAssertEqual(item.decision, .reject)
        XCTAssertNil(item.modifiedAt)
        XCTAssertFalse(item.isUndated)
        XCTAssertTrue(item.sidecars.isEmpty)
        XCTAssertNil(item.dateSource)
        XCTAssertNil(item.folderPath)
        XCTAssertEqual(snap.groups.first?.id, "visit-abc")
    }

    func testSaveWritesSchemaVersionAndRoundTrips() throws {
        var snap = LibraryStore.emptySnapshot
        snap.config.keepTargets = ["visit-1": 4]
        snap.items = [MediaItem(path: "/p/a.jpg", kind: .photo, sizeBytes: 1, takenAt: Date(timeIntervalSince1970: 1_700_000_000),
                                relativeFolder: "p", decision: .keep, modifiedAt: Date(timeIntervalSince1970: 1_700_000_001),
                                sidecars: [SidecarFile(path: "/p/a.mov", sizeBytes: 2, modifiedAt: nil)],
                                isUndated: false, dateSource: .folder, folderPath: "p")]
        snap.schemaVersion = nil
        try LibraryStore.save(snap)
        let text = try String(contentsOf: LibraryStore.snapshotURL, encoding: .utf8)
        XCTAssertTrue(text.contains("\"schemaVersion\" : 2"))
        let loaded = LibraryStore.load()
        XCTAssertNil(loaded.problem)
        XCTAssertEqual(loaded.snapshot.schemaVersion, 2)
        XCTAssertEqual(loaded.snapshot.items, snap.items)
        XCTAssertEqual(loaded.snapshot.config.keepTargets, ["visit-1": 4])
    }

    func testCorruptFileIsBackedUpNotSilentlyReset() throws {
        let garbage = "{ \"config\": { this is not json"
        try writeLibrary(garbage)
        let result = LibraryStore.load()
        XCTAssertNotNil(result.problem)
        XCTAssertTrue(result.snapshot.items.isEmpty)
        let backup = try XCTUnwrap(result.backupURL)
        XCTAssertEqual(try String(contentsOf: backup, encoding: .utf8), garbage)
        XCTAssertEqual(try backups().count, 1)
        // Original left in place until the next save; saving afterwards is safe because the backup exists.
        XCTAssertEqual(try String(contentsOf: LibraryStore.snapshotURL, encoding: .utf8), garbage)
        XCTAssertNoThrow(try LibraryStore.save(result.snapshot))
        XCTAssertEqual(try String(contentsOf: backup, encoding: .utf8), garbage)
    }

    func testWrongShapeIsBackedUp() throws {
        try writeLibrary("{\"items\": 5}")
        let result = LibraryStore.load()
        XCTAssertNotNil(result.problem)
        XCTAssertEqual(try backups().count, 1)
    }

    func testNewerSchemaIsBackedUpAndReported() throws {
        let newer = Self.v010JSON.replacingOccurrences(of: "\"lastIndexedAt\"", with: "\"schemaVersion\" : 99, \"lastIndexedAt\"")
        try writeLibrary(newer)
        let result = LibraryStore.load()
        XCTAssertTrue(result.problem?.contains("newer version") == true)
        XCTAssertEqual(try backups().count, 1)
    }

    func testRepeatedFailuresNeverOverwriteBackups() throws {
        try writeLibrary("bad 1")
        let date = Date(timeIntervalSince1970: 1_760_000_000)
        try LibraryStore.backupSnapshot(now: date)
        try writeLibrary("bad 2")
        try LibraryStore.backupSnapshot(now: date)
        let contents = try backups().map { try String(contentsOf: $0, encoding: .utf8) }
        XCTAssertEqual(Set(contents), ["bad 1", "bad 2"])
    }

    func testSaveRefusedWhenBlocked() throws {
        try writeLibrary("keep me")
        LibraryStore.saveBlockedReason = "test"
        XCTAssertThrowsError(try LibraryStore.save(LibraryStore.emptySnapshot))
        XCTAssertEqual(try String(contentsOf: LibraryStore.snapshotURL, encoding: .utf8), "keep me")
    }
}

final class PersistenceTests: StoreIsolatedTestCase {
    @MainActor private func savedSnapshot() -> LibrarySnapshot? {
        guard FileManager.default.fileExists(atPath: LibraryStore.snapshotURL.path) else { return nil }
        return LibraryStore.load().snapshot
    }

    @MainActor func testPersistIsDebouncedAndCoalesced() async throws {
        let state = AppState()
        state.saveDelay = .milliseconds(300)
        state.config.keepTargetDefault = 3
        state.persist()
        state.config.keepTargetDefault = 7
        state.persist()
        XCTAssertNil(savedSnapshot(), "nothing written before the debounce delay")
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertNil(savedSnapshot())
        try await Task.sleep(for: .milliseconds(900))
        XCTAssertEqual(savedSnapshot()?.config.keepTargetDefault, 7)
    }

    @MainActor func testFlushWritesImmediatelyAndCancelsPendingSave() async throws {
        let state = AppState()
        state.saveDelay = .milliseconds(200)
        state.config.keepTargetDefault = 4
        state.persist()
        state.flush()
        XCTAssertEqual(savedSnapshot()?.config.keepTargetDefault, 4)
        try FileManager.default.removeItem(at: LibraryStore.snapshotURL)
        try await Task.sleep(for: .milliseconds(500))
        XCTAssertNil(savedSnapshot(), "the cancelled debounced save must not fire")
    }

    @MainActor func testUpdateDecisionPersistsAndKeepsOrder() async throws {
        let state = AppState()
        state.saveDelay = .milliseconds(50)
        state.items = ["/c", "/a", "/b"].map {
            MediaItem(path: $0, kind: .photo, sizeBytes: 1, takenAt: Date(), relativeFolder: "", decision: .undecided)
        }
        state.updateDecision(paths: ["/a"], decision: .reject)
        XCTAssertEqual(state.items.map(\.path), ["/c", "/a", "/b"])
        try await Task.sleep(for: .milliseconds(400))
        XCTAssertEqual(savedSnapshot()?.items.first { $0.path == "/a" }?.decision, .reject)
    }

    @MainActor func testCorruptLibrarySurfacesAlertAtLaunch() throws {
        try "nope".data(using: .utf8)!.write(to: LibraryStore.snapshotURL)
        let state = AppState()
        XCTAssertNotNil(state.libraryLoadProblem)
        XCTAssertTrue(state.items.isEmpty)
    }

    @MainActor func testPendingManifestIsOfferedAtLaunch() throws {
        let m = CommitManifest(status: .pending, id: "x", createdAt: Date(), mode: .quarantine, libraryRoot: "/", entries: [])
        let url = LibraryStore.commitsDir.appendingPathComponent("x.json")
        try LibraryStore.writeManifest(m, to: [url])
        let state = AppState()
        XCTAssertEqual(state.interruptedCommitURL?.lastPathComponent, url.lastPathComponent)
        state.keepInterruptedCommit()
        XCTAssertNil(state.interruptedCommitURL)
        XCTAssertTrue(LibraryStore.pendingManifestURLs().isEmpty)
    }

    @MainActor func testPerVisitKeepTarget() {
        let state = AppState()
        state.setKeepTarget(0, for: "visit-1")
        XCTAssertEqual(state.keepTarget(for: "visit-1"), 1, "clamped to at least 1")
        XCTAssertEqual(state.keepTarget(for: "visit-2"), state.config.keepTargetDefault)
    }
}

final class HelpLibraryTests: XCTestCase {
    func testAllHelpTopicsLoad() {
        XCTAssertEqual(HelpLibrary.topics.count, 15)
        XCTAssertNotNil(HelpLibrary.topicID(containing: "google-photos"))
        XCTAssertNotNil(HelpLibrary.topicID(containing: "blurry"))
        XCTAssertNotNil(HelpLibrary.topicID(containing: "keyboard"))
    }

    func testGoogleDriveNoteInHelp() throws {
        let help = TakeoutHelperTests.repoRoot.appendingPathComponent("apps/macos/Sources/Unswiper/Resources/Help")
        let md = try String(contentsOf: help.appendingPathComponent("15-google-photos.md"), encoding: .utf8)
        for must in ["Google Drive for desktop", "~/Library/CloudStorage/GoogleDrive-", "_Rejected/Unswiper/",
                     "Items moved", "Click **OK**", "Don't click \"Move back\"", "Don't show this again",
                     "until you delete the `_Rejected` folder"] {
            XCTAssertTrue(md.contains(must), must)
        }
        let trouble = try String(contentsOf: help.appendingPathComponent("10-troubleshooting.md"), encoding: .utf8)
        XCTAssertTrue(trouble.contains("Items moved") && trouble.contains("Move back"))
    }

    func testGooglePhotosHelpExplainsTheSpaceCheck() throws {
        let md = try String(contentsOf: TakeoutHelperTests.repoRoot
            .appendingPathComponent("apps/macos/Sources/Unswiper/Resources/Help/15-google-photos.md"), encoding: .utf8)
        for must in ["can't be measured", "Pictures", "--no-space-check"] {
            XCTAssertTrue(md.contains(must), must)
        }
        XCTAssertFalse(md.contains("flatpak"), "Linux-only advice on the Mac")
    }

    // MARK: - Schema 2 (plan §20.5)

    /// crates/unswiper-core/tests/fixtures/compat, shared with the Rust tests.
    static func compatFixture(_ name: String) throws -> String {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("crates/unswiper-core/tests/fixtures/compat/\(name)")
        return try String(contentsOf: url, encoding: .utf8)
    }

    func testV1FileLoadsAndSavesAsV2WithoutGeoKeys() throws {
        let v1 = try Self.compatFixture("library-v1.json")
        try writeLibrary(v1)
        let loaded = LibraryStore.load()
        XCTAssertNil(loaded.problem)
        XCTAssertEqual(loaded.snapshot.schemaVersion, 1)
        XCTAssertEqual(loaded.snapshot.config.grouping, .visit)
        XCTAssertNil(loaded.snapshot.config.groupMode)
        try LibraryStore.save(loaded.snapshot)
        let saved = try String(contentsOf: LibraryStore.snapshotURL, encoding: .utf8)
        // Same as Rust: only the version number changes.
        XCTAssertEqual(saved, v1.replacingOccurrences(of: "\"schemaVersion\" : 1", with: "\"schemaVersion\" : 2"))
    }

    func testV2GoldenRoundTripsExactly() throws {
        let v2 = try Self.compatFixture("library-v2.json")
        try writeLibrary(v2)
        let loaded = LibraryStore.load()
        XCTAssertNil(loaded.problem)
        let snap = loaded.snapshot
        XCTAssertEqual(snap.schemaVersion, 2)
        XCTAssertEqual(snap.config.grouping, .visitGeo)
        XCTAssertEqual(snap.config.placeRadiusKm, 2.5)
        XCTAssertEqual(snap.config.minPlaceItems, 3)
        XCTAssertEqual(snap.config.joinSamePlaceDays, true)
        XCTAssertEqual(snap.items[0].location, GeoLocation(lat: 38.7223, lon: -9.1393, source: "exif"))
        XCTAssertNil(snap.items[1].location)
        XCTAssertEqual(snap.groups[0].place?.name, "Lisbon")
        XCTAssertEqual(snap.groups[0].parentVisitId, "visit-1a2b")
        XCTAssertNil(snap.groups[1].place)
        try LibraryStore.save(snap)
        let saved = try String(contentsOf: LibraryStore.snapshotURL, encoding: .utf8)
        XCTAssertEqual(saved, v2, "Swift writes the Rust golden file byte for byte")
    }

    func testUnknownGroupModeReadsAsVisitAndSurvives() throws {
        var config = AppConfig.default
        config.groupMode = "from-the-future"
        XCTAssertEqual(config.grouping, .visit)
        let data = try JSONEncoder().encode(config)
        XCTAssertEqual(try JSONDecoder().decode(AppConfig.self, from: data).groupMode, "from-the-future")
        config.grouping = .geo
        XCTAssertEqual(config.groupMode, "geo")
    }

}
