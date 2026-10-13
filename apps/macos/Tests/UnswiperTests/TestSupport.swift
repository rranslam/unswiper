// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation
import ImageIO
import UniformTypeIdentifiers
import XCTest
@testable import Unswiper

/// Small generated fixtures: temp folders, files with fixed sizes/mtimes, JPEGs with EXIF.
enum Fixture {
    static func tempDir(_ name: String = "unswiper") throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(name)-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        // realpath keeps "/private/var/..." (resolvingSymlinksInPath strips "/private"), matching
        // the paths FileManager's directory enumerator returns.
        guard let real = realpath(url.path, nil) else { return url }
        defer { free(real) }
        return URL(fileURLWithPath: String(cString: real), isDirectory: true)
    }

    @discardableResult
    static func file(_ url: URL, bytes: Int = 16, modifiedAt: Date? = nil) throws -> URL {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(repeating: 0x5A, count: bytes).write(to: url)
        if let modifiedAt {
            try FileManager.default.setAttributes([.modificationDate: modifiedAt], ofItemAtPath: url.path)
        }
        return url
    }

    /// Writes a 4x4 JPEG with optional EXIF DateTimeOriginal and/or TIFF DateTime ("yyyy:MM:dd HH:mm:ss").
    @discardableResult
    static func jpeg(_ url: URL, exifDate: String? = nil, tiffDate: String? = nil) throws -> URL {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let width = 4, height = 4
        let space = CGColorSpaceCreateDeviceRGB()
        guard let ctx = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                                  space: space, bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else {
            throw NSError(domain: "Fixture", code: 1)
        }
        ctx.setFillColor(red: 0.2, green: 0.5, blue: 0.8, alpha: 1)
        ctx.fill(CGRect(x: 0, y: 0, width: width, height: height))
        guard let image = ctx.makeImage(),
              let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.jpeg.identifier as CFString, 1, nil) else {
            throw NSError(domain: "Fixture", code: 2)
        }
        var props: [CFString: Any] = [:]
        if let exifDate { props[kCGImagePropertyExifDictionary] = [kCGImagePropertyExifDateTimeOriginal: exifDate] }
        if let tiffDate { props[kCGImagePropertyTIFFDictionary] = [kCGImagePropertyTIFFDateTime: tiffDate] }
        CGImageDestinationAddImage(dest, image, props as CFDictionary)
        guard CGImageDestinationFinalize(dest) else { throw NSError(domain: "Fixture", code: 3) }
        return url
    }

    static func mtime(_ url: URL) throws -> Date {
        try XCTUnwrap(url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate)
    }

    /// A MediaItem for an existing file, with the size and mtime read from disk.
    static func item(at url: URL, decision: Decision = .reject, sidecars: [URL] = []) throws -> MediaItem {
        let size = Int64(try XCTUnwrap(url.resourceValues(forKeys: [.fileSizeKey]).fileSize))
        return MediaItem(
            path: url.path, kind: .photo, sizeBytes: size, takenAt: Date(timeIntervalSince1970: 1_700_000_000),
            relativeFolder: "", decision: decision, modifiedAt: try mtime(url),
            sidecars: try sidecars.map {
                SidecarFile(path: $0.path, sizeBytes: Int64(try XCTUnwrap($0.resourceValues(forKeys: [.fileSizeKey]).fileSize)), modifiedAt: try mtime($0))
            }
        )
    }

    static func localDate(_ y: Int, _ m: Int, _ d: Int, _ h: Int = 12, _ min: Int = 0, _ s: Int = 0) -> Date {
        var c = DateComponents()
        c.year = y; c.month = m; c.day = d; c.hour = h; c.minute = min; c.second = s
        return Calendar(identifier: .gregorian).date(from: c)!
    }
}

/// Base class: isolates LibraryStore in a temp support folder and cleans up.
class StoreIsolatedTestCase: XCTestCase {
    var supportDir: URL!
    var cleanup: [URL] = []

    override func setUpWithError() throws {
        supportDir = try Fixture.tempDir("support")
        LibraryStore.supportDirOverride = supportDir
        LibraryStore.saveBlockedReason = nil
        cleanup = [supportDir]
    }

    override func tearDownWithError() throws {
        LibraryStore.supportDirOverride = nil
        LibraryStore.saveBlockedReason = nil
        for url in cleanup { try? FileManager.default.removeItem(at: url) }
    }

    func makeRoot() throws -> URL {
        let root = try Fixture.tempDir("library")
        cleanup.append(root)
        return root
    }
}
