// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation
import ImageIO
import AVFoundation

enum LocalTreeIndexer {
    private static let photoExts: Set<String> = [
        "jpg", "jpeg", "png", "heic", "heif", "webp", "gif", "tif", "tiff", "bmp", "raw", "dng"
    ]
    private static let videoExts: Set<String> = [
        "mp4", "mov", "m4v", "avi", "mkv", "mts", "m2ts", "3gp", "webm"
    ]

    private static let sidecarExts: Set<String> = ["aae", "xmp"]
    /// Stills that can be the photo half of a Live Photo (paired with a same-name .mov).
    private static let livePhotoStillExts: Set<String> = ["heic", "heif", "jpg", "jpeg"]

    struct Progress {
        var filesSeen: Int
        var mediaFound: Int
    }

    static func index(
        root: URL,
        visitGapHours: Double,
        existingDecisions: [String: Decision],
        fallbackDates: [String: Date] = [:],
        onProgress: @escaping (Progress) -> Void
    ) throws -> (items: [MediaItem], groups: [VisitGroup]) {
        var items: [MediaItem] = []
        var seen = 0
        let fm = FileManager.default
        guard let enumerator = fm.enumerator(
            at: root,
            includingPropertiesForKeys: [.isRegularFileKey, .fileSizeKey, .contentModificationDateKey, .creationDateKey],
            options: [.skipsHiddenFiles, .skipsPackageDescendants]
        ) else {
            throw IndexError.unreadableRoot
        }

        struct Entry {
            let url: URL
            let ext: String
            let size: Int64
            let modifiedAt: Date?
            let createdAt: Date?
        }
        var entries: [Entry] = []

        for case let fileURL as URL in enumerator {
            seen += 1
            if seen % 50 == 0 {
                onProgress(Progress(filesSeen: seen, mediaFound: entries.count))
            }

            let values = try? fileURL.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey, .contentModificationDateKey, .creationDateKey])
            guard values?.isRegularFile == true else { continue }

            let ext = fileURL.pathExtension.lowercased()
            guard photoExts.contains(ext) || videoExts.contains(ext) || sidecarExts.contains(ext) else { continue }

            // Skip quarantine / trash-like folders
            let pathLower = fileURL.path.lowercased()
            if pathLower.contains("/_rejected/") || pathLower.contains("/.trash/") {
                continue
            }

