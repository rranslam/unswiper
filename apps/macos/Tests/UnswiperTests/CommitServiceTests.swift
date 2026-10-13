// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import XCTest
@testable import Unswiper

final class CommitValidationTests: StoreIsolatedTestCase {
    func testValidFilePasses() throws {
        let root = try makeRoot()
        let url = try Fixture.file(root.appendingPathComponent("a.jpg"), bytes: 10)
        XCTAssertNil(CommitService.validate(path: url.path, size: 10, modifiedAt: try Fixture.mtime(url), root: root))
    }

    func testNoRoot() throws {
        let root = try makeRoot()
        let url = try Fixture.file(root.appendingPathComponent("a.jpg"))
        XCTAssertEqual(CommitService.validate(path: url.path, size: 16, modifiedAt: try Fixture.mtime(url), root: nil),
                       "No library folder; refusing to move.")
    }

    func testOutsideRoot() throws {
        let root = try makeRoot()
        let other = try makeRoot()
        let url = try Fixture.file(other.appendingPathComponent("a.jpg"))
        XCTAssertEqual(CommitService.validate(path: url.path, size: 16, modifiedAt: try Fixture.mtime(url), root: root),
                       "Outside the library folder; not moved.")
    }

    func testDotDotEscapeIsOutsideRoot() throws {
        let root = try makeRoot()
        let other = try makeRoot()
        let url = try Fixture.file(other.appendingPathComponent("a.jpg"))
        let sneaky = root.path + "/../" + other.lastPathComponent + "/a.jpg"
        XCTAssertEqual(CommitService.validate(path: sneaky, size: 16, modifiedAt: try Fixture.mtime(url), root: root),
                       "Outside the library folder; not moved.")
    }

    func testSiblingPrefixFolderIsOutside() throws {
        // "/x/lib2/a.jpg" must not count as inside "/x/lib".
        let base = try makeRoot()
        let root = base.appendingPathComponent("lib")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let url = try Fixture.file(base.appendingPathComponent("lib2/a.jpg"))
        XCTAssertEqual(CommitService.validate(path: url.path, size: 16, modifiedAt: try Fixture.mtime(url), root: root),
                       "Outside the library folder; not moved.")
    }

    func testSymlinkIsRefused() throws {
        let root = try makeRoot()
        let other = try makeRoot()
        let target = try Fixture.file(other.appendingPathComponent("real.jpg"))
        let link = root.appendingPathComponent("link.jpg")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: target)
        XCTAssertEqual(CommitService.validate(path: link.path, size: 16, modifiedAt: try Fixture.mtime(target), root: root),
                       "Is a symbolic link; not moved.")
        let inside = try Fixture.file(root.appendingPathComponent("inside.jpg"))
        let link2 = root.appendingPathComponent("link2.jpg")
        try FileManager.default.createSymbolicLink(at: link2, withDestinationURL: inside)
        XCTAssertEqual(CommitService.validate(path: link2.path, size: 16, modifiedAt: try Fixture.mtime(inside), root: root),
                       "Is a symbolic link; not moved.")
    }

    func testMissingFile() throws {
        let root = try makeRoot()
        XCTAssertEqual(CommitService.validate(path: root.appendingPathComponent("gone.jpg").path, size: 1, modifiedAt: Date(), root: root),
                       "File is missing or not a regular file; re-index.")
    }

    func testDirectoryIsNotARegularFile() throws {
        let root = try makeRoot()
        let dir = root.appendingPathComponent("folder.jpg")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        XCTAssertEqual(CommitService.validate(path: dir.path, size: 0, modifiedAt: Date(), root: root),
                       "File is missing or not a regular file; re-index.")
    }

    func testSizeMismatch() throws {
        let root = try makeRoot()
        let url = try Fixture.file(root.appendingPathComponent("a.jpg"), bytes: 10)
        XCTAssertEqual(CommitService.validate(path: url.path, size: 11, modifiedAt: try Fixture.mtime(url), root: root),
                       "File size changed since indexing; re-index.")
    }

    func testMissingModifiedAt() throws {
        let root = try makeRoot()
        let url = try Fixture.file(root.appendingPathComponent("a.jpg"))
        XCTAssertEqual(CommitService.validate(path: url.path, size: 16, modifiedAt: nil, root: root),
                       "No modification date recorded; re-index before committing.")
    }

    func testMtimeToleranceOneSecond() throws {
        let root = try makeRoot()
        let mtime = Date(timeIntervalSince1970: 1_700_000_000.4)
        let url = try Fixture.file(root.appendingPathComponent("a.jpg"), modifiedAt: mtime)
        let actual = try Fixture.mtime(url)
        // library.json drops fractional seconds, so whole-second values must pass.
        XCTAssertNil(CommitService.validate(path: url.path, size: 16, modifiedAt: Date(timeIntervalSince1970: 1_700_000_000), root: root))
        XCTAssertNil(CommitService.validate(path: url.path, size: 16, modifiedAt: actual.addingTimeInterval(0.9), root: root))
        XCTAssertNil(CommitService.validate(path: url.path, size: 16, modifiedAt: actual.addingTimeInterval(-0.9), root: root))
        XCTAssertEqual(CommitService.validate(path: url.path, size: 16, modifiedAt: actual.addingTimeInterval(1.5), root: root),
                       "File modified since indexing; re-index.")
        XCTAssertEqual(CommitService.validate(path: url.path, size: 16, modifiedAt: actual.addingTimeInterval(-60), root: root),
                       "File modified since indexing; re-index.")
    }
}

