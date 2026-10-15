// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import CoreImage
import Foundation
import ImageIO
import UniformTypeIdentifiers
import XCTest
@testable import Unswiper

private func item(_ path: String, _ d: Decision = .undecided, source: String? = nil, dismissed: Bool? = nil) -> MediaItem {
    var i = MediaItem(path: path, kind: .photo, sizeBytes: 1, takenAt: Date(timeIntervalSince1970: 1_700_000_000),
                      relativeFolder: "", decision: d)
    i.decisionSource = source
    i.blurDismissed = dismissed
    return i
}

/// Same cases as `unswiper_core::quality` tests, so both apps follow the same rules.
final class BlurRulesTests: XCTestCase {
    func testApplySuggestsOnlyUndecidedAndNotDismissed() {
        var items = [item("/a"), item("/b", .keep), item("/c", .reject), item("/d", dismissed: true), item("/e")]
        let all: Set = ["/a", "/b", "/c", "/d", "/e"]
        let r = BlurRules.apply(&items, flagged: ["/a", "/b", "/c", "/d"], scored: all, enabled: true)
        XCTAssertEqual(r, .init(suggested: 1, withdrawn: 0))
        XCTAssertTrue(BlurRules.isSuggested(items[0]))
        XCTAssertEqual(items[1].decision, .keep)
        XCTAssertFalse(BlurRules.isSuggested(items[2]), "a manual Reject is not a suggestion")
        XCTAssertEqual(items[3].decision, .undecided)
        XCTAssertEqual(items[4].decision, .undecided)
    }

    func testWithdrawOnlyWhereScoredAndWhenOff() {
        var items = [item("/a", .reject, source: "auto-blur"), item("/b", .reject, source: "auto-blur")]
        // /a scored and no longer flagged -> withdrawn; /b not scored this time -> kept.
        var r = BlurRules.apply(&items, flagged: [], scored: ["/a"], enabled: true)
        XCTAssertEqual(r, .init(suggested: 0, withdrawn: 1))
        XCTAssertEqual(items[0].decision, .undecided)
        XCTAssertNil(items[0].decisionSource)
        XCTAssertTrue(BlurRules.isSuggested(items[1]))
        r = BlurRules.apply(&items, flagged: ["/a", "/b"], scored: ["/a", "/b"], enabled: false)
        XCTAssertEqual(r.withdrawn, 1)
        XCTAssertEqual(r.suggested, 0)
        var more = [item("/x", .reject, source: "auto-blur")]
        XCTAssertEqual(BlurRules.withdrawAll(&more), 1)
    }

    func testManualDecisionWinsAndIsRemembered() {
        var i = item("/a", .reject, source: "auto-blur")
        XCTAssertTrue(BlurRules.setManualDecision(&i, .undecided))
        XCTAssertEqual(i.blurDismissed, true)
        XCTAssertNil(i.decisionSource)
        var items = [i]
        XCTAssertEqual(BlurRules.apply(&items, flagged: ["/a"], scored: ["/a"], enabled: true).suggested, 0)
        var k = item("/k", .reject, source: "auto-blur")
        BlurRules.setManualDecision(&k, .keep)
        XCTAssertEqual(k.blurDismissed, true)
        var r = item("/r", .reject, source: "auto-blur")
        BlurRules.setManualDecision(&r, .reject)
        XCTAssertNil(r.blurDismissed, "confirming as Reject is not a dismissal")
        XCTAssertNil(r.decisionSource)
        var plain = item("/p")
        XCTAssertFalse(BlurRules.setManualDecision(&plain, .undecided))
    }

    func testCarryOverConfirmAndDismiss() {
        let old = [item("/a", .reject, source: "auto-blur"), item("/b", dismissed: true), item("/c", .reject, source: "auto-blur")]
        var fresh = [item("/a", .reject), item("/b"), item("/c", .undecided), item("/new")]
        BlurRules.carryOver(from: old, to: &fresh)
        XCTAssertTrue(BlurRules.isSuggested(fresh[0]))
        XCTAssertEqual(fresh[1].blurDismissed, true)
        XCTAssertNil(fresh[2].decisionSource)
        var items = [item("/a", .reject, source: "auto-blur"), item("/b", .reject, source: "auto-blur"), item("/z", .reject, source: "auto-blur")]
        XCTAssertEqual(BlurRules.confirm(&items, paths: ["/a"]), 1)
        XCTAssertEqual(items[0].decision, .reject)
        XCTAssertNil(items[0].decisionSource)
        XCTAssertEqual(BlurRules.dismiss(&items, paths: ["/b"]), 1)
        XCTAssertEqual(items[1].decision, .undecided)
        XCTAssertEqual(items[1].blurDismissed, true)
        XCTAssertTrue(BlurRules.isSuggested(items[2]), "other visits untouched")
    }

