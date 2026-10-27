// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import SwiftUI

struct GroupsView: View {
    @EnvironmentObject private var state: AppState

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            if state.groups.isEmpty {
                empty
            } else {
                List(state.groups) { group in
                    Button {
                        // Deferred: navigating from inside the List's (NSTableView) click
                        // handler is a reentrant table update and logs an AppKit warning.
                        Task { @MainActor in state.openGroup(group) }
                    } label: {
                        GroupRow(group: group, counts: state.visitCounts[group.id])
                    }
                    .buttonStyle(.plain)
                }
                .listStyle(.inset)
            }
        }
        .navigationTitle("Visits")
    }

    private var header: some View {
        HStack(spacing: 16) {
            VStack(alignment: .leading, spacing: 4) {
                Text(state.librarySummary)
                    .font(.headline)
                if let root = state.config.rootPath {
                    Text(root)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                if let at = state.lastIndexedAt {
                    Text("Last index \(at.formatted(date: .abbreviated, time: .shortened))")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            }
            Spacer()
            if let busy = state.busyMessage {
                ProgressView()
                    .controlSize(.small)
                Text(busy)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            if state.isIndexing, !state.groups.isEmpty {
                IndexProgressBar(compact: true)
                    .frame(maxWidth: 360)
            }
            if let g = state.groupingProgress {
                ProgressView()
                    .controlSize(.small)
                Text(g)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Picker("Grouping", selection: Binding(
                get: { state.config.grouping },
                set: { mode in Task { @MainActor in state.setGroupMode(mode) } }
            )) {
                ForEach(GroupMode.allCases) { mode in
                    Text(mode.label).tag(mode)
                }
            }
            .frame(width: 230)
            .disabled(state.isBusy)
            .help("How visits are formed: by time, by place, or by time and then place")

            Picker("Commit", selection: $state.config.commitMode) {
                ForEach(CommitMode.allCases) { mode in
                    Text(mode.label).tag(mode)
                }
            }
            .frame(width: 220)
            .disabled(state.isBusy)
            .onChange(of: state.config.commitMode) { _, _ in state.persist() }

            Button("Restore Last Commit") {
                Task { await state.restoreLastCommit() }
            }
            .disabled(state.isBusy || !state.hasRestorableCommit)
            .help("Move files from the most recent commit back to their original locations")

            Button("Re-index") {
                Task { await state.reindex() }
            }
            .disabled(state.isBusy || state.config.rootPath == nil)

            Button("Change Folder…") {
                state.path = .setup
            }
            .disabled(state.isBusy)
        }
        .padding()
    }

    private var empty: some View {
        VStack(spacing: 16) {
            Spacer()
            Text(state.isIndexing ? "Indexing…" : "No visits yet")
                .font(.title2)
            if state.isIndexing {
                IndexProgressBar(compact: false)
                    .frame(maxWidth: 480)
            } else {
                Text("Choose a folder and index to build visit groups.")
                    .foregroundStyle(.secondary)
            }
            if !state.isIndexing {
                Button("Choose Folder…") { state.path = .setup }
                    .buttonStyle(.borderedProminent)
            }
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }
}

struct GroupRow: View {
    let group: VisitGroup
    let counts: VisitCounts.Count

    var body: some View {
        let progress = counts
        HStack(alignment: .top, spacing: 14) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 4) {
                    if group.place != nil {
                        Image(systemName: "mappin.and.ellipse")
                            .foregroundStyle(.secondary)
                            .accessibilityLabel("Place")
                    }
                    Text(group.title)
                        .font(.headline)
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
            Spacer()
            VStack(alignment: .trailing, spacing: 4) {
                Text(ByteFormat.string(group.totalBytes))
                    .font(.headline.monospacedDigit())
                Text("\(group.photoCount) photos · \(group.videoCount) videos")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text("\(group.itemCount) items")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(.vertical, 6)
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
