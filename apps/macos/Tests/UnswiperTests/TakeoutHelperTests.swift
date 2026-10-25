// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import XCTest
@testable import Unswiper

/// Google Takeout import (plan §19): the helper's JSON (sample output written by the Rust tests in
/// crates/unswiper-takeout/tests/fixtures/helper), arguments, words, and the real helper end to end.
final class TakeoutHelperTests: XCTestCase {
    static var repoRoot: URL {
        var url = URL(fileURLWithPath: #filePath)
        for _ in 0..<5 { url.deleteLastPathComponent() }
        return url
    }

    static var fixtures: URL {
        repoRoot.appendingPathComponent("crates/unswiper-takeout/tests/fixtures/helper", isDirectory: true)
    }

    private func fixture(_ name: String) throws -> Data {
        try Data(contentsOf: Self.fixtures.appendingPathComponent(name))
    }

    func testPlanEvent() throws {
        guard case .plan(let p, let fits, let needed, let lines) = try TakeoutHelper.parseEvent(fixture("plan.json")) else {
            return XCTFail("not a plan")
        }
        XCTAssertTrue(fits)
        XCTAssertGreaterThan(needed, p.neededKeep)
        XCTAssertEqual(p.items, 3)
        XCTAssertEqual(p.newItems, 2)
        XCTAssertEqual(p.archives.count, 2)
        XCTAssertTrue(p.archives[0].exact)
        XCTAssertFalse(p.archives[0].doneEarlier)
        XCTAssertNotNil(p.freeBytes)
        XCTAssertEqual(p.onPortal, false)
        XCTAssertNil(p.hostPath)
        XCTAssertTrue(lines.first?.hasPrefix("2 archives") ?? false, "\(lines)")
        XCTAssertTrue(lines.contains { $0.hasPrefix("Space needed:") })
    }

    func testUnknownFreeSpaceDoesNotFit() throws {
        // What the helper prints when it can't measure the library disk (free_bytes null).
        let json = #"""
        {"event":"plan","fits":false,"needed":1073741957,"lines":["Free space on the library disk: can't be measured, so Unswiper won't start. A full Google Photos export can be huge, and running out of space halfway is worse than not starting.","How to fix it: click Choose… next to Library folder and pick a folder on a disk of this computer, for example inside your Pictures folder. Then click Check Again."],"plan":{"archives":[],"archive_bytes":0,"items":0,"photos_bytes":0,"new_items":0,"new_bytes":0,"free_bytes":null,"on_portal":false,"host_path":null,"needed_keep":0,"needed_delete":0,"same_disk":false,"seconds_min":0,"seconds_max":0}}
        """#
        guard case .plan(let p, let fits, _, let lines) = try TakeoutHelper.parseEvent(Data(json.utf8)) else {
            return XCTFail("not a plan")
        }
        XCTAssertFalse(fits)
        XCTAssertNil(p.freeBytes)
        XCTAssertTrue(TakeoutHelper.isSpaceWarning(lines[0]))
        XCTAssertFalse(TakeoutHelper.isSpaceWarning("Time: under a minute."))
        XCTAssertTrue(TakeoutHelper.isSpaceWarning("Free on the library disk: 1 GB. NOT enough: 2 GB more is needed."))
    }

    func testProgressDoneAndErrorEvents() throws {
        guard case .progress(let pr) = try TakeoutHelper.parseEvent(fixture("progress.json")) else {
            return XCTFail("not progress")
        }
        XCTAssertEqual(pr.archives, 2)
        XCTAssertEqual(pr.bytesTotal, 133)
        guard case .done(let r, let lines) = try TakeoutHelper.parseEvent(fixture("done.json")) else {
            return XCTFail("not done")
        }
        XCTAssertEqual(r.extracted, 3)
        XCTAssertEqual(r.duplicatesSkipped, 1)
        XCTAssertEqual(r.archivesDeleted.count, 2)
        XCTAssertEqual(lines.first, "Unpacked 3 files (133 bytes).")
        guard case .error(let kind, let message) = try TakeoutHelper.parseEvent(fixture("error.json")) else {
            return XCTFail("not an error")
        }
        XCTAssertEqual(kind, "no_archives")
        XCTAssertTrue(message.contains("no .zip or .tgz"))
        XCTAssertThrowsError(try TakeoutHelper.parseEvent(Data(#"{"event":"bogus"}"#.utf8)))
    }

    func testDates() throws {
        let entry = try fixture("dates-entry.json")
        let array = Data("[".utf8) + entry + Data("]".utf8)
        let d = try TakeoutHelper.parseDates(array)
        XCTAssertEqual(d.count, 1)
        XCTAssertEqual(d.values.first, Date(timeIntervalSince1970: 1_561_190_459))
        XCTAssertEqual(try TakeoutHelper.parseDates(Data(#"[{"path":"/x.jpg","taken":null,"lat":1.0,"lon":2.0}]"#.utf8)).count, 0)
    }

    func testArguments() {
        let a = URL(fileURLWithPath: "/Users/me/Downloads"), l = URL(fileURLWithPath: "/Users/me/Pictures/GooglePhotosLibrary")
        XCTAssertEqual(TakeoutHelper.importArguments(archives: a, library: l, options: .init(), dryRun: true),
                       ["takeout", "import", a.path, "--to", l.path, "--dry-run", "--json"])
        XCTAssertEqual(TakeoutHelper.importArguments(archives: a, library: l,
                                                     options: .init(deleteArchives: true, keepDuplicates: true), dryRun: false),
                       ["takeout", "import", a.path, "--to", l.path, "--delete-archives", "--keep-duplicates", "--progress-json"])
        XCTAssertEqual(TakeoutHelper.datesArguments(library: l), ["takeout", "dates", l.path])
    }

    func testTakeLines() {
        var b = Data("{\"a\":1}\n\n{\"b\"".utf8)
        XCTAssertEqual(TakeoutHelper.takeLines(&b), [Data("{\"a\":1}".utf8)])
        XCTAssertEqual(b, Data("{\"b\"".utf8))
        b.append(Data(":2}\n".utf8))
        XCTAssertEqual(TakeoutHelper.takeLines(&b), [Data("{\"b\":2}".utf8)])
        XCTAssertTrue(b.isEmpty)
    }

    /// Same words as the Linux app (crates/unswiper-gtk/src/takeout.rs tests).
    func testWords() {
        XCTAssertEqual(TakeoutHelper.human(999), "999 bytes")
        XCTAssertEqual(TakeoutHelper.human(1_500_000_000), "1.5 GB")
        XCTAssertEqual(TakeoutHelper.duration(30), "under a minute")
        XCTAssertEqual(TakeoutHelper.duration(61), "about 2 min")
        XCTAssertEqual(TakeoutHelper.duration(7_800), "about 2 h 10 min")
        let p = TakeoutHelper.Progress(archive: 2, archives: 3, files: 120, bytes: 1_000_000_000, bytesTotal: 4_000_000_000)
        let (f, t) = TakeoutHelper.progressText(p, elapsed: 10)
        XCTAssertEqual(f, 0.25)
        XCTAssertEqual(t, "Part 2 of 3 · 120 files · 1.0 GB of 4.0 GB · 100.0 MB/s · under a minute left")
        let (f0, t0) = TakeoutHelper.progressText(.init(archive: 0, archives: 0, files: 0, bytes: 0, bytesTotal: 0), elapsed: 0)
        XCTAssertEqual(f0, 0)
        XCTAssertEqual(t0, "Part 1 of 0 · 0 files · 0 bytes of 0 bytes")
    }

    func testFolderProblem() {
        func u(_ p: String) -> URL { URL(fileURLWithPath: p, isDirectory: true) }
        XCTAssertEqual(TakeoutHelper.folderProblem(archives: nil, library: nil), "Choose the archives folder.")
        XCTAssertEqual(TakeoutHelper.folderProblem(archives: u("/d"), library: nil), "Choose the library folder.")
        XCTAssertTrue(TakeoutHelper.folderProblem(archives: u("/d"), library: u("/d"))?.contains("two different") ?? false)
        XCTAssertNil(TakeoutHelper.folderProblem(archives: u("/Users/me/Downloads"), library: u("/Users/me/Downloads/GooglePhotosLibrary")))
        XCTAssertTrue(TakeoutHelper.folderProblem(archives: u("/Users/me/Downloads"), library: u("/Users/me"))?.contains("inside the library") ?? false)
        XCTAssertNil(TakeoutHelper.folderProblem(archives: u("/Users/me/Downloads"), library: u("/Volumes/Photos/GooglePhotosLibrary")))
    }

    func testGuideIsTheSharedOne() throws {
        let rust = try Data(contentsOf: Self.repoRoot.appendingPathComponent("crates/unswiper-takeout/src/takeout-guide.json"))
        let mac = try Data(contentsOf: Self.repoRoot.appendingPathComponent("apps/macos/Sources/Unswiper/Resources/Takeout/takeout-guide.json"))
        XCTAssertEqual(rust, mac, "copy crates/unswiper-takeout/src/takeout-guide.json to the Mac resources")
        let g = try TakeoutGuide.decode(mac)
        XCTAssertEqual(g.steps.map(\.id), ["open-takeout", "choose-photos", "choose-options", "wait-email",
                                           "download", "choose-folders", "unpack", "index"])
        XCTAssertEqual(g.takeoutUrl, "https://takeout.google.com/settings/takeout/custom/photos")
        let all = g.steps.map(\.body).joined()
        for must in ["Deselect all", "Export every 2 months for 1 year", "Export once", "by hand", ".zip", "50 GB", "Create export"] {
            XCTAssertTrue(all.contains(must), must)
        }
    }

    func testIsTakeoutLibrary() throws {
        let d = FileManager.default.temporaryDirectory.appendingPathComponent("takeout-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: d) }
        XCTAssertFalse(TakeoutHelper.isTakeoutLibrary(d))
        try Data("{}".utf8).write(to: d.appendingPathComponent(TakeoutHelper.stateFile))
        XCTAssertTrue(TakeoutHelper.isTakeoutLibrary(d))
    }

    /// End to end with the real Rust helper (UNSWIPER_BLUR_HELPER, as in CI): dry run, streamed
    /// import of the synthetic fixture export, then Google's dates.
    func testRealHelperImport() throws {
        guard let helper = BlurHelper.locate(executable: nil) else {
            throw XCTSkip("UNSWIPER_BLUR_HELPER not set")
        }
        let d = FileManager.default.temporaryDirectory.appendingPathComponent("takeout-\(UUID().uuidString)", isDirectory: true)
        let downloads = d.appendingPathComponent("Downloads", isDirectory: true)
        let library = d.appendingPathComponent("GooglePhotosLibrary", isDirectory: true)
        try FileManager.default.createDirectory(at: downloads, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: d) }
        let zip = "takeout-20261015T000000Z-001.zip"
        try FileManager.default.copyItem(at: Self.fixtures.appendingPathComponent(zip), to: downloads.appendingPathComponent(zip))

        guard case .plan(let p, let fits, _, _) = try TakeoutHelper.dryRun(helper: helper, archives: downloads, library: library, options: .init()) else {
            return XCTFail("no plan")
        }
        XCTAssertTrue(fits)
        XCTAssertEqual(p.newItems, 1)
        XCTAssertFalse(FileManager.default.fileExists(atPath: library.path), "a dry run writes nothing")

        var events: [TakeoutHelper.Event] = []
        let exited = expectation(description: "helper exits")
        var code: Int32 = -1
        let args = TakeoutHelper.importArguments(archives: downloads, library: library,
                                                 options: .init(deleteArchives: true), dryRun: false)
        let run = try TakeoutRun(helper: helper, arguments: args, onEvent: { events.append($0) },
                                 onExit: { code = $0; exited.fulfill() })
        wait(for: [exited], timeout: 30)
        _ = run
        XCTAssertEqual(code, 0)
        guard case .plan = events.first else { return XCTFail("first event: \(String(describing: events.first))") }
        guard case .done(let r, _) = events.last else { return XCTFail("last event: \(String(describing: events.last))") }
        XCTAssertEqual(r.extracted, 2)
        XCTAssertEqual(r.duplicatesSkipped, 1)
        XCTAssertEqual(r.archivesDeleted.count, 1)
        XCTAssertFalse(FileManager.default.fileExists(atPath: downloads.appendingPathComponent(zip).path))
        XCTAssertTrue(TakeoutHelper.isTakeoutLibrary(library))

        let dates = try TakeoutHelper.dates(helper: helper, library: library)
        let img = library.appendingPathComponent("Photos from 2019/IMG_1.jpg").resolvingSymlinksInPath().path
        XCTAssertEqual(dates[img], Date(timeIntervalSince1970: 1_561_190_459))
    }
}
