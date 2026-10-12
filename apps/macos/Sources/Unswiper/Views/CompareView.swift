// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import SwiftUI

struct CompareView: View {
    @EnvironmentObject private var state: AppState
    let groupID: VisitGroup.ID
    let paths: [String]

    private var items: [MediaItem] {
        paths.compactMap { path in state.items.first { $0.path == path } }
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Button("Done") {
                    state.path = .reduce(groupID)
                }
                Spacer()
                Text("Compare \(items.count)")
                    .font(.headline)
                Spacer()
                Color.clear.frame(width: 60)
            }
            .padding()
            Divider()
            HStack(alignment: .top, spacing: 12) {
                ForEach(items) { item in
                    VStack(spacing: 8) {
                        ThumbnailView(url: item.fileURL, size: 280)
                            .frame(maxWidth: .infinity, maxHeight: .infinity)
                            .clipShape(RoundedRectangle(cornerRadius: 8))
                        Text(item.fileName)
                            .font(.caption)
                            .lineLimit(2)
                        Text("\(ByteFormat.string(item.sizeBytes)) · \(item.isUndated ? "Undated" : item.takenAt.formatted(date: .abbreviated, time: .shortened))")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                        HStack {
                            Button("Keep") {
                                state.updateDecision(paths: [item.path], decision: .keep)
                            }
                            Button("Reject") {
                                state.updateDecision(paths: [item.path], decision: .reject)
                            }
                            Button("Clear") {
                                state.updateDecision(paths: [item.path], decision: .undecided)
                            }
                        }
                        .controlSize(.small)
                    }
                    .frame(maxWidth: .infinity)
                }
            }
            .padding()
        }
    }
}
