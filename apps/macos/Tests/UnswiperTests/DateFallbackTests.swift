// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation
import ImageIO
import XCTest
@testable import Unswiper

/// M3 (plan §20.14): GPS timestamp and file-name dates, and GPS read by the Mac indexer.
/// The cases match the Rust tests in `unswiper_core::meta::file_name_date`.
final class DateFallbackTests: StoreIsolatedTestCase {
    private func parts(_ s: String) -> String? {
        FileNameDate.parseParts(stem: s).map {
            String(format: "%04d-%02d-%02d %02d:%02d:%02d", $0.year, $0.month, $0.day, $0.hour, $0.minute, $0.second)
        }
    }

    func testCameraFileNames() {
        XCTAssertEqual(parts("IMG_20240612_133241"), "2024-06-12 13:32:41")
        XCTAssertEqual(parts("IMG_20240612_133241-EFFECTS"), "2024-06-12 13:32:41")
        XCTAssertEqual(parts("PXL_20240612_133241123"), "2024-06-12 13:32:41")
        XCTAssertEqual(parts("PANO_20240612_133241"), "2024-06-12 13:32:41")
        XCTAssertEqual(parts("20240612_133241"), "2024-06-12 13:32:41")
        XCTAssertEqual(parts("IMG-20240612-WA0001"), "2024-06-12 12:00:00")
        XCTAssertEqual(parts("00000IMG_00000_BURST20240612133241123_COVER"), "2024-06-12 13:32:41")
        XCTAssertEqual(parts("Screenshot_2024-06-12-13-32-41"), "2024-06-12 13:32:41")
        XCTAssertEqual(parts("2024-06-12 13.32.41"), "2024-06-12 13:32:41")
        XCTAssertEqual(parts("Trip 2024-06-12"), "2024-06-12 12:00:00")
        XCTAssertEqual(parts("IMG_20240612_996000"), "2024-06-12 12:00:00", "a bad time keeps the day")
        for s in ["IMG_1234", "DSCF0001", "P1000123", "IMG_20241332_120000", "IMG_19800101_120000",
                  "123456789", "IMG_2024", "abc", ""] {
            XCTAssertNil(parts(s), s)
        }
    }

    func testPixelNamesAreUTC() {
        let utc = TimeZone(identifier: "UTC")!
        let la = TimeZone(identifier: "America/Los_Angeles")!
        XCTAssertEqual(FileNameDate.parse(fileName: "PXL_20240612_133241123.jpg", timeZone: la),
                       FileNameDate.parse(fileName: "IMG_20240612_133241.jpg", timeZone: utc))
        XCTAssertNil(FileNameDate.parse(fileName: "IMG_1234.JPG"))
    }

    func testGPSDictionary() {
        let gps: [CFString: Any] = [
            kCGImagePropertyGPSLatitude: 37.5, kCGImagePropertyGPSLatitudeRef: "N",
            kCGImagePropertyGPSLongitude: 122.25, kCGImagePropertyGPSLongitudeRef: "W",
            kCGImagePropertyGPSDateStamp: "2024:06:12", kCGImagePropertyGPSTimeStamp: "13:32:41.50",
        ]
        let info = LocalTreeIndexer.gpsInfo(fromGPSDictionary: gps)
        XCTAssertEqual(info.location, GeoLocation(lat: 37.5, lon: -122.25, source: "exif"))
        XCTAssertEqual(info.date, Date(timeIntervalSince1970: 1_718_199_161))
        XCTAssertNil(LocalTreeIndexer.makeLocation(lat: 0, lon: 0, source: "exif"), "0,0 is never stored")
        XCTAssertNil(LocalTreeIndexer.gpsDate(day: "0000:00:00", time: "00:00:00"))
    }

    func testISO6709() {
        XCTAssertEqual(LocalTreeIndexer.parseISO6709("+37.5000-122.2500+010.000/"),
                       GeoLocation(lat: 37.5, lon: -122.25, source: "quicktime"))
        XCTAssertEqual(LocalTreeIndexer.parseISO6709("-33.8688+151.2093/"),
                       GeoLocation(lat: -33.8688, lon: 151.2093, source: "quicktime"))
        XCTAssertNil(LocalTreeIndexer.parseISO6709("+00.0000+000.0000/"))
        XCTAssertNil(LocalTreeIndexer.parseISO6709("junk"))
    }

    func testIndexerUsesFileNameBeforeFolder() throws {
        let root = try Fixture.tempDir("m3")
        cleanup.append(root)
        try Fixture.file(root.appendingPathComponent("2023-01-01 Old/IMG_20240613_101500.jpg"))
        try Fixture.file(root.appendingPathComponent("2023-01-01 Old/IMG_1234.jpg"))
        let r = try LocalTreeIndexer.index(root: root, visitGapHours: 36, existingDecisions: [:], onProgress: { _ in })
        let named = r.items.first { $0.fileName == "IMG_20240613_101500.jpg" }!
        XCTAssertEqual(named.dateSource, .fileName)
        XCTAssertFalse(named.isUndated)
        XCTAssertEqual(named.takenAt, Fixture.localDate(2024, 6, 13, 10, 15, 0))
        XCTAssertEqual(r.items.first { $0.fileName == "IMG_1234.jpg" }!.dateSource, .folder)
    }

    func testDateSourceStringsMatchRust() throws {
        let data = try JSONEncoder().encode([DateSource.gps, .fileName])
        XCTAssertEqual(String(data: data, encoding: .utf8), "[\"gps\",\"filename\"]")
    }
}
