// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// Parses a date from folder names, per the Undated rule (docs/linux-port-plan.md §7).
///
/// Recognised, scanning from the deepest folder upward:
/// - Consecutive components `YYYY/MM/DD`, `YYYY/MM`, `YYYY` (month/day may be 1 or 2 digits).
/// - One component `YYYY-MM-DD`, `YYYY_MM_DD`, `YYYY.MM.DD`, `YYYYMMDD` or `YYYY-MM`,
///   optionally followed by a space and a name (`2025-07-04 Beach`, `1999 Photos`).
/// Years must be 1980–2099 and dates must exist. Only whole components or a leading token count,
/// so `IMG_2026…`, `v2026`, `Trip 2026-01-01`, `2026-13-40` and `0000` are not dates.
enum FolderDate {
    enum Precision: String, Codable, Hashable { case day, month, year }

    struct Match: Equatable {
        var year: Int
        var month: Int
        var day: Int
        var precision: Precision

        /// Noon local time on the first day of the span.
        var date: Date {
            var c = DateComponents()
            c.year = year; c.month = month; c.day = day; c.hour = 12
            return Calendar(identifier: .gregorian).date(from: c)!
        }
    }

    /// `components` are the folder names from the library root down to the file's folder.
    static func parse(components: [String]) -> Match? {
        guard !components.isEmpty else { return nil }
        var k = components.count - 1
        while k >= 0 {
            if k >= 2, let y = year(components[k - 2]), let m = month(components[k - 1]),
               let d = number(components[k], digits: 1...2), valid(y, m, d) {
                return Match(year: y, month: m, day: d, precision: .day)
            }
            if k >= 1, let y = year(components[k - 1]), let m = month(components[k]) {
                return Match(year: y, month: m, day: 1, precision: .month)
            }
            if let match = single(components[k]) { return match }
            k -= 1
        }
        return nil
    }

    /// Convenience: parse a relative folder path such as `2026/01/01` or `Trips/2025-07-04 Beach`.
    static func parse(relativeFolder: String) -> Match? {
        parse(components: relativeFolder.split(separator: "/").map(String.init))
    }

    /// One folder name: whole component, or its leading whitespace-separated token.
    static func single(_ component: String) -> Match? {
        let trimmed = component.trimmingCharacters(in: .whitespaces)
        guard let token = trimmed.split(whereSeparator: { $0 == " " || $0 == "\t" }).first.map(String.init) else { return nil }

        if let y = year(token) { return Match(year: y, month: 1, day: 1, precision: .year) }

        let chars = Array(token)
        // YYYYMMDD
        if chars.count == 8, chars.allSatisfy(\.isASCIIDigit) {
            let y = Int(String(chars[0..<4]))!, m = Int(String(chars[4..<6]))!, d = Int(String(chars[6..<8]))!
            return valid(y, m, d) ? Match(year: y, month: m, day: d, precision: .day) : nil
        }
        // YYYY<sep>MM<sep>DD with one consistent separator, or YYYY-MM
        for sep in ["-", "_", "."] {
            let parts = token.components(separatedBy: sep)
            if parts.count == 3, let y = year(parts[0]), let m = month(parts[1]),
               let d = number(parts[2], digits: 1...2), valid(y, m, d) {
                return Match(year: y, month: m, day: d, precision: .day)
            }
            if sep == "-", parts.count == 2, let y = year(parts[0]), let m = month(parts[1]) {
                return Match(year: y, month: m, day: 1, precision: .month)
            }
        }
        return nil
    }

    private static func number(_ s: String, digits: ClosedRange<Int>) -> Int? {
        guard digits.contains(s.count), s.allSatisfy(\.isASCIIDigit) else { return nil }
        return Int(s)
    }

    private static func year(_ s: String) -> Int? {
        guard let y = number(s, digits: 4...4), (1980...2099).contains(y) else { return nil }
        return y
    }

    private static func month(_ s: String) -> Int? {
        guard let m = number(s, digits: 1...2), (1...12).contains(m) else { return nil }
        return m
    }

    static func valid(_ y: Int, _ m: Int, _ d: Int) -> Bool {
        guard (1980...2099).contains(y), (1...12).contains(m), d >= 1 else { return false }
        let days = [31, (y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)) ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        return d <= days[m - 1]
    }
}

private extension Character {
    var isASCIIDigit: Bool { isASCII && isNumber }
}