    func testSettingsKeys() {
        var c = AppConfig.default
        XCTAssertTrue(BlurRules.enabled(c))
        XCTAssertEqual(BlurRules.sensitivity(c), .normal)
        BlurRules.setEnabled(&c, false)
        XCTAssertEqual(c.blurDetection, false)
        BlurRules.setEnabled(&c, true)
        XCTAssertNil(c.blurDetection, "on is the default and stays unwritten")
        BlurRules.setSensitivity(&c, .high)
        XCTAssertEqual(c.blurSensitivity, "high")
        BlurRules.setSensitivity(&c, .normal)
        XCTAssertNil(c.blurSensitivity)
        c.blurSensitivity = "bogus"
        XCTAssertEqual(BlurRules.sensitivity(c), .normal)
    }
}

/// library.json written by Linux keeps its blur keys through a Mac load + save (no dropping),
/// and files without blur data get no new keys.
final class BlurPersistenceTests: StoreIsolatedTestCase {
    func testSuggestionKeysSurviveMacSave() throws {
        let json = """
        {"schemaVersion":1,"config":{"rootPath":"/p","keepTargetDefault":10,"visitGapHours":36,"commitMode":"trash",
         "blurDetection":false,"blurSensitivity":"high"},
         "items":[{"path":"/p/a.jpg","kind":"photo","sizeBytes":1,"takenAt":"2026-10-01T10:00:00Z","relativeFolder":"",
                   "decision":"reject","decisionSource":"auto-blur"},
                  {"path":"/p/b.jpg","kind":"photo","sizeBytes":1,"takenAt":"2026-10-01T10:00:00Z","relativeFolder":"",
                   "decision":"undecided","blurDismissed":true},
                  {"path":"/p/c.jpg","kind":"photo","sizeBytes":1,"takenAt":"2026-10-01T10:00:00Z","relativeFolder":"",
                   "decision":"keep"}],
         "groups":[]}
        """
        try json.data(using: .utf8)!.write(to: LibraryStore.snapshotURL)
        let snap = LibraryStore.load().snapshot
        XCTAssertEqual(snap.config.blurDetection, false)
        XCTAssertEqual(snap.config.blurSensitivity, "high")
        XCTAssertTrue(BlurRules.isSuggested(snap.items[0]))
        XCTAssertEqual(snap.items[1].blurDismissed, true)
        try LibraryStore.save(snap)
        let text = try String(contentsOf: LibraryStore.snapshotURL, encoding: .utf8)
        XCTAssertTrue(text.contains("\"decisionSource\" : \"auto-blur\""), text)
        XCTAssertTrue(text.contains("\"blurDismissed\" : true"))
        XCTAssertTrue(text.contains("\"blurDetection\" : false"))
        XCTAssertTrue(text.contains("\"blurSensitivity\" : \"high\""))
        XCTAssertEqual(text.components(separatedBy: "decisionSource").count - 1, 1, "c.jpg gets no blur keys")
        XCTAssertEqual(text.components(separatedBy: "blurDismissed").count - 1, 1)
    }

