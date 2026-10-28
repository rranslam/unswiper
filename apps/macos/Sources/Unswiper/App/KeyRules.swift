// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// A single-key command in an open visit (grid or large photo).
enum KeyCommand: Equatable, CaseIterable {
    case keep, reject, clear, view, close, previous, next, info
    /// Grid only: one row up/down, a page of rows, first/last photo.
    case up, down, pageUp, pageDown, home, end
}

/// Which plain key does what. Handled once for the whole window (see `KeyRouter`), so it works
/// no matter whether the visits list or the photo grid was clicked last.
enum KeyRules {
    static let spaceKey: UInt16 = 49
    static let escapeKey: UInt16 = 53
    static let leftKey: UInt16 = 123
    static let rightKey: UInt16 = 124
    static let downKey: UInt16 = 125
    static let upKey: UInt16 = 126
    static let pageUpKey: UInt16 = 116
    static let pageDownKey: UInt16 = 121
    static let homeKey: UInt16 = 115
    static let endKey: UInt16 = 119

    /// `characters` is the key without modifiers, lowercased. Keys pressed with ⌘, ⌃, ⌥ or ⇧
    /// and keys typed into a text field are left alone (nil).
    static func command(characters: String, keyCode: UInt16, hasModifiers: Bool,
                        editingText: Bool, viewerOpen: Bool) -> KeyCommand? {
        guard !hasModifiers, !editingText else { return nil }
        switch keyCode {
        case spaceKey: return viewerOpen ? .close : .view
        case escapeKey: return viewerOpen ? .close : nil
        // ←/→: the previous/next photo, in the large photo and in the grid (like Linux).
        case leftKey: return .previous
        case rightKey: return .next
        // Rows and pages move the grid selection; the large photo leaves them alone.
        case upKey: return viewerOpen ? nil : .up
        case downKey: return viewerOpen ? nil : .down
        case pageUpKey: return viewerOpen ? nil : .pageUp
        case pageDownKey: return viewerOpen ? nil : .pageDown
        case homeKey: return viewerOpen ? nil : .home
        case endKey: return viewerOpen ? nil : .end
        default: break
        }
        switch characters {
        case "k": return .keep
        case "r": return .reject
        case "u": return .clear
        case "i": return .info
        default: return nil
        }
    }
}

/// Moving the grid selection with the keyboard.
enum GridNav {
    /// Columns of the photo grid (LazyVGrid, adaptive minimum `cellMin`, `spacing`, padded
    /// by `padding` on each side) at `width`.
    static func columns(width: Double, cellMin: Double = 110, spacing: Double = 8, padding: Double = 16) -> Int {
        max(1, Int(((width - 2 * padding + spacing) / (cellMin + spacing)).rounded(.down)))
    }

    /// Full rows that fit in `height` (at least 1), for Page Up/Down.
    static func rowsPerPage(width: Double, height: Double, cellMin: Double = 110,
                            spacing: Double = 8, padding: Double = 16) -> Int {
        let cols = columns(width: width, cellMin: cellMin, spacing: spacing, padding: padding)
        let cell = (width - 2 * padding - Double(cols - 1) * spacing) / Double(cols)
        guard cell > 0 else { return 1 }
        return max(1, Int(((height - 2 * padding + spacing) / (cell + spacing)).rounded(.down)))
    }

    /// The index to select after `command`, from `current` (nil: nothing selected yet, so
    /// any move starts on the first photo, End on the last). Stays inside 0..<count; nil
    /// when there's nothing to move or the command isn't a move.
    static func target(from current: Int?, count: Int, columns: Int, rowsPerPage: Int,
                       command: KeyCommand) -> Int? {
        guard count > 0 else { return nil }
        let cols = max(1, columns)
        guard let current, current >= 0, current < count else {
            switch command {
            case .end: return count - 1
            case .previous, .next, .up, .down, .pageUp, .pageDown, .home: return 0
            default: return nil
            }
        }
        let step: Int
        switch command {
        case .previous: step = -1
        case .next: step = 1
        case .up: step = -cols
        case .down: step = cols
        case .pageUp: step = -cols * max(1, rowsPerPage)
        case .pageDown: step = cols * max(1, rowsPerPage)
        case .home: return 0
        case .end: return count - 1
        default: return nil
        }
        return min(count - 1, max(0, current + step))
    }
}

enum LaunchRules {
    /// True when started as a bare executable (`swift run`, Xcode's scheme on the package)
    /// instead of from Unswiper.app: macOS then treats it as a background tool with no
    /// menu bar or Dock icon unless it asks to be a regular app.
    static func isUnbundled(bundleURL: URL) -> Bool {
        bundleURL.pathExtension.lowercased() != "app"
    }
}
