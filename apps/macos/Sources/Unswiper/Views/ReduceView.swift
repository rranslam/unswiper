// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import SwiftUI

struct ReduceView: View {
    @EnvironmentObject private var state: AppState
    let groupID: VisitGroup.ID

    @State private var selected: Set<String> = []
    @State private var filter: MediaFilter = .all
    @State private var keepTarget: Int = 10
    @State private var viewer = ViewerModel()
    /// Path whose right-click Show Info popover is open.
    @State private var infoPopover: String?
    /// Identifies this page to KeyRouter (another visit's page may appear before this one goes).
    @State private var keyToken = UUID()
    /// The photo the arrow keys move from (the last one clicked or moved to).
    @State private var cursor: String?
    /// Scrolls the grid to this photo (set by a key press, read by the ScrollViewReader).
    @State private var scrollTarget: String?
    /// The grid's size, noted for the keys. A plain object, not observed state: writing it
    /// while the grid lays out never redraws anything, so it can't start a layout loop.
    @State private var gridSize = GridSizeBox()

    enum MediaFilter: String, CaseIterable, Identifiable {
        case all, photos, videos, blurry
        var id: String { rawValue }
        var label: String {
            switch self {
            case .all: return "All"
            case .photos: return "Photos"
            case .videos: return "Videos"
            case .blurry: return "Blurry"
            }
        }
    }

    private var group: VisitGroup? {
        state.groups.first { $0.id == groupID }
    }

    private var members: [MediaItem] {
        guard let group else { return [] }
        let all = state.items(in: group)
        switch filter {
        case .all: return all
        case .photos: return all.filter { $0.kind == .photo }
        case .videos: return all.filter { $0.kind == .video }
        case .blurry: return all.filter(BlurRules.isSuggested)
        }
    }

    /// What Photo Info shows: the viewer item, else the first selected item in grid order.
    private var infoTarget: (path: String, others: Int)? {
        InfoRules.target(viewer: viewer.current, selected: selected, ordered: members.map(\.path))
    }

    private var counts: (kept: Int, rejected: Int, left: Int) {
        group.map { state.visitCounts[$0.id].tuple } ?? (0, 0, 0)
    }

    var body: some View {
        Group {
            if let group, let path = viewer.current, let item = members.first(where: { $0.path == path }) {
                MediaViewer(
                    item: item,
                    position: viewer.position(in: members.map(\.path)),
                    onClose: { viewer.close() },
                    onStep: { viewer.step($0, in: members.map(\.path)) }
                )
                .id(group.id)
            } else if let group {
                VStack(spacing: 0) {
                    header(group)
                    Divider()
                    grid
                    Divider()
                    bottomBar(group)
                }
            } else {
                ContentUnavailableView("Visit not found", systemImage: "questionmark.folder")
            }
        }
        // The photos keep at least this much room next to Photo Info.
        .frame(minWidth: LayoutRules.contentMin)
        .inspector(isPresented: $state.infoOpen) {
            InfoInspector(path: infoTarget?.path, others: infoTarget?.others ?? 0)
        }
        .onAppear {
            keepTarget = state.keepTarget(for: groupID)
            KeyRouter.shared.attach(.init(token: keyToken,
                                          viewerOpen: { viewer.current != nil },
                                          perform: { handleKey($0) }))
        }
        .onDisappear { KeyRouter.shared.detach(keyToken) }
        .onChange(of: members.map(\.path)) { _, paths in viewer.check(paths) }
        // Another grouping: back to the grid, like switching visits.
        .onChange(of: state.groupingGeneration) { _, _ in viewer.close() }
    }