            entries.append(Entry(
                url: fileURL,
                ext: ext,
                size: Int64(values?.fileSize ?? 0),
                modifiedAt: values?.contentModificationDate,
                createdAt: values?.creationDate
            ))
        }

        // Pair Live Photos and sidecars: same directory + base name.
        var sets: [String: [Entry]] = [:]
        for e in entries {
            let key = e.url.deletingPathExtension().path.lowercased()
            sets[key, default: []].append(e)
        }

        for (_, set) in sets {
            let photos = set.filter { photoExts.contains($0.ext) }
            let videos = set.filter { videoExts.contains($0.ext) }
            var attached = set.filter { sidecarExts.contains($0.ext) }
            var standalone = photos + videos
            if photos.contains(where: { livePhotoStillExts.contains($0.ext) }) {
                attached += videos.filter { $0.ext == "mov" }
                standalone = photos + videos.filter { $0.ext != "mov" }
            }
            // Anchor: prefer a Live Photo still, else first primary. Orphan sidecars are ignored.
            guard let anchor = standalone.first(where: { livePhotoStillExts.contains($0.ext) }) ?? standalone.first else { continue }

            for e in standalone {
                let kind: MediaKind = photoExts.contains(e.ext) ? .photo : .video
                var captured = kind == .photo ? exifDate(for: e.url) : videoDate(for: e.url)
                // Google Photos (Takeout) libraries: Google's photoTakenTime from the JSON next to
                // the file, when the file has no date of its own (plan §19.5). Keys are paths with
                // symlinks resolved (TakeoutHelper.parseDates).
                if captured == nil, !fallbackDates.isEmpty {
                    captured = fallbackDates[e.url.resolvingSymlinksInPath().path]
                }
                let folderPath = relativeFolderPath(of: e.url.deletingLastPathComponent(), root: root)
                // Undated rule (v0.1.2): capture date, else a date in the folder path. File
                // modification/creation dates are no longer used for grouping (only for commit checks).
                // M3 (plan §20.14): GPS timestamp (photos), then a date in the file name.
                let gps = kind == .photo ? gpsInfo(for: e.url) : nil
                let location = kind == .photo ? gps?.location : videoLocation(for: e.url)
                let takenAt: Date
                let source: DateSource
                if let captured {
                    takenAt = captured; source = .capture
                } else if let gpsDate = gps?.date {
                    takenAt = gpsDate; source = .gps
                } else if let named = FileNameDate.parse(fileName: e.url.lastPathComponent) {
                    takenAt = named; source = .fileName
                } else if let folderDate = FolderDate.parse(relativeFolder: folderPath) {
                    takenAt = folderDate.date; source = .folder
                } else {
                    takenAt = .distantPast; source = .unknown
                }
                let isUndated = source == .unknown
                let relFolder = folderPath.split(separator: "/").first.map(String.init) ?? root.lastPathComponent

                let path = e.url.path
                let sidecars = e.url == anchor.url
                    ? attached.map { SidecarFile(path: $0.url.path, sizeBytes: $0.size, modifiedAt: $0.modifiedAt) }
                    : []
                items.append(MediaItem(
                    path: path,
                    kind: kind,
                    sizeBytes: e.size,
                    takenAt: takenAt,
                    relativeFolder: relFolder,
                    decision: existingDecisions[path] ?? .undecided,
                    modifiedAt: e.modifiedAt,
                    sidecars: sidecars,
                    isUndated: isUndated,
                    dateSource: source,
                    folderPath: folderPath,
                    location: location
                ))
            }
        }

        onProgress(Progress(filesSeen: seen, mediaFound: items.count))
        items.sort { $0.takenAt < $1.takenAt }
        let groups = VisitGrouper.group(items: items, gapHours: visitGapHours)
        return (items, groups)
    }

    /// Parent folder of a file relative to the library root, "" for the root itself.
    /// Both sides are canonicalised so `/var` vs `/private/var` style differences don't matter.
    static func relativeFolderPath(of dir: URL, root: URL) -> String {
        let d = PathUtil.canonicalPath(dir)
        let r = PathUtil.canonicalPath(root)
        guard d != r else { return "" }
        let prefix = r.hasSuffix("/") ? r : r + "/"
        guard d.hasPrefix(prefix) else { return "" }
        return String(d.dropFirst(prefix.count)).trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    }

    /// File dates at or before 1980 (zeroed/epoch timestamps) are treated as missing.
    static func usable(_ date: Date?) -> Date? {
        guard let date, date.timeIntervalSince1970 > 315_532_800 else { return nil }
        return date
    }

    /// Capture date from QuickTime/MP4 metadata (creationDate, then com.apple.quicktime.creationdate).
    static func videoDate(for url: URL) -> Date? {
        let asset = AVURLAsset(url: url)
        let sem = DispatchSemaphore(value: 0)
        nonisolated(unsafe) var result: Date?
        Task.detached {
            defer { sem.signal() }
            if let item = try? await asset.load(.creationDate),
               let date = try? await item.load(.dateValue) {
                result = date
                return
            }
            if let meta = try? await asset.load(.metadata) {
                let items = AVMetadataItem.metadataItems(from: meta, filteredByIdentifier: .quickTimeMetadataCreationDate)
                for item in items {
                    if let date = try? await item.load(.dateValue) { result = date; return }
                    if let s = try? await item.load(.stringValue),
                       let date = ISO8601DateFormatter().date(from: s) { result = date; return }
                }
            }
        }
        sem.wait()
        return usable(result)
    }

    static func exifDate(for url: URL) -> Date? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
              let props = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any]
        else { return nil }

        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyy:MM:dd HH:mm:ss"

        if let exif = props[kCGImagePropertyExifDictionary] as? [CFString: Any],
           let raw = exif[kCGImagePropertyExifDateTimeOriginal] as? String,
           let date = formatter.date(from: raw) {
            return date
        }
        if let tiff = props[kCGImagePropertyTIFFDictionary] as? [CFString: Any],
           let raw = tiff[kCGImagePropertyTIFFDateTime] as? String,
           let date = formatter.date(from: raw) {
            return date
        }
        return nil
    }

    // MARK: - Location (plan §20.1, M3: the Mac indexer reads GPS itself)

    /// A location rounded to 5 decimals (about 1 m), or nil for `0,0`, NaN or out of range.
    static func makeLocation(lat: Double, lon: Double, source: String) -> GeoLocation? {
        guard lat.isFinite, lon.isFinite, abs(lat) <= 90, abs(lon) <= 180, !(lat == 0 && lon == 0) else { return nil }
        let r = { (v: Double) in (v * 1e5).rounded() / 1e5 }
        return GeoLocation(lat: r(lat), lon: r(lon), source: source)
    }

    /// EXIF GPS of a photo: the location and the GPS timestamp (UTC), each optional.
    static func gpsInfo(for url: URL) -> (location: GeoLocation?, date: Date?)? {
        guard let src = CGImageSourceCreateWithURL(url as CFURL, nil),
              let props = CGImageSourceCopyPropertiesAtIndex(src, 0, nil) as? [CFString: Any],
              let gps = props[kCGImagePropertyGPSDictionary] as? [CFString: Any]
        else { return nil }
        return gpsInfo(fromGPSDictionary: gps)
    }

    /// Split out for tests: reads ImageIO's GPS dictionary.
    static func gpsInfo(fromGPSDictionary gps: [CFString: Any]) -> (location: GeoLocation?, date: Date?) {
        var location: GeoLocation?
        if let lat = (gps[kCGImagePropertyGPSLatitude] as? NSNumber)?.doubleValue,
           let lon = (gps[kCGImagePropertyGPSLongitude] as? NSNumber)?.doubleValue {
            let ns = (gps[kCGImagePropertyGPSLatitudeRef] as? String)?.uppercased() == "S" ? -1.0 : 1.0
            let ew = (gps[kCGImagePropertyGPSLongitudeRef] as? String)?.uppercased() == "W" ? -1.0 : 1.0
            location = makeLocation(lat: lat * ns, lon: lon * ew, source: "exif")
        }
        var date: Date?
        if let day = gps[kCGImagePropertyGPSDateStamp] as? String,
           let time = gps[kCGImagePropertyGPSTimeStamp] as? String {
            date = gpsDate(day: day, time: time)
        }
        return (location, date)
    }

    /// `"YYYY:MM:DD"` + `"HH:MM:SS(.ss)"` in UTC (ImageIO's GPS strings).
    static func gpsDate(day: String, time: String) -> Date? {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.timeZone = TimeZone(identifier: "UTC")
        f.dateFormat = "yyyy:MM:dd HH:mm:ss"
        let t = time.split(separator: ".").first.map(String.init) ?? time
        return usable(f.date(from: "\(day.trimmingCharacters(in: .whitespacesAndNewlines)) \(t)"))
    }

    /// QuickTime/MP4 location (ISO 6709, e.g. `+37.5000-122.2500+010.000/`).
    static func videoLocation(for url: URL) -> GeoLocation? {
        let asset = AVURLAsset(url: url)
        let sem = DispatchSemaphore(value: 0)
        nonisolated(unsafe) var result: String?
        Task.detached {
            defer { sem.signal() }
            guard let meta = try? await asset.load(.metadata) else { return }
            let ids: [AVMetadataIdentifier] = [.quickTimeMetadataLocationISO6709, .commonIdentifierLocation]
            for id in ids {
                for item in AVMetadataItem.metadataItems(from: meta, filteredByIdentifier: id) {
                    if let s = try? await item.load(.stringValue) { result = s; return }
                }
            }
        }
        sem.wait()
        return result.flatMap(parseISO6709)
    }

    /// `±DD.DDDD±DDD.DDDD[±AAA.AAA]/` → location (source "quicktime").
    static func parseISO6709(_ s: String) -> GeoLocation? {
        var parts: [Double] = []
        var cur = ""
        for ch in s {
            if ch == "+" || ch == "-" || ch == "/" {
                if let v = Double(cur) { parts.append(v) }
                cur = ch == "/" ? "" : String(ch)
                if ch == "/" { break }
            } else {
                cur.append(ch)
            }
        }
        if let v = Double(cur) { parts.append(v) }
        guard parts.count >= 2 else { return nil }
        return makeLocation(lat: parts[0], lon: parts[1], source: "quicktime")
    }

    enum IndexError: LocalizedError {
        case unreadableRoot
        var errorDescription: String? { "Could not read the selected folder." }
    }
}

