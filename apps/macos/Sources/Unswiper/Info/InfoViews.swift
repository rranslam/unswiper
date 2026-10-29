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
                // A plain ScrollView, not a grouped Form: Form rows are table cells with their
                // own hosting views whose heights depend on the column width, and wrapped,
                // selectable values made window resizing loop until AppKit threw
                // ("more Update Constraints in Window passes than there are views").
                ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        Text(InfoRules.heading(fileName: URL(fileURLWithPath: path).lastPathComponent, others: others))
                            .font(.headline)
                            .fixedSize(horizontal: false, vertical: true)
                            .contextMenu { copyButton(URL(fileURLWithPath: path).lastPathComponent) }
                        if let answer = state.infoAnswer(path) {
                            ForEach(InfoRules.sections(answer.rows, shortOnly: false)) { section in
                                VStack(alignment: .leading, spacing: 6) {
                                    Text(section.title)
                                        .font(.subheadline.weight(.semibold))
                                        .foregroundStyle(.secondary)
                                    // Fixed label column and plain (not selectable) text: a selectable
                                    // Text is an AppKit text view, and wrapped ones in a flexible
                                    // column got a new frame on every pass when the selection
                                    // changed, which crashed AppKit (v0.6.3). Right-click copies.
                                    VStack(alignment: .leading, spacing: 4) {
                                        ForEach(section.rows, id: \.self) { row in
                                            HStack(alignment: .firstTextBaseline, spacing: 8) {
                                                Text(row.label)
                                                    .foregroundStyle(.secondary)
                                                    .lineLimit(1)
                                                    .frame(width: 92, alignment: .trailing)
                                                Text(row.value)
                                                    .fixedSize(horizontal: false, vertical: true)
                                                    .frame(maxWidth: .infinity, alignment: .leading)
                                            }
                                            .contextMenu { copyButton(row.value) }
                                        }
                                    }
                                    .font(.callout)
                                    if section.title == "Location" { locationFooter(section.rows) }
                                }
                            }
                        } else if let problem = state.infoProblem {
                            Text(problem).foregroundStyle(.secondary)
                        } else {
                            ProgressView("Reading…").controlSize(.small)
                        }
                    }
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            } else {
                ContentUnavailableView(
                    "No Selection",
                    systemImage: "info.circle",
                    description: Text("Select a photo or video to see its camera, settings, date, size and location.")
                )
            }
        }
        // A fixed width: the panel never resizes with its content or the window.
        .frame(width: InfoRules.inspectorWidth)
        .inspectorColumnWidth(InfoRules.inspectorWidth)
        .task(id: InfoTaskKey(path: path, settings: state.infoSettings, generation: state.infoGeneration)) {
            if let path { state.loadInfo([path]) }
        }
    }

    private func copyButton(_ text: String) -> some View {
        Button("Copy") {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(text, forType: .string)
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
            .fixedSize(horizontal: false, vertical: true)
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
