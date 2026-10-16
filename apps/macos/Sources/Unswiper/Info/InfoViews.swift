// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import AppKit
import SwiftUI

/// Identity for "(re)load info": the item, the settings that change the rows, and the cache generation.
struct InfoTaskKey: Hashable {
    var path: String?
    var settings: InfoHelper.Settings
    var generation: Int
}

/// The Photo Info inspector (I / ⌘I): every row for the viewer item or the first selected item.
struct InfoInspector: View {
    @EnvironmentObject private var state: AppState
    let path: String?
    let others: Int

    var body: some View {
        Group {
            if let path {
                Form {
                    Section {
                        Text(InfoRules.heading(fileName: URL(fileURLWithPath: path).lastPathComponent, others: others))
                            .font(.headline)
                            .textSelection(.enabled)
                    }
                    if let answer = state.infoAnswer(path) {
                        ForEach(InfoRules.sections(answer.rows, shortOnly: false)) { section in
                            Section(section.title) {
                                ForEach(section.rows, id: \.self) { row in
                                    LabeledContent(row.label) {
                                        Text(row.value).textSelection(.enabled)
                                    }
                                }
                                if section.title == "Location" { locationFooter(section.rows) }
                            }
                        }
                    } else if let problem = state.infoProblem {
                        Text(problem).foregroundStyle(.secondary)
                    } else {
                        ProgressView("Reading…").controlSize(.small)
                    }
                }
                .formStyle(.grouped)
            } else {
                ContentUnavailableView(
                    "No Selection",
                    systemImage: "info.circle",
                    description: Text("Select a photo or video to see its camera, settings, date, size and location.")
                )
            }
        }
        .inspectorColumnWidth(min: 240, ideal: 300, max: 420)
        .task(id: InfoTaskKey(path: path, settings: state.infoSettings, generation: state.infoGeneration)) {
            if let path { state.loadInfo([path]) }
        }
    }

    @ViewBuilder
    private func locationFooter(_ rows: [InfoHelper.Row]) -> some View {
        if let coords = rows.first(where: { $0.label == "Coordinates" }),
           let url = InfoRules.mapsURL(coordinates: coords.value) {
            Button("Open in Maps") { NSWorkspace.shared.open(url) }
                .help("Opens this place on OpenStreetMap in your web browser. Only then does the location leave this Mac.")
        }
        Text("Shown only on this Mac. Turn off in View ▸ Show Location in Photo Info.")
            .font(.caption)
            .foregroundStyle(.secondary)
    }
}

/// Right-click ▸ Show Info: the short rows next to the tile.
struct InfoPopover: View {
    @EnvironmentObject private var state: AppState
    let item: MediaItem
    let onShowAll: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(item.fileName).font(.headline)
            if let answer = state.infoAnswer(item.path) {
                Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 4) {
                    ForEach(answer.rows.filter(\.short), id: \.self) { row in
                        GridRow {
                            Text(row.label)
                                .foregroundStyle(.secondary)
                                .gridColumnAlignment(.trailing)
                            Text(row.value)
                        }
                    }
                }
                .font(.callout)
            } else if let problem = state.infoProblem {
                Text(problem).foregroundStyle(.secondary)
            } else {
                ProgressView().controlSize(.small)
            }
            HStack {
                Spacer()
                Button("Show All Info") { onShowAll() }
            }
        }
        .padding(12)
        .frame(minWidth: 280, maxWidth: 420)
        .task(id: InfoTaskKey(path: item.path, settings: state.infoSettings, generation: state.infoGeneration)) {
            state.loadInfo([item.path])
        }
    }
}

/// Compare strip: one line of settings for one item only (the selected / focused one).
struct InfoStrip: View {
    @EnvironmentObject private var state: AppState
    let path: String?

    var body: some View {
        Group {
            if let path, let strip = state.infoAnswer(path)?.strip, !strip.isEmpty {
                Text(strip)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .textSelection(.enabled)
                    .help("Settings of the selected item")
            } else {
                Text(" ").font(.caption)
            }
        }
        .task(id: InfoTaskKey(path: path, settings: state.infoSettings, generation: state.infoGeneration)) {
            if let path { state.loadInfo([path]) }
        }
    }
}