    @MainActor func testCheckBlurAppliesVerdictVisitFirstAndRespectsUser() async throws {
        let state = AppState()
        state.saveDelay = .milliseconds(10)
        state.items = [item("/v/a.jpg"), item("/v/b.jpg"), item("/o/c.jpg")]
        state.groups = [VisitGroup(id: "v", title: "V", startDate: Date(), endDate: Date(), itemPaths: ["/v/a.jpg", "/v/b.jpg"],
                                   photoCount: 2, videoCount: 0, totalBytes: 2, folderHint: "")]
        var calls: [[String]?] = []
        state.blurRunner = { library, sensitivity, only in
            calls.append(only)
            XCTAssertEqual(sensitivity, .normal)
            XCTAssertTrue(FileManager.default.fileExists(atPath: library.path), "library.json flushed first")
            return BlurHelper.Verdict(flagged: ["/v/a.jpg", "/o/c.jpg"], scored: ["/v/a.jpg", "/v/b.jpg", "/o/c.jpg"])
        }
        state.checkBlur(visitFirst: "v")
        try await waitUntil { state.blurProgress == nil && calls.count == 2 }
        XCTAssertEqual(calls.first??.sorted(), ["/v/a.jpg", "/v/b.jpg"])
        XCTAssertNil(calls.last!)
        XCTAssertTrue(state.isBlurSuggested("/v/a.jpg"))
        XCTAssertTrue(state.isBlurSuggested("/o/c.jpg"))
        XCTAssertEqual(state.suggestedPaths(in: state.groups[0]), ["/v/a.jpg"])
        XCTAssertEqual(state.statusMessage, "2 photos look blurry and were marked Reject. Nothing moves until you commit.")
        // U on a suggestion: cleared for good.
        state.updateDecision(paths: ["/v/a.jpg"], decision: .undecided)
        state.checkBlur(visitFirst: nil)
        try await waitUntil { state.blurProgress == nil && calls.count == 3 }
        XCTAssertFalse(state.isBlurSuggested("/v/a.jpg"))
        XCTAssertEqual(state.items[0].decision, .undecided)
        // Turning detection off withdraws the rest; the runner is not called while off.
        state.setBlurEnabled(false)
        XCTAssertFalse(state.isBlurSuggested("/o/c.jpg"))
        XCTAssertEqual(state.items[2].decision, .undecided)
        state.checkBlur(visitFirst: nil)
        XCTAssertNil(state.blurProgress)
        XCTAssertEqual(calls.count, 3)
    }

    @MainActor func testBulkVisitActions() {
        let state = AppState()
        state.items = [item("/v/a.jpg", .reject, source: "auto-blur"), item("/v/b.jpg", .reject, source: "auto-blur"),
                       item("/o/c.jpg", .reject, source: "auto-blur")]
        state.groups = [VisitGroup(id: "v", title: "V", startDate: Date(), endDate: Date(), itemPaths: ["/v/a.jpg"],
                                   photoCount: 1, videoCount: 0, totalBytes: 1, folderHint: ""),
                        VisitGroup(id: "w", title: "W", startDate: Date(), endDate: Date(), itemPaths: ["/v/b.jpg"],
                                   photoCount: 1, videoCount: 0, totalBytes: 1, folderHint: "")]
        state.dismissSuggestions(in: "v")
        XCTAssertEqual(state.items[0].decision, .undecided)
        XCTAssertEqual(state.items[0].blurDismissed, true)
        state.confirmSuggestions(in: "w")
        XCTAssertEqual(state.items[1].decision, .reject)
        XCTAssertNil(state.items[1].decisionSource)
        XCTAssertTrue(state.isBlurSuggested("/o/c.jpg"))
    }

    @MainActor private func waitUntil(_ cond: @escaping () -> Bool, timeout: Double = 5) async throws {
        let end = Date().addingTimeInterval(timeout)
        while !cond() {
            if Date() > end { XCTFail("timed out"); return }
            try await Task.sleep(for: .milliseconds(20))
        }
    }
}

final class BlurHelperTests: XCTestCase {
    func testParseArgumentsAndLocate() throws {
        let data = """
        [{"path":"/a.jpg","score":{"sharpness":1.0},"blurry":true,"reason":"soft"},
         {"path":"/b.jpg","score":{"sharpness":90.0},"blurry":false,"reason":null}]
        """.data(using: .utf8)!
        let v = try BlurHelper.parse(data)
        XCTAssertEqual(v.flagged, ["/a.jpg"])
        XCTAssertEqual(v.scored, ["/a.jpg", "/b.jpg"])
        XCTAssertThrowsError(try BlurHelper.parse(Data("{".utf8)))
        let lib = URL(fileURLWithPath: "/s/library.json")
        XCTAssertEqual(BlurHelper.arguments(library: lib, sensitivity: .low, onlyList: nil),
                       ["scan", "/s/library.json", "--json", "--sensitivity", "low", "--write-cache"])
        XCTAssertEqual(BlurHelper.arguments(library: lib, sensitivity: .high, onlyList: URL(fileURLWithPath: "/t/l.txt")).suffix(2),
                       ["--only", "/t/l.txt"])
        // Sibling of the executable, as inside Unswiper.app/Contents/MacOS.
        let dir = try Fixture.tempDir("helper")
        defer { try? FileManager.default.removeItem(at: dir) }
        let exe = dir.appendingPathComponent("Unswiper")
        let helper = dir.appendingPathComponent("unswiper-blur")
        for f in [exe, helper] {
            try Data("#!/bin/sh\n".utf8).write(to: f)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: f.path)
        }
        XCTAssertEqual(BlurHelper.locate(environment: [:], executable: exe)?.lastPathComponent, "unswiper-blur")
        XCTAssertEqual(BlurHelper.locate(environment: ["UNSWIPER_BLUR_HELPER": exe.path], executable: nil), exe)
        XCTAssertNil(BlurHelper.locate(environment: [:], executable: dir.appendingPathComponent("sub/none")))
    }
}

