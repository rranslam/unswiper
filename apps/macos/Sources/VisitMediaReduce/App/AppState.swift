import Foundation
import AppKit
import Combine

@MainActor
final class AppState: ObservableObject {
    @Published var config: AppConfig
    @Published var items: [MediaItem] = []
    @Published var groups: [VisitGroup] = []
    @Published var lastIndexedAt: Date?
    @Published var isIndexing = false
    @Published var indexProgress: LocalTreeIndexer.Progress?
    @Published var statusMessage: String?
    @Published var selectedGroupID: VisitGroup.ID?
    @Published var path: AppPath = .home
    @Published var hasRestorableCommit = LibraryStore.latestManifestURL() != nil

    enum AppPath: Hashable {
        case home
        case setup
        case reduce(VisitGroup.ID)
        case commit(VisitGroup.ID)
        case compare(VisitGroup.ID, [String])
    }

    private var bookmarkURL: URL?

    init() {
        let snap = LibraryStore.load()
        self.config = snap.config
        self.items = snap.items
        self.groups = snap.groups
        self.lastIndexedAt = snap.lastIndexedAt
        if let data = snap.config.bookmarkData {
            restoreBookmark(data)
        }
        if config.rootPath == nil {
            path = .setup
        }
    }

    var librarySummary: String {
        let bytes = items.reduce(Int64(0)) { $0 + $1.totalBytes }
        return "\(items.count) items · \(ByteFormat.string(bytes))"
    }

    var selectedGroup: VisitGroup? {
        guard let id = selectedGroupID else { return nil }
        return groups.first { $0.id == id }
    }

    func persist() {
        let snap = LibrarySnapshot(
            config: config,
            items: items,
            groups: groups,
            lastIndexedAt: lastIndexedAt
        )
        do {
            try LibraryStore.save(snap)
        } catch {
            statusMessage = "Save failed: \(error.localizedDescription)"
        }
    }

    func pickFolder() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.message = "Choose a folder of photos and videos"
        panel.prompt = "Select"
        guard panel.runModal() == .OK, let url = panel.url else { return }

