// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import XCTest
@testable import Unswiper

final class BlurCutoffsTests: XCTestCase {
    func testDefaultsValidationAndSaving() {
        var c = AppConfig.default
        XCTAssertEqual(BlurRules.cutoffs(c), .default)
        XCTAssertEqual(BlurCutoffs.default.value(.normal), 13)
        XCTAssertNil(BlurRules.setCutoffs(&c, BlurCutoffs(low: 4.5, normal: 9, high: 20.04)))
        XCTAssertEqual(c.blurCutoffs, BlurCutoffs(low: 4.5, normal: 9, high: 20), "rounded to 0.1")
        XCTAssertEqual(BlurRules.cutoffs(c).value(.high), 20)
        for bad in [(9.0, 9.0, 20.0), (10, 9, 20), (4, 30, 20), (0, 9, 20), (-1, 9, 20), (4, 9, 2000), (.nan, 9, 20)] {
            XCTAssertNotNil(BlurRules.setCutoffs(&c, BlurCutoffs(low: bad.0, normal: bad.1, high: bad.2)), "\(bad)")
        }
        XCTAssertEqual(c.blurCutoffs?.normal, 9, "a refused change keeps the old values")
        XCTAssertTrue(BlurCutoffs(low: 10, normal: 9, high: 20).problem!.contains("Low below Normal"))
        XCTAssertNil(BlurRules.setCutoffs(&c, .default))
        XCTAssertNil(c.blurCutoffs, "the defaults are stored as a missing key")
    }

    func testSharedJSONWithLinux() throws {
        // As the Linux app writes it; partial and broken values fall back to the defaults.
        let decode = { (json: String) in
            try JSONDecoder().decode(BlurCutoffs.self, from: Data(json.utf8))
        }
        XCTAssertEqual(try decode(#"{"high":60,"low":7,"normal":40}"#), BlurCutoffs(low: 7, normal: 40, high: 60))
        XCTAssertEqual(try decode(#"{"normal":10}"#), BlurCutoffs(low: 7, normal: 10, high: 25))
        XCTAssertEqual(try decode(#"{"normal":"x"}"#), .default)
        var c = AppConfig.default
        c.blurCutoffs = BlurCutoffs(low: 30, normal: 9, high: 20) // hand-edited, out of order
        XCTAssertEqual(BlurRules.cutoffs(c), .default)
        let encoded = String(decoding: try JSONEncoder().encode(BlurCutoffs(low: 7, normal: 40, high: 60)), as: UTF8.self)
        XCTAssertTrue(encoded.contains(#""normal":40"#), encoded)
    }

    func testViewerTexts() {
        XCTAssertEqual(BlurRules.viewerTexts(flag: "Blurry?", suggested: true, score: 42.66).flag, "Blurry? · 42.7")
        XCTAssertEqual(BlurRules.viewerTexts(flag: "Blurry?", suggested: true, score: 42.66).score, "")
        XCTAssertEqual(BlurRules.viewerTexts(flag: "Keep", suggested: false, score: 130).score, "Sharpness 130.0")
        XCTAssertEqual(BlurRules.viewerTexts(flag: "", suggested: false, score: nil).flag, "")
    }

    func testScoresFromTheHelperCache() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("bc-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let photo = dir.appendingPathComponent("a.jpg")
        try Data(repeating: 1, count: 10).write(to: photo)
        let mtime = Int64((try FileManager.default.attributesOfItem(atPath: photo.path)[.modificationDate] as! Date)
            .timeIntervalSince1970.rounded(.down))
        let entry = { (path: String, conf: Double, size: Int) in
            #""\#(path)":{"sharpness":42.66,"global":1,"motion":0.1,"meanLuma":120,"confidence":\#(conf),"fileSize":\#(size),"mtime":\#(mtime),"scoredAt":0}"#
        }
        let dark = dir.appendingPathComponent("d.jpg")
        try Data(repeating: 1, count: 10).write(to: dark)
        let json = #"{"v":1,"algo":1,"entries":{\#(entry(photo.path, 1, 10)),\#(entry(dark.path, 0.2, 10))}}"#
        let cacheURL = dir.appendingPathComponent("quality-cache.json")
        try Data(json.utf8).write(to: cacheURL)
        let scores = BlurScores.load(cacheURL)
        let item = { (url: URL) in
            MediaItem(path: url.path, kind: .photo, sizeBytes: 10, takenAt: Date(), relativeFolder: "", decision: .undecided)
        }
        XCTAssertEqual(scores.score(for: item(photo)), 42.66)
        XCTAssertNil(scores.score(for: item(dark)), "too dark to judge")
        try Data(repeating: 1, count: 11).write(to: photo)
        XCTAssertNil(scores.score(for: item(photo)), "changed since it was scored")
        try Data(#"{"v":1,"algo":2,"entries":{}}"#.utf8).write(to: cacheURL)
        XCTAssertTrue(BlurScores.load(cacheURL).entries.isEmpty, "another algorithm's scores are ignored")
    }
}
