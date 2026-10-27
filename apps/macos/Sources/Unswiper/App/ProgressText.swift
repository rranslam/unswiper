// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// Progress bar values and text for indexing and the blur check (same wording as Linux).
enum ProgressText {
    /// `1,234` with the user's grouping separator.
    static func count(_ n: Int) -> String {
        n.formatted(.number)
    }

    /// `less than a minute left`, `about 8 min left`, `about 2 h 5 min left`.
    static func timeLeft(_ seconds: Double) -> String {
        let m = Int((seconds + 30) / 60)
        switch m {
        case ..<1: return "less than a minute left"
        case 1..<60: return "about \(m) min left"
        default: return "about \(m / 60) h \(m % 60) min left"
        }
    }

    /// Time left from the rate so far; nil until there is enough to go on (2 s, 3 items).
    static func eta(done: Int, total: Int, elapsed: TimeInterval) -> Double? {
        guard done >= 3, elapsed >= 2, total > done else { return nil }
        return elapsed / Double(done) * Double(total - done)
    }

    /// Indexing: (fraction or nil while still finding files, text).
    static func index(_ p: LocalTreeIndexer.Progress, elapsedDating: TimeInterval) -> (Double?, String) {
        if p.toDate == 0 {
            return (nil, "Looking for photos and videos · \(count(p.filesSeen)) files checked · \(count(p.mediaFound)) found")
        }
        var t = "Reading dates and places · \(count(p.dated)) of \(count(p.toDate))"
        if let e = eta(done: p.dated, total: p.toDate, elapsed: elapsedDating) {
            t += " · " + timeLeft(e)
        }
        return (min(1, Double(p.dated) / Double(max(p.toDate, 1))), t)
    }

    /// Blur check: (fraction, text). The time left comes from the rate since `sinceDone`
    /// photos were done `elapsed` seconds ago (reset after a pause).
    static func blur(done: Int, total: Int, sinceDone: Int = 0, elapsed: TimeInterval,
                     paused: Bool, label: String = "") -> (Double, String) {
        let frac = min(1, Double(done) / Double(max(total, 1)))
        if paused { return (frac, "Paused · \(count(done)) of \(count(total)) checked") }
        var t = "Checking sharpness\(label) \(count(done)) of \(count(total))"
        if let e = eta(done: done - sinceDone, total: total - sinceDone, elapsed: elapsed) {
            t += " · " + timeLeft(e)
        }
        return (frac, t)
    }
}

/// Lets through at most ~10 progress updates a second (and always the last one).
final class ProgressThrottle: @unchecked Sendable {
    private let lock = NSLock()
    private var last = Date.distantPast
    private let interval: TimeInterval

    init(interval: TimeInterval = 0.1) { self.interval = interval }

    func ready(final: Bool = false, now: Date = Date()) -> Bool {
        lock.lock(); defer { lock.unlock() }
        guard final || now.timeIntervalSince(last) >= interval else { return false }
        last = now
        return true
    }
}
