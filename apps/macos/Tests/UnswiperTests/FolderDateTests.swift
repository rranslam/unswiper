// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import XCTest
@testable import Unswiper

/// Parser table for the folder-date part of the Undated rule (docs/linux-port-plan.md §7).
final class FolderDateTests: XCTestCase {
    typealias P = FolderDate.Precision

    /// (relative folder, expected (y, m, d, precision) or nil)
    static let table: [(String, (Int, Int, Int, P)?)] = [
        // Consecutive components
        ("2026/01/01", (2026, 1, 1, .day)),
        ("2026/1/5", (2026, 1, 5, .day)),
        ("2026/01", (2026, 1, 1, .month)),
        ("2026/12", (2026, 12, 1, .month)),
        ("2026", (2026, 1, 1, .year)),
        ("1999", (1999, 1, 1, .year)),
        ("Photos/2026/03/14", (2026, 3, 14, .day)),
        ("2026/01/01/extra", (2026, 1, 1, .day)),
        ("2026/02/30", (2026, 2, 1, .month)),          // invalid day -> falls back to month
        ("2026/13", (2026, 1, 1, .year)),              // invalid month -> year
        ("2026/13/01", (2026, 1, 1, .year)),
        ("2024/02/29", (2024, 2, 29, .day)),           // leap year
        ("2023/02/29", (2023, 2, 1, .month)),          // not a leap year
        ("2000/02/29", (2000, 2, 29, .day)),
        ("2026/04/31", (2026, 4, 1, .month)),
        ("1980/01/01", (1980, 1, 1, .day)),
        ("2099/12/31", (2099, 12, 31, .day)),
        ("1979/05/05", nil),
        ("2100/01/01", nil),
        // Single component
        ("2026-01-01", (2026, 1, 1, .day)),
        ("2026_01_01", (2026, 1, 1, .day)),
        ("2026.01.01", (2026, 1, 1, .day)),
        ("20260101", (2026, 1, 1, .day)),
        ("2026-01", (2026, 1, 1, .month)),
        ("2026-1-5", (2026, 1, 5, .day)),
        ("2025-07-04 Beach", (2025, 7, 4, .day)),
        ("2026-01-01 Lisbon", (2026, 1, 1, .day)),
        ("20260101 Party", (2026, 1, 1, .day)),
        ("1999 Photos", (1999, 1, 1, .year)),
        ("2026-01 Ski trip", (2026, 1, 1, .month)),
        ("Trips/2025-07-04 Beach", (2025, 7, 4, .day)),
        ("2025-07-04 Beach/raw", (2025, 7, 4, .day)),
        ("2024/2025-07-04 Beach", (2025, 7, 4, .day)),  // deepest wins
        ("2024-02-29", (2024, 2, 29, .day)),
        ("2026-02-29", nil),
        ("2026-13-40", nil),
        ("2026-00-10", nil),
        ("2026-01-00", nil),
        ("20261301", nil),
        ("2026-01_01", nil),                            // mixed separators
        ("2026-01-01-Beach", nil),                      // name must follow a space
        ("2026/01/01 x", (2026, 1, 1, .month)),         // "01 x" is not a day component -> month
        // Not dates
        ("IMG_2026", nil),
        ("v2026", nil),
        ("12345", nil),
        ("0000", nil),
        ("Trip 2026-01-01", nil),
        ("Lisbon", nil),
        ("Beach 2025", nil),
        ("202601", nil),
        ("2026010", nil),
        ("26-01-01", nil),
        ("Camera Uploads", nil),
        ("Fotos de verão 2024", nil),
        ("", nil),
        ("１９９９", nil),                              // full-width digits are not ASCII
        ("2026-01-01T10", nil),
        ("DCIM/100APPLE", nil),
        ("Lisboa – 2024", nil),
        ("2024 – Lisboa", (2024, 1, 1, .year)),
    ]

    func testParserTable() {
        XCTAssertGreaterThanOrEqual(Self.table.count, 60)
        for (input, expected) in Self.table {
            let got = FolderDate.parse(relativeFolder: input)
            if let (y, m, d, p) = expected {
                XCTAssertEqual(got, FolderDate.Match(year: y, month: m, day: d, precision: p), "\(input)")
            } else {
                XCTAssertNil(got, "\(input) should not parse, got \(String(describing: got))")
            }
        }
    }

    func testPlacedAtNoonOnFirstDay() {
        let m = FolderDate.parse(relativeFolder: "2026-03")!
        XCTAssertEqual(m.date, Fixture.localDate(2026, 3, 1, 12))
    }

    func testRoundTripEveryFormat() {
        // Property-style: every valid date in each supported format parses back to itself.
        let cal = Calendar(identifier: .gregorian)
        var day = Fixture.localDate(1980, 1, 1)
        let end = Fixture.localDate(2099, 12, 31)
        var checked = 0
        while day <= end {
            let c = cal.dateComponents([.year, .month, .day], from: day)
            let y = c.year!, m = c.month!, d = c.day!
            let expected = FolderDate.Match(year: y, month: m, day: d, precision: .day)
            for s in [String(format: "%04d-%02d-%02d", y, m, d), String(format: "%04d_%02d_%02d", y, m, d),
                      String(format: "%04d.%02d.%02d", y, m, d), String(format: "%04d%02d%02d", y, m, d),
                      String(format: "%04d/%02d/%02d", y, m, d), String(format: "%04d/%d/%d", y, m, d)] {
                XCTAssertEqual(FolderDate.parse(relativeFolder: s), expected, s)
            }
            checked += 1
            day = cal.date(byAdding: .day, value: 37, to: day)!   // sample ~1,200 dates
        }
        XCTAssertGreaterThan(checked, 1000)
    }
}
