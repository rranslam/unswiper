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
                let captured = kind == .photo ? exifDate(for: e.url) : videoDate(for: e.url)
                let fileDate = usable(e.modifiedAt) ?? usable(e.createdAt)
                let takenAt = captured ?? fileDate ?? .distantPast
                let isUndated = captured == nil && fileDate == nil

                let relFolder: String
                if let rel = e.url.deletingLastPathComponent().path.replacingOccurrences(of: root.path, with: "")
                    .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
                    .split(separator: "/")
                    .first {
                    relFolder = String(rel)
                } else {
                    relFolder = root.lastPathComponent
                }

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
                    isUndated: isUndated
                ))
            }
        }

        onProgress(Progress(filesSeen: seen, mediaFound: items.count))
        items.sort { $0.takenAt < $1.takenAt }
        let groups = VisitGrouper.group(items: items, gapHours: visitGapHours)
        return (items, groups)
    }

    /// File dates at or before 1980 (zeroed/epoch timestamps) are treated as missing.
    private static func usable(_ date: Date?) -> Date? {
        guard let date, date.timeIntervalSince1970 > 315_532_800 else { return nil }
        return date
    }

    /// Capture date from QuickTime/MP4 metadata (creationDate, then com.apple.quicktime.creationdate).
    private static func videoDate(for url: URL) -> Date? {
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

    private static func exifDate(for url: URL) -> Date? {
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

    enum IndexError: LocalizedError {
        case unreadableRoot
        var errorDescription: String? { "Could not read the selected folder." }
    }
}

enum VisitGrouper {
    static func group(items: [MediaItem], gapHours: Double) -> [VisitGroup] {
        let undated = items.filter(\.isUndated)
        let items = items.filter { !$0.isUndated }.sorted { $0.takenAt < $1.takenAt }
        var result = cluster(items: items, gapHours: gapHours)
        if !undated.isEmpty {
            result.append(VisitGroup(
                id: "undated",
                title: "Undated",
                startDate: .distantPast,
                endDate: .distantPast,
                itemPaths: undated.map(\.path),
                photoCount: undated.filter { $0.kind == .photo }.count,
                videoCount: undated.filter { $0.kind == .video }.count,
                totalBytes: undated.reduce(Int64(0)) { $0 + $1.totalBytes },
                folderHint: mostCommonFolder(undated)
            ))
        }
        return result
    }

    private static func cluster(items: [MediaItem], gapHours: Double) -> [VisitGroup] {
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

        return clusters.map { cluster in
            let start = cluster.first!.takenAt
            let end = cluster.last!.takenAt
            let folder = mostCommonFolder(cluster)
            let title = titleFor(folder: folder, start: start, end: end)
            let photos = cluster.filter { $0.kind == .photo }.count
            let videos = cluster.filter { $0.kind == .video }.count
            let bytes = cluster.reduce(Int64(0)) { $0 + $1.totalBytes }
            let id = stableGroupID(firstPath: cluster.first!.path, start: start)
            return VisitGroup(
                id: id,
                title: title,
                startDate: start,
                endDate: end,
                itemPaths: cluster.map(\.path),
                photoCount: photos,
                videoCount: videos,
                totalBytes: bytes,
                folderHint: folder
            )
        }
        .sorted { $0.totalBytes > $1.totalBytes }
    }

    /// Stable across commits/re-index: FNV-1a of first item path + start time (no item count).
    static func stableGroupID(firstPath: String, start: Date) -> String {
        var hash: UInt64 = 0xcbf29ce484222325
        for byte in "\(firstPath)|\(Int64(start.timeIntervalSince1970))".utf8 {
            hash ^= UInt64(byte)
            hash = hash &* 0x100000001b3
        }
        return "visit-" + String(hash, radix: 16)
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
