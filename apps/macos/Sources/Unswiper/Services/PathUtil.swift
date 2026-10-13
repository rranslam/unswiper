// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

enum PathUtil {
    /// Absolute path with `.`/`..` removed and symlinks resolved (via realpath(3)), so
    /// `/var/...` and `/private/var/...` compare equal. For a path that doesn't exist, the
    /// deepest existing parent is resolved and the rest appended unchanged.
    static func canonicalPath(_ url: URL) -> String {
        let path = url.standardizedFileURL.path
        if let real = realpath(path, nil) {
            defer { free(real) }
            return String(cString: real)
        }
        let parent = (path as NSString).deletingLastPathComponent
        guard !parent.isEmpty, parent != path else { return path }
        let base = canonicalPath(URL(fileURLWithPath: parent))
        return (base as NSString).appendingPathComponent((path as NSString).lastPathComponent)
    }
}
