// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import SwiftUI

/// The visits list (the left side of the window): library header, grouping and commit
/// settings, then one row per visit. Clicking a visit opens it on the right.
struct GroupsView: View {
    @EnvironmentObject private var state: AppState

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            if state.groups.isEmpty {
                empty
            } else {
                List(selection: Binding(
                    get: { state.path.visitID },
                    // Deferred: navigating from inside the List's (NSTableView) selection
                    // handler is a reentrant table update and logs an AppKit warning.
                    set: { id in Task { @MainActor in state.selectVisit(id) } }
                )) {
                    ForEach(state.groups) { group in
                        GroupRow(group: group, counts: state.visitCounts[group.id])
                            .tag(group.id)
                    }
                }
                .listStyle(.sidebar)
            }
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(state.librarySummary)
                        .font(.headline)
                    if let root = state.config.rootPath {
                        Text(root)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                            .help(root)
                    }
                    if let at = state.lastIndexedAt {
                        Text("Last index \(at.formatted(date: .abbreviated, time: .shortened))")
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                    }
                }
                Spacer(minLength: 4)
                Menu {
                    Button("Re-index") {
                        Task { await state.reindex() }
                    }
                    .disabled(state.isBusy || state.config.rootPath == nil)
                    Button("Restore Last Commit") {
                        Task { await state.restoreLastCommit() }
                    }
                    .disabled(state.isBusy || !state.hasRestorableCommit)
                    Divider()
                    Button("Change Folder…") {
                        state.path = .setup
                    }
                    .disabled(state.isBusy)
                } label: {
                    Image(systemName: "ellipsis.circle")
                }
                .menuStyle(.button)
                .buttonStyle(.borderless)
                .menuIndicator(.hidden)
                .fixedSize()
                .help("Re-index, Restore Last Commit (move the files of the most recent commit back), Change Folder")
            }
            if let busy = state.busyMessage {
                HStack(spacing: 6) {
                    ProgressView()
                        .controlSize(.small)
                    Text(busy)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            }
            if state.isIndexing, !state.groups.isEmpty {
                IndexProgressBar(compact: true)
            }
            if let g = state.groupingProgress {
                HStack(spacing: 6) {
                    ProgressView()
                        .controlSize(.small)
                    Text(g)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            }
            Picker("Grouping", selection: Binding(
                get: { state.config.grouping },
                set: { mode in Task { @MainActor in state.setGroupMode(mode) } }
            )) {
                ForEach(GroupMode.allCases) { mode in
                    Text(mode.label).tag(mode)
                }
            }
            .disabled(state.isBusy)
            .help("How visits are formed: by time, by place, or by time and then place")

            Picker("Commit", selection: $state.config.commitMode) {
                ForEach(CommitMode.allCases) { mode in
                    Text(mode.label).tag(mode)
                }
            }
            .disabled(state.isBusy)
            .onChange(of: state.config.commitMode) { _, _ in state.persist() }
        }
        .padding(12)
    }

    private var empty: some View {
        VStack(spacing: 12) {
            Spacer()
            Text(state.isIndexing ? "Indexing…" : "No visits yet")
                .font(.title3)
            if state.isIndexing {
                IndexProgressBar(compact: false)
            } else {
                Text("Choose a folder and index to build visit groups.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                Button("Choose Folder…") { state.path = .setup }
                    .buttonStyle(.borderedProminent)
            }
            Spacer()
        }
        .padding()
        .frame(maxWidth: .infinity)
    }
}

struct GroupRow: View {
    let group: VisitGroup
    let counts: VisitCounts.Count

    var body: some View {
        let progress = counts
        HStack(alignment: .top, spacing: 8) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 4) {
                    if group.place != nil {
                        Image(systemName: "mappin.and.ellipse")
                            .foregroundStyle(.secondary)
                            .accessibilityLabel("Place")
                    }
                    Text(group.title)
                        .font(.headline)
                        .lineLimit(2)
                }
                Text(dateRange)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                if progress.kept + progress.rejected > 0 {
                    Text("\(progress.kept) kept · \(progress.rejected) reject · \(progress.left) left")
                        .font(.caption2)
                        .foregroundStyle(.tint)
                }
            }
            Spacer(minLength: 4)
            VStack(alignment: .trailing, spacing: 4) {
                Text(ByteFormat.string(group.totalBytes))
                    .font(.headline.monospacedDigit())
                Text("\(group.photoCount) photos · \(group.videoCount) videos")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Text("\(group.itemCount) items")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(.vertical, 4)
        .contentShape(Rectangle())
    }

    private var dateRange: String {
        if group.startDate == .distantPast {
            return group.id == "undated" ? "No date found" : "No date · folder \(group.folderHint)"
        }
        let f = Date.FormatStyle(date: .abbreviated, time: .omitted)
        if Calendar.current.isDate(group.startDate, inSameDayAs: group.endDate) {
            return group.startDate.formatted(f)
        }
        return "\(group.startDate.formatted(f)) – \(group.endDate.formatted(f))"
    }
}

/// Indexing progress: a bar (spinning while files are still being found), counts, time left
/// and Cancel. Refreshes on its own each second so the time left moves between updates.
struct IndexProgressBar: View {
    @EnvironmentObject private var state: AppState
    let compact: Bool

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            let p = state.indexProgress ?? .init(filesSeen: 0, mediaFound: 0)
            let elapsed = state.indexDatingStartedAt.map { context.date.timeIntervalSince($0) } ?? 0
            let shown = ProgressText.index(p, elapsedDating: elapsed)
            HStack(spacing: 8) {
                VStack(alignment: .leading, spacing: 2) {
                    if let fraction = shown.0 {
                        ProgressView(value: fraction)
                    } else {
                        ProgressView().progressViewStyle(.linear)
                    }
                    Text(shown.1)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(compact ? 1 : 2)
                        .truncationMode(.tail)
                }
                Button("Cancel") { state.cancelIndexing() }
                    .controlSize(compact ? .small : .regular)
                    .help("Stop indexing. The library stays as it was.")
            }
        }
    }
}
