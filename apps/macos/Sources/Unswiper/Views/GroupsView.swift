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
                        state.openGroup(group)
                    } label: {
                        GroupRow(group: group, items: state.items)
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
            if state.isIndexing {
                ProgressView()
                    .controlSize(.small)
                if let p = state.indexProgress {
                    Text("\(p.mediaFound) media / \(p.filesSeen) files")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
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
            Text(state.isIndexing
                 ? "Scanning your folder for photos and videos."
                 : "Choose a folder and index to build visit groups.")
                .foregroundStyle(.secondary)
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
    let items: [MediaItem]

    var body: some View {
        let progress = group.progress(in: items)
        HStack(alignment: .top, spacing: 14) {
            VStack(alignment: .leading, spacing: 4) {
                Text(group.title)
                    .font(.headline)
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