    private func header(_ group: VisitGroup) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline) {
                Text(group.title)
                    .font(.title2.weight(.semibold))
                    .lineLimit(2)
                Spacer()
                Text("Local")
                    .font(.caption)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 3)
                    .background(.quaternary, in: Capsule())
            }
            HStack(spacing: 16) {
                Text("\(group.itemCount) items · \(ByteFormat.string(group.totalBytes))")
                Text("Keep \(keepTarget)")
                Stepper("", value: $keepTarget, in: 1...max(1, group.itemCount))
                    .labelsHidden()
                    .onChange(of: keepTarget) { _, v in state.setKeepTarget(v, for: groupID) }
                Text("\(counts.kept) kept · \(counts.rejected) reject · \(counts.left) left")
                    .foregroundStyle(.secondary)
                if counts.kept >= keepTarget {
                    Text("Keep target reached")
                        .font(.caption)
                        .foregroundStyle(.orange)
                }
                Spacer()
                Picker("Filter", selection: $filter) {
                    ForEach(MediaFilter.allCases) { f in
                        Text(f.label).tag(f)
                    }
                }
                .pickerStyle(.segmented)
                .frame(maxWidth: 320)
                .help("Blurry shows only the photos Unswiper suggested as blurry")
                blurMenu(group)
            }
            .font(.subheadline)
        }
        .padding()
    }

    private func blurMenu(_ group: VisitGroup) -> some View {
        let n = state.suggestedPaths(in: group).count
        return Menu {
            Button("Not Blurry: Clear Suggestions in This Visit") { state.dismissSuggestions(in: group.id) }
                .disabled(n == 0)
            Button("Reject All Blurry Suggestions in This Visit") { state.confirmSuggestions(in: group.id) }
                .disabled(n == 0)
        } label: {
            Label(n == 0 ? "Blurry" : "Blurry? \(n)", systemImage: "camera.metering.unknown")
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
        .help("Photos suggested as blurry in this visit")
    }

    private var grid: some View {
        ScrollViewReader { proxy in
        ScrollView {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 110), spacing: 8)], spacing: 8) {
                ForEach(members) { item in
                    MediaCell(
                        item: item,
                        selected: selected.contains(item.path),
                        onTap: { toggle(item.path); cursor = item.path },
                        onOpen: { viewer.open(item.path, in: members.map(\.path)) }
                    )
                    .popover(isPresented: Binding(
                        get: { infoPopover == item.path },
                        set: { if !$0 { infoPopover = nil } }
                    ), arrowEdge: .trailing) {
                        InfoPopover(item: item) {
                            infoPopover = nil
                            selected = [item.path]
                            state.infoOpen = true
                        }
                        .environmentObject(state)
                    }
                    .contextMenu {
                        Button("Show Info") { infoPopover = item.path }
                        Divider()
                        Button("Keep") { state.updateDecision(paths: [item.path], decision: .keep) }
                        Button("Reject") { state.updateDecision(paths: [item.path], decision: .reject) }
                        Button(BlurRules.isSuggested(item) ? "Clear (Not Blurry)" : "Clear") { state.updateDecision(paths: [item.path], decision: .undecided) }
                    }
                }
            }
            .padding()
        }
        .background {
            GeometryReader { geo in
                Color.clear
                    .onAppear { gridSize.size = geo.size }
                    .onChange(of: geo.size) { _, size in gridSize.size = size }
            }
        }
        .onChange(of: scrollTarget) { _, target in
            guard let target else { return }
            proxy.scrollTo(target)
        }
        }
    }

    /// ←/→, ↑/↓, Page Up/Down, Home/End in the grid: select one photo and scroll to it.
    private func moveSelection(_ command: KeyCommand) -> Bool {
        let paths = members.map(\.path)
        let from = cursor.flatMap { c in selected.contains(c) ? paths.firstIndex(of: c) : nil }
            ?? paths.firstIndex(where: selected.contains)
        let size = gridSize.size
        guard let to = GridNav.target(
            from: from, count: paths.count,
            columns: GridNav.columns(width: size.width),
            rowsPerPage: GridNav.rowsPerPage(width: size.width, height: size.height),
            command: command
        ) else { return false }
        selected = [paths[to]]
        cursor = paths[to]
        scrollTarget = paths[to]
        return true
    }

    private func bottomBar(_ group: VisitGroup) -> some View {
        HStack {
            Text("\(selected.count) selected")
                .foregroundStyle(.secondary)
            InfoStrip(path: viewer.current == nil ? infoTarget?.path : nil)
                .frame(maxWidth: 360, alignment: .leading)
            Spacer()
            Button("Info") { state.infoOpen.toggle() }
                .help("Show or hide Photo Info (I or ⌘I)")
            Button("Keep") { apply(.keep) }
                .disabled(selected.isEmpty)
            Button("Reject") { apply(.reject) }
                .disabled(selected.isEmpty)
            Button("Clear") { apply(.undecided) }
                .disabled(selected.isEmpty)
            Button("View") { viewer.openOnSelection(selected, in: members.map(\.path)) }
                .disabled(selected.isEmpty)
            Button("Compare") { openCompare() }
                .disabled(selected.count < 2 || selected.count > 4)
            Button("Review & commit…") {
                state.path = .commit(group.id)
            }
            .buttonStyle(.borderedProminent)
            .disabled(counts.rejected == 0)
            .keyboardShortcut(.return, modifiers: .command)
        }
        .padding()
    }

    /// K/R/U/I/Space/Esc/←/→ (from KeyRouter or the Photo menu). True when the key was used.
    private func handleKey(_ command: KeyCommand) -> Bool {
        let paths = members.map(\.path)
        if let current = viewer.current {
            switch command {
            case .keep: state.updateDecision(paths: [current], decision: .keep)
            case .reject: state.updateDecision(paths: [current], decision: .reject)
            case .clear: state.updateDecision(paths: [current], decision: .undecided)
            case .view, .close: viewer.close()
            case .previous: _ = viewer.step(-1, in: paths)
            case .next: _ = viewer.step(1, in: paths)
            case .info: state.infoOpen.toggle()
            case .up, .down, .pageUp, .pageDown, .home, .end: return false
            }
            return true
        }
        switch command {
        case .keep: apply(.keep)
        case .reject: apply(.reject)
        case .clear: apply(.undecided)
        case .view: viewer.openOnSelection(selected, in: paths)
        case .info: state.infoOpen.toggle()
        case .previous, .next, .up, .down, .pageUp, .pageDown, .home, .end:
            return moveSelection(command)
        case .close: return false
        }
        return true
    }

    private func toggle(_ path: String) {
        if selected.contains(path) { selected.remove(path) } else { selected.insert(path) }
    }

    private func apply(_ decision: Decision) {
        guard !selected.isEmpty else { return }
        state.updateDecision(paths: Array(selected), decision: decision)
        selected.removeAll()
    }

    private func openCompare() {
        guard selected.count >= 2, selected.count <= 4 else { return }
        state.path = .compare(groupID, Array(selected))
    }
}

