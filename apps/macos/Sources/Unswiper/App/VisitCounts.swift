// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// Kept / rejected / left per visit, counted in one pass over the library and then updated
/// for each decision change. Before, every visit row counted over all items on every redraw,
/// which made large libraries (78k photos, thousands of visits) slow.
struct VisitCounts: Equatable {
    struct Count: Equatable {
        var kept = 0
        var rejected = 0
        var left = 0

        mutating func add(_ d: Decision, _ n: Int) {
            switch d {
            case .keep: kept += n
            case .reject: rejected += n
            case .undecided: left += n
            }
        }

        var tuple: (kept: Int, rejected: Int, left: Int) { (kept, rejected, left) }
    }

    private(set) var byGroup: [String: Count] = [:]
    /// Visits each path belongs to (a path can be in more than one in some grouping modes).
    private var groupsOfPath: [String: [String]] = [:]

    init() {}

    init(items: [MediaItem], groups: [VisitGroup]) {
        var decision: [String: Decision] = [:]
        decision.reserveCapacity(items.count)
        for i in items where decision[i.path] == nil {
            decision[i.path] = i.decision
        }
        for g in groups {
            var c = Count()
            for p in g.itemPaths {
                guard let d = decision[p] else { continue }
                c.add(d, 1)
                groupsOfPath[p, default: []].append(g.id)
            }
            byGroup[g.id] = c
        }
    }

    /// Counts for a visit (zero for an unknown one).
    subscript(id: String) -> Count { byGroup[id] ?? Count() }

    /// One item's decision changed.
    mutating func update(path: String, from old: Decision, to new: Decision) {
        guard old != new else { return }
        for id in groupsOfPath[path] ?? [] {
            byGroup[id]?.add(old, -1)
            byGroup[id]?.add(new, 1)
        }
    }
}
