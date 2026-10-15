// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// Pure rules for the media viewer (plan §16.3), shared in spirit with Linux `player.rs`.
enum ViewerRules {
    /// Clips shorter than this loop.
    static let loopUnder: Double = 10
    /// UserDefaults key for "Play Live Photo Motion" (off by default).
    static let liveMotionKey = "livePhotoMotion"

    /// The Live Photo motion clip of a photo: its first `.mov`/`.mp4`/`.m4v` sidecar.
    static func motionClip(kind: MediaKind, sidecarPaths: [String]) -> String? {
        guard kind == .photo else { return nil }
        return sidecarPaths.first { ["mov", "mp4", "m4v"].contains(($0 as NSString).pathExtension.lowercased()) }
    }

    /// What to play for an item: the video itself, the motion clip (option on), or nothing (still).
    static func playablePath(kind: MediaKind, path: String, sidecarPaths: [String], liveMotion: Bool) -> String? {
        switch kind {
        case .video: return path
        case .photo: return liveMotion ? motionClip(kind: kind, sidecarPaths: sidecarPaths) : nil
        }
    }

    static func shouldLoop(duration: Double, isMotion: Bool) -> Bool {
        isMotion || (duration > 0 && duration < loopUnder)
    }

    /// "0:07", "1:05", "1:02:03".
    static func formatDuration(_ seconds: Double) -> String {
        let s = max(0, Int(seconds.rounded()))
        let h = s / 3600, m = (s % 3600) / 60, sec = s % 60
        return h > 0 ? String(format: "%d:%02d:%02d", h, m, sec) : String(format: "%d:%02d", m, sec)
    }
}

/// Which item the viewer shows, within the visible (filtered) items. No wrap-around.
struct ViewerModel: Equatable {
    var current: String?

    var isOpen: Bool { current != nil }

    mutating func open(_ path: String, in paths: [String]) {
        current = paths.contains(path) ? path : nil
    }

    /// Opens on the first selected path that is visible (grid order).
    mutating func openOnSelection(_ selection: Set<String>, in paths: [String]) {
        current = paths.first { selection.contains($0) }
    }

    mutating func close() { current = nil }

    /// Moves by `delta`; stays put at either end. Returns whether it moved.
    @discardableResult
    mutating func step(_ delta: Int, in paths: [String]) -> Bool {
        guard let current, let i = paths.firstIndex(of: current) else { return false }
        let j = i + delta
        guard paths.indices.contains(j) else { return false }
        self.current = paths[j]
        return true
    }

    /// (1-based index, count), for "3 of 40".
    func position(in paths: [String]) -> (Int, Int)? {
        guard let current, let i = paths.firstIndex(of: current) else { return nil }
        return (i + 1, paths.count)
    }

    /// Closes when the shown item is no longer visible (filter changed, item removed).
    mutating func check(_ paths: [String]) {
        if let current, !paths.contains(current) { self.current = nil }
    }
}
