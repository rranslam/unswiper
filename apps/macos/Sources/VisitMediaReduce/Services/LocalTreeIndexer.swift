import Foundation
import ImageIO

enum LocalTreeIndexer {
    private static let photoExts: Set<String> = [
        "jpg", "jpeg", "png", "heic", "heif", "webp", "gif", "tif", "tiff", "bmp", "raw", "dng"
    ]
    private static let videoExts: Set<String> = [
        "mp4", "mov", "m4v", "avi", "mkv", "mts", "m2ts", "3gp", "webm"
    ]

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

        for case let fileURL as URL in enumerator {
            seen += 1
            if seen % 50 == 0 {
                onProgress(Progress(filesSeen: seen, mediaFound: items.count))
            }

            let values = try? fileURL.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey, .contentModificationDateKey, .creationDateKey])
            guard values?.isRegularFile == true else { continue }

            let ext = fileURL.pathExtension.lowercased()
            let kind: MediaKind
            if photoExts.contains(ext) {
                kind = .photo
            } else if videoExts.contains(ext) {
                kind = .video
            } else {
                continue
            }

            // Skip quarantine / trash-like folders
            let pathLower = fileURL.path.lowercased()
            if pathLower.contains("/_rejected/") || pathLower.contains("/.trash/") {
                continue
            }

            let size = Int64(values?.fileSize ?? 0)
            let takenAt = exifDate(for: fileURL)
                ?? values?.contentModificationDate
                ?? values?.creationDate
                ?? Date()

            let relFolder: String
            if let rel = fileURL.deletingLastPathComponent().path.replacingOccurrences(of: root.path, with: "")
                .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
                .split(separator: "/")
                .first {
                relFolder = String(rel)
            } else {
                relFolder = root.lastPathComponent
            }

            let path = fileURL.path
            let decision = existingDecisions[path] ?? .undecided
            items.append(MediaItem(
                path: path,
                kind: kind,
                sizeBytes: size,
                takenAt: takenAt,
                relativeFolder: relFolder,
                decision: decision
            ))
        }

        onProgress(Progress(filesSeen: seen, mediaFound: items.count))
        items.sort { $0.takenAt < $1.takenAt }
        let groups = VisitGrouper.group(items: items, gapHours: visitGapHours)
        return (items, groups)
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
            let bytes = cluster.reduce(Int64(0)) { $0 + $1.sizeBytes }
            let id = "\(start.timeIntervalSince1970)-\(cluster.count)-\(folder)"
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
