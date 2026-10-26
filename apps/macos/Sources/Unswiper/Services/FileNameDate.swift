// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// Dates from camera file names (plan §20.14, M3). Same rules as Rust
/// `unswiper_core::meta::file_name_date`: the fallback after the capture date and the GPS
/// timestamp, before the folder name.
///
/// - `YYYYMMDD` then `_`/`-` and `HHMMSS` (`IMG_20240612_133241`, `PXL_20240612_133241123`);
/// - a 14+ digit run `YYYYMMDDHHMMSS…` (`…BURST20240612133241123_COVER`);
/// - `YYYY-MM-DD` with an optional `[ _-]HH[-.:]MM[-.:]SS`.
/// Years 1990–2099; the date must exist. No time = local noon. `PXL_` names are UTC.
enum FileNameDate {
    struct Parts: Equatable {
        var year, month, day, hour, minute, second: Int
    }

    private static func isDigit(_ c: UInt8) -> Bool { c >= 48 && c <= 57 }

    private static func digits(_ b: [UInt8], _ i: Int) -> Int {
        var n = 0
        while i + n < b.count, isDigit(b[i + n]) { n += 1 }
        return n
    }

    private static func num(_ b: [UInt8], _ i: Int, _ n: Int) -> Int? {
        guard i >= 0, i + n <= b.count else { return nil }
        var v = 0
        for k in i..<(i + n) {
            guard isDigit(b[k]) else { return nil }
            v = v * 10 + Int(b[k] - 48)
        }
        return v
    }

    private static func validDate(_ y: Int, _ m: Int, _ d: Int) -> Bool {
        guard (1990...2099).contains(y), (1...12).contains(m), d >= 1 else { return false }
        var c = DateComponents()
        c.year = y; c.month = m; c.day = 1
        let cal = Calendar(identifier: .gregorian)
        guard let first = cal.date(from: c), let range = cal.range(of: .day, in: .month, for: first) else { return false }
        return range.contains(d)
    }

    private static func validTime(_ h: Int?, _ m: Int?, _ s: Int?) -> (Int, Int, Int)? {
        guard let h, let m, let s, h < 24, m < 60, s < 60 else { return nil }
        return (h, m, s)
    }

    /// The date in a file name without its extension, as wall-clock parts.
    static func parseParts(stem: String) -> Parts? {
        let b = Array(stem.utf8)
        var i = 0
        while i < b.count {
            if !isDigit(b[i]) || (i > 0 && isDigit(b[i - 1])) { i += 1; continue }
            let run = digits(b, i)
            if run == 8 || run >= 14, let y = num(b, i, 4), let mo = num(b, i + 4, 2), let d = num(b, i + 6, 2),
               validDate(y, mo, d) {
                var t: (Int, Int, Int)?
                if run >= 14 {
                    t = validTime(num(b, i + 8, 2), num(b, i + 10, 2), num(b, i + 12, 2))
                } else {
                    let j = i + 8
                    if j < b.count, b[j] == UInt8(ascii: "_") || b[j] == UInt8(ascii: "-"), digits(b, j + 1) >= 6 {
                        t = validTime(num(b, j + 1, 2), num(b, j + 3, 2), num(b, j + 5, 2))
                    }
                }
                let (h, m, s) = t ?? (12, 0, 0)
                return Parts(year: y, month: mo, day: d, hour: h, minute: m, second: s)
            }
            if run == 4, let p = dashed(b, i) { return p }
            i += run
        }
        return nil
    }

    private static func dashed(_ b: [UInt8], _ i: Int) -> Parts? {
        func sep(_ k: Int) -> Bool { k < b.count && "-_.".utf8.contains(b[k]) }
        func tsep(_ k: Int) -> Bool { k < b.count && "-_.: ".utf8.contains(b[k]) }
        guard sep(i + 4), digits(b, i + 5) == 2, sep(i + 7), digits(b, i + 8) == 2,
              let y = num(b, i, 4), let m = num(b, i + 5, 2), let d = num(b, i + 8, 2), validDate(y, m, d)
        else { return nil }
        func two(_ k: Int) -> Int? { digits(b, k) == 2 ? num(b, k, 2) : nil }
        var t: (Int, Int, Int)?
        if tsep(i + 10), tsep(i + 13), tsep(i + 16) {
            t = validTime(two(i + 11), two(i + 14), two(i + 17))
        }
        let (h, mi, s) = t ?? (12, 0, 0)
        return Parts(year: y, month: m, day: d, hour: h, minute: mi, second: s)
    }

    /// The capture time a camera wrote into `fileName`, or nil.
    static func parse(fileName: String, timeZone: TimeZone = .current) -> Date? {
        let stem = fileName.split(separator: ".", maxSplits: 1, omittingEmptySubsequences: false).first.map(String.init) ?? fileName
        guard let p = parseParts(stem: stem) else { return nil }
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = stem.hasPrefix("PXL_") ? TimeZone(identifier: "UTC")! : timeZone
        var c = DateComponents()
        c.year = p.year; c.month = p.month; c.day = p.day
        c.hour = p.hour; c.minute = p.minute; c.second = p.second
        return cal.date(from: c)
    }
}
