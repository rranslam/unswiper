// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// The sharpness score below which a photo counts as blurry, per sensitivity. Stored in
/// library.json as `config.blurCutoffs` (shared with Linux; missing = the defaults). The score
/// is the helper's Laplacian-variance measure: higher = sharper. Low flags the fewest photos,
/// so its cutoff is the smallest: low < normal < high.
struct BlurCutoffs: Codable, Equatable {
    var low: Double
    var normal: Double
    var high: Double

    /// Calibrated on 85 real photos (docs/blur-evaluation.md, 15 Oct 2026).
    static let `default` = BlurCutoffs(low: 7, normal: 13, high: 25)
    static let max: Double = 1000

    init(low: Double, normal: Double, high: Double) {
        self.low = low
        self.normal = normal
        self.high = high
    }

    /// Lenient: a missing value takes its default, and anything unreadable gives the defaults,
    /// so a hand-edited library.json still opens.
    init(from decoder: Decoder) throws {
        let d = BlurCutoffs.default
        guard let c = try? decoder.container(keyedBy: CodingKeys.self) else {
            self = d
            return
        }
        low = (try? c.decodeIfPresent(Double.self, forKey: .low)) ?? d.low
        normal = (try? c.decodeIfPresent(Double.self, forKey: .normal)) ?? d.normal
        high = (try? c.decodeIfPresent(Double.self, forKey: .high)) ?? d.high
    }

    func value(_ s: BlurRules.Sensitivity) -> Double {
        switch s {
        case .low: return low
        case .normal: return normal
        case .high: return high
        }
    }

    /// Why these values can't be used (same rules and words as Linux), or nil.
    var problem: String? {
        for (name, v) in [("Low", low), ("Normal", normal), ("High", high)] {
            if !v.isFinite || v <= 0 { return "\(name) must be a number above 0." }
            if v > BlurCutoffs.max { return "\(name) must be at most \(Int(BlurCutoffs.max))." }
        }
        if !(low < normal && normal < high) {
            return "The values must go up: Low below Normal, Normal below High (Low flags the fewest photos)."
        }
        return nil
    }

    /// Rounded to 0.1, as saved.
    var rounded: BlurCutoffs {
        let r = { (x: Double) in (x * 10).rounded() / 10 }
        return BlurCutoffs(low: r(low), normal: r(normal), high: r(high))
    }
}

extension BlurRules {
    static func cutoffs(_ c: AppConfig) -> BlurCutoffs {
        guard let v = c.blurCutoffs, v.problem == nil else { return .default }
        return v
    }

    /// Saves valid cutoffs (the defaults as nil); returns the problem otherwise.
    @discardableResult
    static func setCutoffs(_ c: inout AppConfig, _ v: BlurCutoffs) -> String? {
        let v = v.rounded
        if let p = v.problem { return p }
        c.blurCutoffs = v == .default ? nil : v
        return nil
    }

    /// The large viewer's flag and score texts: a suggested photo shows `Blurry? · 42.7` in
    /// its flag; any other scored photo shows `Sharpness 42.7` next to it (same as Linux).
    static func viewerTexts(flag: String, suggested: Bool, score: Double?) -> (flag: String, score: String) {
        guard let score else { return (flag, "") }
        let s = String(format: "%.1f", score)
        return suggested ? ("\(flag) · \(s)", "") : (flag, "Sharpness \(s)")
    }
}

/// Scores from the helper's cache (`quality-cache.json` next to library.json), read-only.
struct BlurScores {
    struct Entry: Decodable, Equatable {
        let sharpness: Double
        let confidence: Double
        let fileSize: UInt64
        let mtime: Int64
    }

    private struct File: Decodable {
        let v: Int
        let algo: Int
        let entries: [String: Entry]
    }

    /// The helper's score algorithm version (`ALGO_VERSION`); other versions are ignored.
    static let algoVersion = 1
    /// Below this the photo is too dark or plain to judge and is never flagged.
    static let minConfidence = 0.5
    static let cameraExts: Set<String> = ["jpg", "jpeg", "heic", "heif", "tif", "tiff", "webp"]

    var entries: [String: Entry] = [:]

    static var url: URL { LibraryStore.supportDir.appendingPathComponent("quality-cache.json") }

    static func load(_ url: URL = BlurScores.url) -> BlurScores {
        guard let data = try? Data(contentsOf: url),
              let f = try? JSONDecoder().decode(File.self, from: data),
              f.v == 1, f.algo == algoVersion else { return BlurScores() }
        return BlurScores(entries: f.entries)
    }

    /// The score for the large viewer (higher = sharper): nil for videos, screenshots, photos
    /// not checked yet or changed since, and photos too dark or plain to judge.
    func score(for item: MediaItem) -> Double? {
        guard item.kind == .photo,
              BlurScores.cameraExts.contains((item.path as NSString).pathExtension.lowercased()),
              let e = entries[item.path], e.confidence >= BlurScores.minConfidence,
              let attrs = try? FileManager.default.attributesOfItem(atPath: item.path),
              let size = (attrs[.size] as? NSNumber)?.uint64Value,
              let mtime = attrs[.modificationDate] as? Date,
              size == e.fileSize, Int64(mtime.timeIntervalSince1970.rounded(.down)) == e.mtime
        else { return nil }
        return e.sharpness
    }
}
