// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

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
    /// Set while a commit or restore is moving files; navigation and file actions are disabled.
    @Published var busyMessage: String?
    var isBusy: Bool { busyMessage != nil || isIndexing }
    @Published var hasRestorableCommit = LibraryStore.latestManifestURL() != nil
    /// Shown once at launch when library.json couldn't be opened (it was backed up first).
    @Published var libraryLoadProblem: String?
    /// A commit manifest left `pending` by an interrupted commit; the UI offers to restore it.
    @Published var interruptedCommitURL: URL?
    /// Delay for debounced saves (tests shorten it).
    var saveDelay: Duration = .seconds(1)
    /// Set while the blur checker runs ("Checking sharpness…").
    @Published var blurProgress: String?
    /// Tests swap in a fake checker; nil means "find the bundled helper".
    var blurRunner: ((_ library: URL, _ sensitivity: BlurRules.Sensitivity, _ only: [String]?) async throws -> BlurHelper.Verdict)?
    private var blurTask: Task<Void, Never>?

    enum AppPath: Hashable {
        case home
        case setup
        case reduce(VisitGroup.ID)
        case commit(VisitGroup.ID)
        case compare(VisitGroup.ID, [String])
    }

    private var bookmarkURL: URL?
    /// The URL we currently hold security-scoped access to (started once, stopped on root change/quit).
    private var accessedURL: URL?
    private var saveTask: Task<Void, Never>?
    private var terminateObserver: NSObjectProtocol?

    init() {
        let loaded = LibraryStore.load()
        let snap = loaded.snapshot
        self.libraryLoadProblem = loaded.problem
        self.interruptedCommitURL = LibraryStore.pendingManifestURLs().last
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
        terminateObserver = NotificationCenter.default.addObserver(
            forName: NSApplication.willTerminateNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.flush()
                self?.stopAccess()
            }
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

    /// Debounced save (~1s). Use `flush()` to write immediately.
    func persist() {
        saveTask?.cancel()
        saveTask = Task { [weak self] in
            guard let delay = self?.saveDelay else { return }
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled else { return }
            self?.flush()
        }
    }

    func flush() {
        saveTask?.cancel()
        saveTask = nil
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
            beginAccess(url)
            persist()
            path = .home
            Task { await reindex() }
        } catch {
            // Non-sandboxed SPM binary may not need bookmarks
            config.rootPath = url.path
            config.bookmarkData = nil
            bookmarkURL = url
            stopAccess()
            persist()
            path = .home
            Task { await reindex() }
        }
    }

    func ensureAccess() -> URL? {
        if bookmarkURL == nil, let data = config.bookmarkData {
            restoreBookmark(data)
        }
        if let bookmarkURL {
            beginAccess(bookmarkURL)
            return bookmarkURL
        }
        if let path = config.rootPath {
            return URL(fileURLWithPath: path)
        }
        return nil
    }

    /// Starts security-scoped access once per root; stops access to any previous root.
    private func beginAccess(_ url: URL) {
        if let accessedURL, accessedURL.standardizedFileURL == url.standardizedFileURL { return }
        stopAccess()
        if url.startAccessingSecurityScopedResource() {
            accessedURL = url
        }
    }

    private func stopAccess() {
        accessedURL?.stopAccessingSecurityScopedResource()
        accessedURL = nil
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
        let before = items

        do {
            let result = try await Task.detached(priority: .userInitiated) {
                try LocalTreeIndexer.index(
                    root: root,
                    visitGapHours: gap,
                    existingDecisions: decisions,
                    onProgress: { _ in }
                )
            }.value
            var fresh = result.items
            BlurRules.carryOver(from: before, to: &fresh)
            items = fresh
            groups = result.groups
            lastIndexedAt = Date()
            indexProgress = .init(filesSeen: items.count, mediaFound: items.count)
            persist()
            statusMessage = "Indexed \(items.count) items in \(groups.count) visits."
            isIndexing = false
            checkBlur(visitFirst: selectedGroupID)
            return
        } catch {
            statusMessage = "Index failed: \(error.localizedDescription)"
        }
        isIndexing = false
    }

    func openGroup(_ group: VisitGroup) {
        selectedGroupID = group.id
        path = .reduce(group.id)
        checkBlur(visitFirst: group.id, visitOnly: true)
    }

    func items(in group: VisitGroup) -> [MediaItem] {
        let set = Set(group.itemPaths)
        return items.filter { set.contains($0.path) }.sorted { $0.takenAt < $1.takenAt }
    }

    func updateDecision(paths: [String], decision: Decision) {
        let targets = Set(paths)
        // Mutate in place so order is preserved and duplicate paths can't trap.
        for index in items.indices where targets.contains(items[index].path) {
            // The user's choice always wins over a blur suggestion (and is remembered).
            BlurRules.setManualDecision(&items[index], decision)
        }
        persist()
    }

    // MARK: Blur suggestions (plan §15, same rules and keys as Linux)

    var blurEnabled: Bool { BlurRules.enabled(config) }
    var blurSensitivity: BlurRules.Sensitivity { BlurRules.sensitivity(config) }

    func suggestedPaths(in group: VisitGroup) -> [String] {
        items(in: group).filter(BlurRules.isSuggested).map(\.path)
    }

    func isBlurSuggested(_ path: String) -> Bool {
        items.first { $0.path == path }.map(BlurRules.isSuggested) ?? false
    }

    func setBlurEnabled(_ on: Bool) {
        BlurRules.setEnabled(&config, on)
        if on {
            persist()
            checkBlur(visitFirst: selectedGroupID)
        } else {
            blurTask?.cancel()
            blurProgress = nil
            let n = BlurRules.withdrawAll(&items)
            persist()
            statusMessage = n > 0 ? "Blurry-photo detection is off. \(n) suggestion\(n == 1 ? "" : "s") removed." : "Blurry-photo detection is off."
        }
    }

    func setBlurSensitivity(_ s: BlurRules.Sensitivity) {
        BlurRules.setSensitivity(&config, s)
        persist()
        checkBlur(visitFirst: selectedGroupID)
    }

    /// "Not Blurry: Clear Suggestions in This Visit".
    func dismissSuggestions(in groupID: VisitGroup.ID) {
        guard let g = groups.first(where: { $0.id == groupID }) else { return }
        let n = BlurRules.dismiss(&items, paths: Set(g.itemPaths))
        if n > 0 { persist() }
        statusMessage = "\(n) suggestion\(n == 1 ? "" : "s") cleared."
    }

    /// "Reject All Blurry Suggestions in This Visit".
    func confirmSuggestions(in groupID: VisitGroup.ID) {
        guard let g = groups.first(where: { $0.id == groupID }) else { return }
        let n = BlurRules.confirm(&items, paths: Set(g.itemPaths))
        if n > 0 { persist() }
        statusMessage = "\(n) photo\(n == 1 ? "" : "s") kept as Reject."
    }

    /// Runs the blur checker in the background: the open visit first, then (unless `visitOnly`)
    /// the whole library. Suggestions only ever touch Undecided photos.
    /// `userAsked` (menu "Check for Blurry Photos Now") reports a missing helper; automatic
    /// checks stay quiet so a development build without the helper works as before.
    func checkBlur(visitFirst groupID: VisitGroup.ID?, visitOnly: Bool = false, userAsked: Bool = false) {
        guard blurEnabled, !items.isEmpty else { return }
        let runner = blurRunner ?? Self.helperRunner()
        guard let runner else {
            if userAsked { statusMessage = BlurHelper.HelperError.notFound.localizedDescription }
            return
        }
        blurTask?.cancel()
        let visitPaths = groupID.flatMap { id in groups.first { $0.id == id } }.map { $0.itemPaths }
        let sensitivity = blurSensitivity
        blurTask = Task { [weak self] in
            guard let self else { return }
            var passes: [[String]?] = []
            if let visitPaths { passes.append(visitPaths) }
            if !visitOnly || visitPaths == nil { passes.append(nil) }
            var suggested = 0
            for only in passes {
                guard !Task.isCancelled else { break }
                self.blurProgress = only == nil ? "Checking sharpness…" : "Checking sharpness in this visit…"
                self.flush() // the helper reads library.json
                do {
                    let v = try await runner(LibraryStore.snapshotURL, sensitivity, only)
                    guard !Task.isCancelled, self.blurEnabled else { break }
                    let r = BlurRules.apply(&self.items, flagged: v.flagged, scored: v.scored, enabled: true)
                    suggested += r.suggested
                    if r != BlurRules.ApplyReport() { self.persist() }
                } catch {
                    self.statusMessage = error.localizedDescription
                    break
                }
            }
            self.blurProgress = nil
            if suggested > 0, !Task.isCancelled {
                self.statusMessage = suggested == 1
                    ? "1 photo looks blurry and was marked Reject. Nothing moves until you commit."
                    : "\(suggested) photos look blurry and were marked Reject. Nothing moves until you commit."
            }
        }
    }

    private static func helperRunner() -> ((URL, BlurRules.Sensitivity, [String]?) async throws -> BlurHelper.Verdict)? {
        guard let helper = BlurHelper.locate() else { return nil }
        return { library, sensitivity, only in
            try await Task.detached(priority: .utility) {
                try BlurHelper.run(helper: helper, library: library, sensitivity: sensitivity, only: only)
            }.value
        }
    }

    /// Keep target for one visit; falls back to the global default.
    func keepTarget(for groupID: VisitGroup.ID) -> Int {
        config.keepTargets?[groupID] ?? config.keepTargetDefault
    }

    /// Sets the keep target for one visit only (does not change the global default).
    func setKeepTarget(_ value: Int, for groupID: VisitGroup.ID) {
        var targets = config.keepTargets ?? [:]
        targets[groupID] = max(1, value)
        config.keepTargets = targets
        persist()
    }

    func commitGroup(_ groupID: VisitGroup.ID) async {
        guard let group = groups.first(where: { $0.id == groupID }) else { return }
        let members = items(in: group)
        let root = ensureAccess()
        let mode = config.commitMode
        busyMessage = mode == .dryRun ? "Checking…" : "Moving \(members.filter { $0.decision == .reject }.count) items…"
        defer { busyMessage = nil }
        do {
            let result = try await Task.detached(priority: .userInitiated) {
                try CommitService.commitRejects(items: members, mode: mode, libraryRoot: root)
            }.value
            if result.manifestURL != nil { hasRestorableCommit = true }
            if !result.dryRun {
                let failed = Set(result.failed.map(\.path))
                let rejectedMoved = Set(members.filter { $0.decision == .reject && !failed.contains($0.path) }.map(\.path))
                items.removeAll { rejectedMoved.contains($0.path) }
                // Rebuild groups from remaining items
                groups = VisitGrouper.group(items: items.sorted { $0.takenAt < $1.takenAt }, gapHours: config.visitGapHours)
                flush()
            }
            if result.dryRun {
                statusMessage = "Dry run: would move \(result.wouldMove) items (\(ByteFormat.string(result.bytes))). Nothing was moved."
            } else {
                statusMessage = "Moved \(result.moved) items (\(ByteFormat.string(result.bytes)))."
            }
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
        await restoreCommit(at: url)
    }

    /// Restores the files moved by an interrupted (pending) commit found at launch.
    func restoreInterruptedCommit() async {
        guard let url = interruptedCommitURL else { return }
        interruptedCommitURL = nil
        await restoreCommit(at: url)
    }

    /// Keeps an interrupted commit as-is (its files stay where they are; Restore Last Commit still works).
    func keepInterruptedCommit() {
        guard let url = interruptedCommitURL else { return }
        interruptedCommitURL = nil
        if var manifest = try? LibraryStore.readManifest(at: url) {
            manifest.status = .complete
            try? LibraryStore.writeManifest(manifest, to: [url])
        }
    }

    func restoreCommit(at url: URL) async {
        _ = ensureAccess()
        busyMessage = "Restoring…"
        do {
            let manifest = try LibraryStore.readManifest(at: url)
            let result = await Task.detached(priority: .userInitiated) {
                CommitService.restore(manifest: manifest)
            }.value
            busyMessage = nil
            if result.failed.isEmpty {
                // Mark restored so it is no longer the "last" commit.
                try? FileManager.default.moveItem(at: url, to: url.deletingPathExtension().appendingPathExtension("restored"))
            }
            hasRestorableCommit = LibraryStore.latestManifestURL() != nil
            await reindex()
            let movedCount = manifest.entries.filter { $0.effectiveState != .failed }.count
            var msg = "Restored \(result.restored) of \(movedCount) files."
            if result.alreadyInPlace > 0 {
                msg += " \(result.alreadyInPlace) had not been moved yet."
            }
            if !result.failed.isEmpty {
                msg += " \(result.failed.count) failed (manifest kept): \(result.failed.first!.error)"
            }
            statusMessage = msg
        } catch {
            busyMessage = nil
            statusMessage = "Restore failed: \(error.localizedDescription)"
        }
    }
}
