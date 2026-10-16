// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// Pure rules for the photo info popover, inspector and Compare strip (plan §17.2), mirroring
/// `AppState::info_target` and `info.rs` in the Linux app.
enum InfoRules {
    /// The item the info shows: the one in the viewer, else the first selected one in grid order,
    /// plus how many others are selected.
    static func target(viewer: String?, selected: Set<String>, ordered: [String]) -> (path: String, others: Int)? {
        if let viewer, ordered.contains(viewer) { return (viewer, 0) }
        let picked = ordered.filter(selected.contains)
        guard let first = picked.first else { return nil }
        return (first, picked.count - 1)
    }

    /// Inspector heading: the file name, plus "(and N more selected)".
    static func heading(fileName: String, others: Int) -> String {
        others == 0 ? fileName : "\(fileName) (and \(others) more selected)"
    }

    /// Display title of a row group (`camera` → "Camera").
    static func title(ofGroup g: String) -> String {
        switch g {
        case "camera": return "Camera"
        case "exposure": return "Exposure"
        case "date": return "Date"
        case "image": return "Image"
        case "file": return "File"
        case "location": return "Location"
        case "unswiper": return "Unswiper"
        default: return g.prefix(1).uppercased() + g.dropFirst()
        }
    }

    struct Section: Equatable, Identifiable {
        var title: String
        var rows: [InfoHelper.Row]
        var id: String { title }
    }

    /// Rows grouped in the order the helper sent them; `shortOnly` for the popover.
    static func sections(_ rows: [InfoHelper.Row], shortOnly: Bool) -> [Section] {
        var out: [Section] = []
        for r in rows where !shortOnly || r.short {
            let t = title(ofGroup: r.group)
            if out.last?.title == t {
                out[out.count - 1].rows.append(r)
            } else {
                out.append(Section(title: t, rows: [r]))
            }
        }
        return out
    }

    /// OpenStreetMap link from a Coordinates row value ("37.77493, -122.41942"), for the
    /// "Open in Maps" button. Only opened when the user clicks.
    static func mapsURL(coordinates: String) -> URL? {
        let parts = coordinates.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }
        guard parts.count == 2, let lat = Double(parts[0]), let lon = Double(parts[1]),
              (-90...90).contains(lat), (-180...180).contains(lon) else { return nil }
        let la = String(format: "%.5f", lat), lo = String(format: "%.5f", lon)
        return URL(string: "https://www.openstreetmap.org/?mlat=\(la)&mlon=\(lo)#map=16/\(la)/\(lo)")
    }

    /// The Compare view shows the strip under one picture only: the focused one, else the first.
    static func compareFocus(_ focus: String?, in paths: [String]) -> String? {
        if let focus, paths.contains(focus) { return focus }
        return paths.first
    }
}