final class CommitRestoreTests: StoreIsolatedTestCase {
    private let now = Date(timeIntervalSince1970: 1_760_000_000)

    func testDryRunMovesNothing() throws {
        let root = try makeRoot()
        let a = try Fixture.file(root.appendingPathComponent("a.jpg"), bytes: 10)
        let b = try Fixture.file(root.appendingPathComponent("b.jpg"), bytes: 20)
        let items = [try Fixture.item(at: a), try Fixture.item(at: b, decision: .keep)]
        let result = try CommitService.commitRejects(items: items, mode: .dryRun, libraryRoot: root)
        XCTAssertTrue(result.dryRun)
        XCTAssertEqual(result.moved, 0)
        XCTAssertEqual(result.wouldMove, 1)
        XCTAssertEqual(result.bytes, 10)
        XCTAssertNil(result.manifestURL)
        XCTAssertTrue(FileManager.default.fileExists(atPath: a.path))
        XCTAssertTrue(LibraryStore.manifestURLs().isEmpty)
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("_Rejected").path))
    }

    func testNoRejectsIsANoOp() throws {
        let root = try makeRoot()
        let a = try Fixture.file(root.appendingPathComponent("a.jpg"))
        let result = try CommitService.commitRejects(items: [try Fixture.item(at: a, decision: .keep)], mode: .quarantine, libraryRoot: root)
        XCTAssertEqual(result.moved, 0)
        XCTAssertTrue(LibraryStore.manifestURLs().isEmpty)
    }

    func testQuarantineCommitAndRestoreRoundTrip() throws {
        let root = try makeRoot()
        let still = try Fixture.file(root.appendingPathComponent("2026/IMG_1.HEIC"), bytes: 30)
        let mov = try Fixture.file(root.appendingPathComponent("2026/IMG_1.MOV"), bytes: 40)
        let keep = try Fixture.file(root.appendingPathComponent("2026/IMG_2.HEIC"), bytes: 5)
        let items = [try Fixture.item(at: still, sidecars: [mov]), try Fixture.item(at: keep, decision: .keep)]

        let result = try CommitService.commitRejects(items: items, mode: .quarantine, libraryRoot: root, now: now)
        XCTAssertEqual(result.moved, 1)
        XCTAssertEqual(result.bytes, 70)
        XCTAssertTrue(result.failed.isEmpty)
        XCTAssertFalse(FileManager.default.fileExists(atPath: still.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: mov.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: keep.path))

        // Manifest contents
        let manifestURL = try XCTUnwrap(result.manifestURL)
        XCTAssertEqual(manifestURL.deletingLastPathComponent().lastPathComponent, "commits")
        let manifest = try LibraryStore.readManifest(at: manifestURL)
        XCTAssertEqual(manifest.status, .complete)
        XCTAssertEqual(manifest.mode, .quarantine)
        XCTAssertEqual(manifest.libraryRoot, root.path)
        XCTAssertEqual(manifest.createdAt, now)
        XCTAssertEqual(manifest.entries.map(\.originalPath), [still.path, mov.path])
        XCTAssertEqual(manifest.entries.map(\.sizeBytes), [30, 40])
        for entry in manifest.entries {
            XCTAssertEqual(entry.state, .moved)
            XCTAssertNotNil(entry.movedAt)
            XCTAssertTrue(entry.resultPath.hasPrefix(root.path + "/_Rejected/Unswiper/"))
            XCTAssertTrue(FileManager.default.fileExists(atPath: entry.resultPath))
        }
        let copy = URL(fileURLWithPath: manifest.entries[0].resultPath).deletingLastPathComponent().appendingPathComponent("manifest.json")
        XCTAssertEqual(try LibraryStore.readManifest(at: copy).entries.count, 2)
        XCTAssertTrue(LibraryStore.pendingManifestURLs().isEmpty)

        // Restore
        let restored = CommitService.restore(manifest: manifest)
        XCTAssertEqual(restored.restored, 2)
        XCTAssertTrue(restored.failed.isEmpty)
        XCTAssertEqual(try Data(contentsOf: still).count, 30)
        XCTAssertEqual(try Data(contentsOf: mov).count, 40)
    }

    func testQuarantineNameCollisionsGetUniqueNames() throws {
        let root = try makeRoot()
        let a = try Fixture.file(root.appendingPathComponent("x/IMG.jpg"))
        let b = try Fixture.file(root.appendingPathComponent("y/img.JPG"))
        let result = try CommitService.commitRejects(items: [try Fixture.item(at: a), try Fixture.item(at: b)], mode: .quarantine, libraryRoot: root, now: now)
        XCTAssertEqual(result.moved, 2)
        let manifest = try LibraryStore.readManifest(at: try XCTUnwrap(result.manifestURL))
        XCTAssertEqual(Set(manifest.entries.map(\.resultPath)).count, 2)
        XCTAssertEqual(CommitService.restore(manifest: manifest).restored, 2)
    }

    func testInvalidSetIsSkippedWhole() throws {
        let root = try makeRoot()
        let still = try Fixture.file(root.appendingPathComponent("IMG_1.HEIC"))
        let mov = try Fixture.file(root.appendingPathComponent("IMG_1.MOV"))
        var item = try Fixture.item(at: still, sidecars: [mov])
        item.sidecars[0].sizeBytes += 1   // sidecar changed since indexing
        let result = try CommitService.commitRejects(items: [item], mode: .quarantine, libraryRoot: root, now: now)
        XCTAssertEqual(result.moved, 0)
        XCTAssertEqual(result.failed.count, 1)
        XCTAssertTrue(FileManager.default.fileExists(atPath: still.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: mov.path))
        XCTAssertTrue(LibraryStore.manifestURLs().isEmpty, "nothing moved -> no manifest")
    }

    func testInterruptedCommitLeavesPendingManifestAndRestores() throws {
        let root = try makeRoot()
        let still = try Fixture.file(root.appendingPathComponent("IMG_1.HEIC"))
        let mov = try Fixture.file(root.appendingPathComponent("IMG_1.MOV"))
        let other = try Fixture.file(root.appendingPathComponent("IMG_9.jpg"))
        let items = [try Fixture.item(at: still, sidecars: [mov]), try Fixture.item(at: other)]

        let result = try CommitService.commitRejects(items: items, mode: .quarantine, libraryRoot: root, now: now, stopAfterMoves: 1)
        let url = try XCTUnwrap(result.manifestURL)
        XCTAssertEqual(LibraryStore.pendingManifestURLs().map(\.lastPathComponent), [url.lastPathComponent])

        let manifest = try LibraryStore.readManifest(at: url)
        XCTAssertEqual(manifest.status, .pending)
        XCTAssertEqual(manifest.entries.map(\.state), [.moved, .pending, .pending])
        XCTAssertFalse(manifest.entries[1].resultPath.isEmpty, "quarantine destination is planned before moving")
        XCTAssertFalse(FileManager.default.fileExists(atPath: still.path))

        let restored = CommitService.restore(manifest: manifest)
        XCTAssertEqual(restored.restored, 1)
        XCTAssertEqual(restored.alreadyInPlace, 2)
        XCTAssertTrue(restored.failed.isEmpty)
        for url in [still, mov, other] { XCTAssertTrue(FileManager.default.fileExists(atPath: url.path)) }
    }

    func testPendingEntryMovedButNotRecordedIsFoundAtPlannedPath() throws {
        // Crash between the move and the manifest update: the planned path is still known.
        let root = try makeRoot()
        let a = try Fixture.file(root.appendingPathComponent("a.jpg"))
        let planned = root.appendingPathComponent("_Rejected/Unswiper/t/a.jpg")
        try FileManager.default.createDirectory(at: planned.deletingLastPathComponent(), withIntermediateDirectories: true)
        try FileManager.default.moveItem(at: a, to: planned)
        let manifest = CommitManifest(status: .pending, id: "t", createdAt: now, mode: .quarantine, libraryRoot: root.path,
                                      entries: [.init(originalPath: a.path, resultPath: planned.path, sizeBytes: 16, movedAt: nil, state: .pending)])
        let restored = CommitService.restore(manifest: manifest)
        XCTAssertEqual(restored.restored, 1)
        XCTAssertTrue(FileManager.default.fileExists(atPath: a.path))
    }

    func testRestoreReportsTrashItemThatCannotBeLocated() throws {
        let root = try makeRoot()
        let gone = root.appendingPathComponent("a.jpg")
        let manifest = CommitManifest(status: .complete, id: "t", createdAt: now, mode: .trash, libraryRoot: root.path, entries: [
            .init(originalPath: gone.path, resultPath: "", sizeBytes: 1, movedAt: now, state: .moved),
            .init(originalPath: root.appendingPathComponent("b.jpg").path, resultPath: "/nonexistent/.Trash/b.jpg", sizeBytes: 1, movedAt: now, state: .moved),
        ])
        let result = CommitService.restore(manifest: manifest)
        XCTAssertEqual(result.restored, 0)
        XCTAssertEqual(result.failed.count, 2)
        XCTAssertTrue(result.failed.allSatisfy { $0.error.contains("Couldn't locate this file in the Trash") })
    }

    func testRestoreDoesNotOverwriteExistingFile() throws {
        let root = try makeRoot()
        let a = try Fixture.file(root.appendingPathComponent("a.jpg"))
        let result = try CommitService.commitRejects(items: [try Fixture.item(at: a)], mode: .quarantine, libraryRoot: root, now: now)
        try Fixture.file(a, bytes: 3)   // a new file appeared at the original path
        let restored = CommitService.restore(manifest: try LibraryStore.readManifest(at: try XCTUnwrap(result.manifestURL)))
        XCTAssertEqual(restored.restored, 0)
        XCTAssertEqual(restored.failed.first?.error, "A file already exists at the original path.")
        XCTAssertEqual(try Data(contentsOf: a).count, 3)
    }

    func testLegacyManifestWithoutStatusDecodesAsComplete() throws {
        let json = """
        {"createdAt":"2026-10-11T10:00:00Z","entries":[{"movedAt":"2026-10-11T10:00:01Z","originalPath":"/a.jpg","resultPath":"/q/a.jpg","sizeBytes":3}],"id":"x","libraryRoot":"/","mode":"quarantine"}
        """
        let url = supportDir.appendingPathComponent("old.json")
        try json.data(using: .utf8)!.write(to: url)
        let m = try LibraryStore.readManifest(at: url)
        XCTAssertFalse(m.isPending)
        XCTAssertEqual(m.entries[0].effectiveState, .moved)
    }

    func testTrashCommitAndRestoreRoundTrip() throws {
        let root = try makeRoot()
        let a = try Fixture.file(root.appendingPathComponent("unswiper-test-\(UUID().uuidString).jpg"), bytes: 12)
        let result: CommitService.Result
        do {
            result = try CommitService.commitRejects(items: [try Fixture.item(at: a)], mode: .trash, libraryRoot: root, now: now)
        } catch {
            throw XCTSkip("Trash not available here: \(error)")
        }
        if !result.failed.isEmpty { throw XCTSkip("Trash not available here: \(result.failed)") }
        XCTAssertEqual(result.moved, 1)
        XCTAssertFalse(FileManager.default.fileExists(atPath: a.path))
        let manifest = try LibraryStore.readManifest(at: try XCTUnwrap(result.manifestURL))
        XCTAssertEqual(manifest.mode, .trash)
        XCTAssertFalse(manifest.entries[0].resultPath.isEmpty)
        let restored = CommitService.restore(manifest: manifest)
        XCTAssertEqual(restored.restored, 1, "\(restored.failed)")
        XCTAssertTrue(FileManager.default.fileExists(atPath: a.path))
    }
}