enum VisitGrouper {
    /// Groups items into visits:
    /// 1. Items with a capture or folder date are clustered by time gap (oldest visit first).
    /// 2. Items without a date in a sub-folder join the visit holding most of the dated files
    ///    in the same folder; if that folder has no dated files they form a visit named after
    ///    the folder (sorted by folder name).
    /// 3. Only undated items directly in the library root (or from pre-0.1.2 indexes) go to "Undated".
    static func group(items: [MediaItem], gapHours: Double) -> [VisitGroup] {
        let undatedAll = items.filter(\.isUndated)
        let dated = items.filter { !$0.isUndated }.sorted { $0.takenAt < $1.takenAt }
        var clusters = cluster(items: dated, gapHours: gapHours)

        // Dated items per folder -> cluster index counts.
        var clusterOf: [String: Int] = [:]
        for (i, c) in clusters.enumerated() { for item in c { clusterOf[item.path] = i } }
        var datedByFolder: [String: [Int: Int]] = [:]
        for item in dated {
            guard let folder = item.folderPath, let ci = clusterOf[item.path] else { continue }
            datedByFolder[folder, default: [:]][ci, default: 0] += 1
        }

        var rootUndated: [MediaItem] = []
        var namedFolders: [String: [MediaItem]] = [:]
        var joined: [Int: [MediaItem]] = [:]
        for item in undatedAll {
            guard let folder = item.folderPath, !folder.isEmpty else { rootUndated.append(item); continue }
            if let counts = datedByFolder[folder],
               let best = counts.max(by: { $0.value != $1.value ? $0.value < $1.value : $0.key > $1.key }) {
                joined[best.key, default: []].append(item)
            } else {
                namedFolders[folder, default: []].append(item)
            }
        }
        for (ci, extra) in joined { clusters[ci].append(contentsOf: extra) }

        var result = clusters.map(makeVisit).sorted {
            // Visit order: oldest first (Rob, 26 Oct 2026; was largest first). Same as Rust.
            $0.startDate != $1.startDate ? $0.startDate < $1.startDate : $0.id < $1.id
        }

        for folder in namedFolders.keys.sorted(by: { $0.localizedStandardCompare($1) == .orderedAscending }) {
            let members = namedFolders[folder]!
            let name = folder.split(separator: "/").last.map(String.init) ?? folder
            result.append(VisitGroup(
                id: "folder-" + fnv1a(folder),
                title: name,
                startDate: .distantPast,
                endDate: .distantPast,
                itemPaths: members.map(\.path),
                photoCount: members.filter { $0.kind == .photo }.count,
                videoCount: members.filter { $0.kind == .video }.count,
                totalBytes: members.reduce(Int64(0)) { $0 + $1.totalBytes },
                folderHint: folder
            ))
        }

        if !rootUndated.isEmpty {
            result.append(VisitGroup(
                id: "undated",
                title: "Undated",
                startDate: .distantPast,
                endDate: .distantPast,
                itemPaths: rootUndated.map(\.path),
                photoCount: rootUndated.filter { $0.kind == .photo }.count,
                videoCount: rootUndated.filter { $0.kind == .video }.count,
                totalBytes: rootUndated.reduce(Int64(0)) { $0 + $1.totalBytes },
                folderHint: mostCommonFolder(rootUndated)
            ))
        }
        return result
    }