        do {
            let data = try url.bookmarkData(
                options: [.withSecurityScope],
                includingResourceValuesForKeys: nil,
                relativeTo: nil
            )
            config.bookmarkData = data
            config.rootPath = url.path
            bookmarkURL = url
            _ = url.startAccessingSecurityScopedResource()
            persist()
            path = .home
            Task { await reindex() }
        } catch {
            // Non-sandboxed SPM binary may not need bookmarks
            config.rootPath = url.path
            config.bookmarkData = nil
            bookmarkURL = url
            persist()
            path = .home
            Task { await reindex() }
        }
    }

    func ensureAccess() -> URL? {
        if let bookmarkURL {
            _ = bookmarkURL.startAccessingSecurityScopedResource()
            return bookmarkURL
        }
        if let data = config.bookmarkData {
            restoreBookmark(data)
            if let bookmarkURL {
                _ = bookmarkURL.startAccessingSecurityScopedResource()
                return bookmarkURL
            }
        }
        if let path = config.rootPath {
            return URL(fileURLWithPath: path)
        }
        return nil
    }

    private func restoreBookmark(_ data: Data) {
        var stale = false
        do {
            let url = try URL(
                resolvingBookmarkData: data,
                options: [.withSecurityScope],
                relativeTo: nil,
                bookmarkDataIsStale: &stale
            )
            bookmarkURL = url
            if stale {
                config.bookmarkData = try? url.bookmarkData(options: [.withSecurityScope], includingResourceValuesForKeys: nil, relativeTo: nil)
            }
        } catch {
            bookmarkURL = nil
        }
    }

    func reindex() async {
        guard let root = ensureAccess() else {
            path = .setup
            statusMessage = "Choose a library folder first."
            return
        }
        isIndexing = true
        indexProgress = .init(filesSeen: 0, mediaFound: 0)
        statusMessage = "Indexing…"
        let gap = config.visitGapHours
        let decisions = Dictionary(uniqueKeysWithValues: items.map { ($0.path, $0.decision) })

        do {
            let result = try await Task.detached(priority: .userInitiated) {
                try LocalTreeIndexer.index(
                    root: root,
                    visitGapHours: gap,
                    existingDecisions: decisions,
                    onProgress: { _ in }
                )
            }.value
            items = result.items
            groups = result.groups
            lastIndexedAt = Date()
            indexProgress = .init(filesSeen: items.count, mediaFound: items.count)
            persist()
            statusMessage = "Indexed \(items.count) items in \(groups.count) visits."
        } catch {
            statusMessage = "Index failed: \(error.localizedDescription)"
        }
        isIndexing = false
    }

    func openGroup(_ group: VisitGroup) {
        selectedGroupID = group.id
        path = .reduce(group.id)
    }

    func items(in group: VisitGroup) -> [MediaItem] {
        let set = Set(group.itemPaths)
        return items.filter { set.contains($0.path) }.sorted { $0.takenAt < $1.takenAt }
    }

    func updateDecision(paths: [String], decision: Decision) {
        var map = Dictionary(uniqueKeysWithValues: items.map { ($0.path, $0) })
        for path in paths {
            guard var item = map[path] else { continue }
            item.decision = decision
            map[path] = item
        }
        items = Array(map.values)
        persist()
    }

    func setKeepTarget(_ value: Int) {
        config.keepTargetDefault = max(1, value)
        persist()
    }

    func commitGroup(_ groupID: VisitGroup.ID) async {
        guard let group = groups.first(where: { $0.id == groupID }) else { return }
        let members = items(in: group)
        let root = ensureAccess()
        do {
            let result = try CommitService.commitRejects(
                items: members,
                mode: config.commitMode,
                libraryRoot: root
            )
            if result.manifestURL != nil { hasRestorableCommit = true }
            if !result.dryRun {
                let failed = Set(result.failed.map(\.path))
                let rejectedMoved = Set(members.filter { $0.decision == .reject && !failed.contains($0.path) }.map(\.path))
                items.removeAll { rejectedMoved.contains($0.path) }
                // Rebuild groups from remaining items
                groups = VisitGrouper.group(items: items.sorted { $0.takenAt < $1.takenAt }, gapHours: config.visitGapHours)
                persist()
            }
            let verb = result.dryRun ? "Would move" : "Moved"
            statusMessage = "\(verb) \(result.moved) items (\(ByteFormat.string(result.bytes)))."
            if !result.failed.isEmpty {
                statusMessage = (statusMessage ?? "") + " \(result.failed.count) failed."
            }
            path = .home
        } catch {
            statusMessage = "Commit failed: \(error.localizedDescription)"
        }
    }

    /// Moves every file from the most recent commit back to its original path, then re-indexes.
    func restoreLastCommit() async {
        guard let url = LibraryStore.latestManifestURL() else {
            statusMessage = "No commit to restore."
            hasRestorableCommit = false
            return
        }
        _ = ensureAccess()
        do {
            let manifest = try LibraryStore.readManifest(at: url)
            let result = CommitService.restore(manifest: manifest)
            if result.failed.isEmpty {
                // Mark restored so it is no longer the "last" commit.
                try? FileManager.default.moveItem(at: url, to: url.deletingPathExtension().appendingPathExtension("restored"))
            }
            hasRestorableCommit = LibraryStore.latestManifestURL() != nil
            await reindex()
            var msg = "Restored \(result.restored) of \(manifest.entries.count) files."
            if !result.failed.isEmpty {
                msg += " \(result.failed.count) failed (manifest kept): \(result.failed.first!.error)"
            }
            statusMessage = msg
        } catch {
            statusMessage = "Restore failed: \(error.localizedDescription)"
        }
    }
}