struct MediaCell: View {
    let item: MediaItem
    let selected: Bool
    let onTap: () -> Void
    var onOpen: () -> Void = {}

    var body: some View {
        VStack(spacing: 4) {
            ZStack(alignment: .topTrailing) {
                ThumbnailView(url: item.fileURL, size: 110)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                    .overlay(alignment: .topLeading) {
                        if BlurRules.isSuggested(item) {
                            Text("Blurry?")
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(.white)
                                .padding(.horizontal, 5)
                                .padding(.vertical, 2)
                                .background(.orange, in: Capsule())
                                .padding(4)
                                .help("Unswiper thinks this photo is blurry and marked it Reject. Press K to keep it, or U if it isn't blurry.")
                        }
                    }
                    .opacity(item.decision == .reject ? 0.35 : 1)
                    .overlay {
                        RoundedRectangle(cornerRadius: 6)
                            .strokeBorder(selected ? Color.accentColor : .clear, lineWidth: 3)
                    }
                decisionBadge
            }
            .overlay(alignment: .bottomLeading) {
                if item.kind == .video { PlayBadge(url: item.fileURL) }
            }
            Text(item.fileName)
                .font(.caption2)
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .gesture(TapGesture(count: 2).onEnded(onOpen).exclusively(before: TapGesture().onEnded(onTap)))
        .help(item.path)
    }

    @ViewBuilder
    private var decisionBadge: some View {
        switch item.decision {
        case .keep:
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(.white, .green)
                .padding(4)
        case .reject:
            Image(systemName: "xmark.circle.fill")
                .foregroundStyle(.white, .red)
                .padding(4)
        case .undecided:
            EmptyView()
        }
    }
}

/// Last known size of the photo grid (see `ReduceView.gridSize`).
final class GridSizeBox {
    var size: CGSize = .zero
}