    /// Splits time-sorted items wherever the gap exceeds `gapHours`.
    static func cluster(items: [MediaItem], gapHours: Double) -> [[MediaItem]] {
        guard !items.isEmpty else { return [] }
        let gap = gapHours * 3600
        var clusters: [[MediaItem]] = []
        var current: [MediaItem] = [items[0]]

        for item in items.dropFirst() {
            if let last = current.last, item.takenAt.timeIntervalSince(last.takenAt) > gap {
                clusters.append(current)
                current = [item]
            } else {
                current.append(item)
            }
        }
        clusters.append(current)
        return clusters
    }

    /// Builds a visit from a cluster. The first element(s) are the dated items in time order;
    /// undated folder-siblings appended at the end don't affect dates or the ID.
    private static func makeVisit(_ cluster: [MediaItem]) -> VisitGroup {
        let datedMembers = cluster.filter { !$0.isUndated }
        let start = datedMembers.first!.takenAt
        let end = datedMembers.last!.takenAt
        let folder = mostCommonFolder(cluster)
        let title = titleFor(folder: folder, start: start, end: end)
        return VisitGroup(
            id: stableGroupID(firstPath: datedMembers.first!.path, start: start),
            title: title,
            startDate: start,
            endDate: end,
            itemPaths: cluster.map(\.path),
            photoCount: cluster.filter { $0.kind == .photo }.count,
            videoCount: cluster.filter { $0.kind == .video }.count,
            totalBytes: cluster.reduce(Int64(0)) { $0 + $1.totalBytes },
            folderHint: folder
        )
    }

    static func fnv1a(_ s: String) -> String {
        var hash: UInt64 = 0xcbf29ce484222325
        for byte in s.utf8 {
            hash ^= UInt64(byte)
            hash = hash &* 0x100000001b3
        }
        return String(hash, radix: 16)
    }

    /// Stable across commits/re-index: FNV-1a of first item path + start time (no item count).
    static func stableGroupID(firstPath: String, start: Date) -> String {
        "visit-" + fnv1a("\(firstPath)|\(Int64(start.timeIntervalSince1970))")
    }

    private static func mostCommonFolder(_ items: [MediaItem]) -> String {
        var counts: [String: Int] = [:]
        for item in items {
            counts[item.relativeFolder, default: 0] += 1
        }
        return counts.max(by: { $0.value < $1.value })?.key ?? "Library"
    }

    private static func titleFor(folder: String, start: Date, end: Date) -> String {
        let df = DateFormatter()
        df.dateFormat = "MMM d"
        let startS = df.string(from: start)
        if Calendar.current.isDate(start, inSameDayAs: end) {
            return "\(startS) · \(folder)"
        }
        df.dateFormat = "MMM d"
        let endS = df.string(from: end)
        return "\(startS)–\(endS) · \(folder)"
    }
}