/// End to end with the real Rust helper. Runs when UNSWIPER_BLUR_HELPER points at a build
/// (CI builds it; locally: `cargo build --release -p unswiper-blur --no-default-features` and
/// export UNSWIPER_BLUR_HELPER=$PWD/target/release/unswiper-blur).
final class BlurHelperIntegrationTests: StoreIsolatedTestCase {
    func testRealHelperFlagsBlurredPhoto() throws {
        guard let helper = BlurHelper.locate(executable: nil) else {
            throw XCTSkip("UNSWIPER_BLUR_HELPER not set")
        }
        let root = try makeRoot()
        let sharp = root.appendingPathComponent("IMG_1.jpg")
        let soft = root.appendingPathComponent("IMG_2.jpg")
        let texture = Self.texture(width: 640, height: 480)
        try Self.writeJPEG(texture, to: sharp)
        try Self.writeJPEG(Self.blur(texture, radius: 5), to: soft)
        var snap = LibraryStore.emptySnapshot
        snap.items = [try Fixture.item(at: sharp, decision: .undecided), try Fixture.item(at: soft, decision: .undecided)]
        snap.items[1].takenAt = snap.items[0].takenAt.addingTimeInterval(3600)
        try LibraryStore.save(snap)
        let v = try BlurHelper.run(helper: helper, library: LibraryStore.snapshotURL, sensitivity: .normal, only: [soft.path])
        XCTAssertEqual(v.flagged, [soft.path])
        XCTAssertEqual(v.scored, [soft.path], "--only scored just the listed photo")
        XCTAssertTrue(FileManager.default.fileExists(atPath: supportDir.appendingPathComponent("quality-cache.json").path))
        let all = try BlurHelper.run(helper: helper, library: LibraryStore.snapshotURL, sensitivity: .normal, only: nil)
        XCTAssertEqual(all.scored, [sharp.path, soft.path])
        XCTAssertEqual(all.flagged, [soft.path])
    }

    /// A scene of random grey rectangles: strong edges at many scales, like `synth::scene`.
    static func texture(width: Int, height: Int) -> CGImage {
        let ctx = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                            space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue)!
        ctx.setFillColor(gray: 0.5, alpha: 1)
        ctx.fill(CGRect(x: 0, y: 0, width: width, height: height))
        var rng = SystemRandomNumberGenerator()
        for _ in 0..<400 {
            ctx.setFillColor(gray: CGFloat.random(in: 0.15...0.9, using: &rng), alpha: 1)
            let w = CGFloat.random(in: 6...90, using: &rng), h = CGFloat.random(in: 6...90, using: &rng)
            ctx.fill(CGRect(x: CGFloat.random(in: 0...CGFloat(width), using: &rng),
                            y: CGFloat.random(in: 0...CGFloat(height), using: &rng), width: w, height: h))
        }
        return ctx.makeImage()!
    }

    static func blur(_ img: CGImage, radius: Double) -> CGImage {
        let ci = CIImage(cgImage: img).clampedToExtent().applyingGaussianBlur(sigma: radius)
            .cropped(to: CGRect(x: 0, y: 0, width: img.width, height: img.height))
        return CIContext().createCGImage(ci, from: ci.extent)!
    }

    static func writeJPEG(_ img: CGImage, to url: URL) throws {
        let dest = try XCTUnwrap(CGImageDestinationCreateWithURL(url as CFURL, UTType.jpeg.identifier as CFString, 1, nil))
        CGImageDestinationAddImage(dest, img, [kCGImageDestinationLossyCompressionQuality: 0.95] as CFDictionary)
        XCTAssertTrue(CGImageDestinationFinalize(dest))
    }
}
