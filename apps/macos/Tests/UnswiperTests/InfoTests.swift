// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation
import ImageIO
import UniformTypeIdentifiers
import XCTest
@testable import Unswiper

/// The golden table shared with Rust (crates/unswiper-core/testdata/info-format-golden.json).
private struct GoldenCase: Decodable {
    var name: String
    var rows: [InfoHelper.Row]
}

private func goldenURL() -> URL {
    var url = URL(fileURLWithPath: #filePath)
    for _ in 0..<4 { url.deleteLastPathComponent() } // UnswiperTests, Tests, macos, apps
    url.deleteLastPathComponent()
    return url.appendingPathComponent("crates/unswiper-core/testdata/info-format-golden.json")
}

private func goldenCases() throws -> [GoldenCase] {
    try JSONDecoder().decode([GoldenCase].self, from: Data(contentsOf: goldenURL()))
}

final class InfoRulesTests: XCTestCase {
    func testTargetFollowsViewerThenSelectionInGridOrder() {
        let order = ["/a", "/b", "/c"]
        XCTAssertNil(InfoRules.target(viewer: nil, selected: [], ordered: order))
        let t = InfoRules.target(viewer: nil, selected: ["/c", "/b"], ordered: order)
        XCTAssertEqual(t?.path, "/b", "first selected in grid order")
        XCTAssertEqual(t?.others, 1)
        let v = InfoRules.target(viewer: "/a", selected: ["/c"], ordered: order)
        XCTAssertEqual(v?.path, "/a")
        XCTAssertEqual(v?.others, 0)
        XCTAssertNil(InfoRules.target(viewer: nil, selected: ["/x"], ordered: order), "selection filtered out")
        XCTAssertEqual(InfoRules.target(viewer: "/gone", selected: ["/c"], ordered: order)?.path, "/c")
    }

    func testHeadingTitlesAndCompareFocus() {
        XCTAssertEqual(InfoRules.heading(fileName: "IMG_1.JPG", others: 0), "IMG_1.JPG")
        XCTAssertEqual(InfoRules.heading(fileName: "IMG_1.JPG", others: 2), "IMG_1.JPG (and 2 more selected)")
        XCTAssertEqual(InfoRules.title(ofGroup: "exposure"), "Exposure")
        XCTAssertEqual(InfoRules.title(ofGroup: "unswiper"), "Unswiper")
        XCTAssertEqual(InfoRules.title(ofGroup: "future"), "Future")
        XCTAssertEqual(InfoRules.compareFocus(nil, in: ["/a", "/b"]), "/a")
        XCTAssertEqual(InfoRules.compareFocus("/b", in: ["/a", "/b"]), "/b")
        XCTAssertEqual(InfoRules.compareFocus("/x", in: ["/a", "/b"]), "/a")
        XCTAssertNil(InfoRules.compareFocus(nil, in: []))
    }

    func testMapsURL() {
        XCTAssertEqual(InfoRules.mapsURL(coordinates: "37.77493, -122.41942")?.absoluteString,
                       "https://www.openstreetmap.org/?mlat=37.77493&mlon=-122.41942#map=16/37.77493/-122.41942")
        XCTAssertNil(InfoRules.mapsURL(coordinates: "95.0, 10.0"))
        XCTAssertNil(InfoRules.mapsURL(coordinates: "nowhere"))
    }

    /// The Rust golden rows decode into the Swift types and group in the shared order.
    func testGoldenTableDecodesAndGroups() throws {
        let cases = try goldenCases()
        XCTAssertGreaterThanOrEqual(cases.count, 5)
        let iphone = try XCTUnwrap(cases.first { $0.name == "iphone_heic_full" })
        XCTAssertEqual(InfoRules.sections(iphone.rows, shortOnly: false).map(\.title),
                       ["Camera", "Exposure", "Date", "Image", "File", "Location", "Unswiper"])
        let short = InfoRules.sections(iphone.rows, shortOnly: true)
        XCTAssertFalse(short.contains { $0.title == "Location" }, "the popover has no location")
        XCTAssertTrue(short.allSatisfy { $0.rows.allSatisfy(\.short) })
        let hidden = try XCTUnwrap(cases.first { $0.name == "dslr_jpeg_location_hidden" })
        XCTAssertFalse(hidden.rows.contains { $0.group == "location" })
        XCTAssertEqual(hidden.rows.last?.value, "Blurry")
    }

    func testHelperArgumentsAndParse() throws {
        let s = InfoHelper.Settings(showLocation: false, blurEnabled: false, sensitivity: .high)
        XCTAssertEqual(InfoHelper.arguments(files: ["/a.jpg"], library: URL(fileURLWithPath: "/l/library.json"), settings: s),
                       ["info", "--library", "/l/library.json", "--sensitivity", "high", "--blur-off", "--hide-location", "/a.jpg"])
        let on = InfoHelper.Settings(showLocation: true, blurEnabled: true, sensitivity: .normal)
        XCTAssertEqual(InfoHelper.arguments(files: ["/a.jpg", "/b.mov"], library: nil, settings: on),
                       ["info", "--sensitivity", "normal", "/a.jpg", "/b.mov"])
        let data = """
        [{"path":"/a.jpg","rows":[{"group":"camera","label":"Camera","value":"Canon EOS R6","short":true}],
          "strip":"f/8 · ISO 100"}]
        """.data(using: .utf8)!
        let a = try InfoHelper.parse(data)
        XCTAssertEqual(a.first?.rows.first?.value, "Canon EOS R6")
        XCTAssertEqual(a.first?.strip, "f/8 · ISO 100")
        XCTAssertThrowsError(try InfoHelper.parse(Data("{}".utf8)))
    }
}

final class InfoStateTests: StoreIsolatedTestCase {
    @MainActor func testShowLocationSettingIsSavedSharedWithLinux() throws {
        let state = AppState()
        XCTAssertTrue(state.showLocation)
        state.setShowLocation(false)
        XCTAssertFalse(state.showLocation)
        state.flush()
        let text = try String(contentsOf: LibraryStore.snapshotURL, encoding: .utf8)
        XCTAssertTrue(text.contains("\"infoShowLocation\" : false"), text)
        XCTAssertFalse(AppState().showLocation, "reloaded from library.json")
        state.setShowLocation(true)
        state.flush()
        let back = try String(contentsOf: LibraryStore.snapshotURL, encoding: .utf8)
        XCTAssertFalse(back.contains("infoShowLocation"), "default on is not written")
    }

    @MainActor func testLoadInfoCachesPerSettingsAndNeverStoresLocation() async throws {
        let state = AppState()
        var calls: [([String], InfoHelper.Settings)] = []
        state.infoRunner = { files, settings in
            calls.append((files, settings))
            return files.map { f in
                var rows = [InfoHelper.Row(group: "camera", label: "Camera", value: "Canon", short: true)]
                if settings.showLocation {
                    rows.append(.init(group: "location", label: "Coordinates", value: "37.50000, -122.25000", short: false))
                }
                return InfoHelper.Answer(path: f, rows: rows, strip: "f/8")
            }
        }
        state.loadInfo(["/a.jpg"])
        try await waitUntil { state.infoAnswer("/a.jpg") != nil }
        XCTAssertEqual(state.infoAnswer("/a.jpg")?.rows.count, 2)
        state.loadInfo(["/a.jpg"])
        XCTAssertEqual(calls.count, 1, "cached")
        state.setShowLocation(false)
        XCTAssertNil(state.infoAnswer("/a.jpg"), "settings changed: not reused")
        state.loadInfo(["/a.jpg"])
        try await waitUntil { state.infoAnswer("/a.jpg") != nil }
        XCTAssertEqual(calls.count, 2)
        XCTAssertFalse(calls[1].1.showLocation)
        XCTAssertEqual(state.infoAnswer("/a.jpg")?.rows.count, 1)
        // Privacy: coordinates never reach library.json.
        state.setShowLocation(true)
        state.loadInfo(["/a.jpg"])
        try await waitUntil { state.infoAnswer("/a.jpg")?.rows.count == 2 }
        state.flush()
        let text = try String(contentsOf: LibraryStore.snapshotURL, encoding: .utf8)
        XCTAssertFalse(text.contains("37.5"), text)
        // Failures are shown, not thrown.
        state.infoRunner = { _, _ in throw BlurHelper.HelperError.failed("boom") }
        state.loadInfo(["/b.jpg"])
        try await waitUntil { state.infoProblem != nil }
        XCTAssertTrue(state.infoProblem?.contains("boom") ?? false)
        let gen = state.infoGeneration
        state.resetInfo()
        XCTAssertEqual(state.infoGeneration, gen + 1)
        XCTAssertNil(state.infoAnswer("/a.jpg"))
    }

    @MainActor private func waitUntil(_ cond: @escaping () -> Bool, timeout: Double = 5) async throws {
        let end = Date().addingTimeInterval(timeout)
        while !cond() {
            if Date() > end { XCTFail("timed out"); return }
            try await Task.sleep(for: .milliseconds(20))
        }
    }
}

/// End to end with the real helper (skipped without it, like the blur integration test).
final class InfoHelperIntegrationTests: StoreIsolatedTestCase {
    func testHelperFormatsTheGoldenTableLikeRust() throws {
        guard let helper = BlurHelper.locate(executable: nil) else { throw XCTSkip("UNSWIPER_BLUR_HELPER not set") }
        let data = try InfoHelper.runHelper(helper, arguments: ["info", "--format", goldenURL().path])
        let got = try JSONDecoder().decode([[InfoHelper.Row]].self, from: data)
        XCTAssertEqual(got, try goldenCases().map(\.rows))
    }

    func testHelperReadsCameraAndLocationFromAJPEG() throws {
        guard let helper = BlurHelper.locate(executable: nil) else { throw XCTSkip("UNSWIPER_BLUR_HELPER not set") }
        let root = try makeRoot()
        let url = root.appendingPathComponent("IMG_1.jpg")
        let img = BlurHelperIntegrationTests.texture(width: 64, height: 48)
        let dest = try XCTUnwrap(CGImageDestinationCreateWithURL(url as CFURL, UTType.jpeg.identifier as CFString, 1, nil))
        let props: [CFString: Any] = [
            kCGImagePropertyTIFFDictionary: [kCGImagePropertyTIFFMake: "Canon", kCGImagePropertyTIFFModel: "Canon EOS R6"],
            kCGImagePropertyExifDictionary: [kCGImagePropertyExifFNumber: 2.8, kCGImagePropertyExifISOSpeedRatings: [400]],
            kCGImagePropertyGPSDictionary: [
                kCGImagePropertyGPSLatitude: 37.5, kCGImagePropertyGPSLatitudeRef: "N",
                kCGImagePropertyGPSLongitude: 122.25, kCGImagePropertyGPSLongitudeRef: "W",
            ],
        ]
        CGImageDestinationAddImage(dest, img, props as CFDictionary)
        XCTAssertTrue(CGImageDestinationFinalize(dest))
        let on = InfoHelper.Settings(showLocation: true, blurEnabled: true, sensitivity: .normal)
        let a = try XCTUnwrap(InfoHelper.run(helper: helper, files: [url.path], library: nil, settings: on).first)
        XCTAssertEqual(a.path, url.path)
        XCTAssertEqual(a.rows.first { $0.label == "Camera" }?.value, "Canon EOS R6")
        XCTAssertEqual(a.rows.first { $0.label == "Aperture" }?.value, "f/2.8")
        XCTAssertEqual(a.rows.first { $0.label == "ISO" }?.value, "ISO 400")
        XCTAssertEqual(a.rows.first { $0.label == "Coordinates" }?.value, "37.50000, -122.25000")
        XCTAssertEqual(a.rows.first { $0.label == "Dimensions" }?.value, "0.1 MP · 64 × 48")
        XCTAssertTrue(a.strip.contains("f/2.8"), a.strip)
        var off = on
        off.showLocation = false
        let hidden = try XCTUnwrap(InfoHelper.run(helper: helper, files: [url.path], library: nil, settings: off).first)
        XCTAssertFalse(hidden.rows.contains { $0.group == "location" })
    }
}
