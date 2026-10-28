// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// A single-key command in an open visit (grid or large photo).
enum KeyCommand: Equatable, CaseIterable {
    case keep, reject, clear, view, close, previous, next, info
}

/// Which plain key does what. Handled once for the whole window (see `KeyRouter`), so it works
/// no matter whether the visits list or the photo grid was clicked last.
enum KeyRules {
    static let spaceKey: UInt16 = 49
    static let escapeKey: UInt16 = 53
    static let leftKey: UInt16 = 123
    static let rightKey: UInt16 = 124

    /// `characters` is the key without modifiers, lowercased. Keys pressed with ⌘, ⌃, ⌥ or ⇧
    /// and keys typed into a text field are left alone (nil).
    static func command(characters: String, keyCode: UInt16, hasModifiers: Bool,
                        editingText: Bool, viewerOpen: Bool) -> KeyCommand? {
        guard !hasModifiers, !editingText else { return nil }
        switch keyCode {
        case spaceKey: return viewerOpen ? .close : .view
        case escapeKey: return viewerOpen ? .close : nil
        // Without the large photo the arrows keep moving through the visits list.
        case leftKey: return viewerOpen ? .previous : nil
        case rightKey: return viewerOpen ? .next : nil
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

enum LaunchRules {
    /// True when started as a bare executable (`swift run`, Xcode's scheme on the package)
    /// instead of from Unswiper.app: macOS then treats it as a background tool with no
    /// menu bar or Dock icon unless it asks to be a regular app.
    static func isUnbundled(bundleURL: URL) -> Bool {
        bundleURL.pathExtension.lowercased() != "app"
    }
}
