// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import SwiftUI

struct RootView: View {
    @EnvironmentObject private var state: AppState
    @State private var columns: NavigationSplitViewVisibility = .all
    /// The visits list was hidden by Unswiper to make room for Photo Info.
    @State private var sidebarAutoHidden = false

    /// Window title: the open visit's name, else the app's.
    private var detailTitle: String {
        guard let id = state.path.visitID,
              let group = state.groups.first(where: { $0.id == id }) else { return "Unswiper" }
        return group.title
    }

    /// The right side: the open visit's page, or a hint to pick one.
    @ViewBuilder private var detail: some View {
        switch state.path {
        case .reduce(let id):
            // A different visit is a fresh view: grid, no selection, no large photo.
            ReduceView(groupID: id)
                .id(id)
        case .commit(let id):
            CommitView(groupID: id)
        case .compare(let id, let paths):
            CompareView(groupID: id, paths: paths)
        case .home, .setup:
            ContentUnavailableView {
                Label(state.groups.isEmpty ? "No Visits Yet" : "No Visit Selected",
                      systemImage: "photo.on.rectangle.angled")
            } description: {
                Text(state.groups.isEmpty
                     ? "Visits appear on the left when indexing finishes."
                     : "Choose a visit on the left to review its photos.")
            }
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            Group {
                if state.path.isFullWindow {
                    SetupView()
                } else {
                    NavigationSplitView(columnVisibility: $columns) {
                        GroupsView()
                            .navigationSplitViewColumnWidth(min: 280, ideal: 340, max: 520)
                    } detail: {
                        detail
                            .frame(minWidth: 640, minHeight: 400)
                            .navigationTitle(detailTitle)
                    }
                    .navigationSplitViewStyle(.balanced)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)

            if let progress = state.blurProgress {
                Divider()
                HStack(spacing: 8) {
                    if let fraction = state.blurFraction {
                        ProgressView(value: fraction)
                            .frame(width: 160)
                    } else {
                        ProgressView().controlSize(.small)
                    }
                    Text(progress).font(.caption).foregroundStyle(.secondary)
                    Spacer()
                    if state.canPauseBlur {
                        Button(state.blurPaused ? "Resume" : "Pause") {
                            if state.blurPaused { state.resumeBlur() } else { state.pauseBlur() }
                        }
                        .buttonStyle(.borderless)
                        .font(.caption)
                    }
                    Button("Stop") { state.stopBlur() }
                        .buttonStyle(.borderless)
                        .font(.caption)
                        .help("Stop checking. Scores already saved are kept; the check runs again next time.")
                }
                .padding(.horizontal)
                .padding(.vertical, 6)
            }
            if let message = state.statusMessage {
                Divider()
                HStack {
                    Text(message)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button("Dismiss") { state.statusMessage = nil }
                        .buttonStyle(.borderless)
                        .font(.caption)
                }
                .padding(.horizontal)
                .padding(.vertical, 6)
            }
        }
        .frame(minWidth: 940, minHeight: 600)
        // Photo Info opening in a narrow window hides the visits list so the photos keep
        // their room, and closing it brings the list back. Decided on the click, from the
        // window's current width, never from layout.
        .onChange(of: state.infoOpen) { _, open in
            let width = Double(KeyRouter.shared.window?.frame.width ?? 10_000)
            let next = LayoutRules.sidebar(inspectorOpen: open, windowWidth: width,
                                           sidebarShown: columns != .detailOnly,
                                           autoHidden: sidebarAutoHidden)
            sidebarAutoHidden = next.autoHidden
            let wanted: NavigationSplitViewVisibility = next.show ? .all : .detailOnly
            if wanted != columns { withAnimation { columns = wanted } }
        }
        .sheet(isPresented: $state.cutoffsSheetOpen) { BlurCutoffsSheet() }
        .alert("Saved library couldn't be opened", isPresented: Binding(
            get: { state.libraryLoadProblem != nil },
            set: { if !$0 { state.libraryLoadProblem = nil } }
        )) {
            Button("OK") { state.libraryLoadProblem = nil }
        } message: {
            Text(state.libraryLoadProblem ?? "")
        }
        .alert("A commit was interrupted", isPresented: Binding(
            get: { state.interruptedCommitURL != nil && state.libraryLoadProblem == nil },
            set: { _ in }
        )) {
            Button("Restore Files") { Task { await state.restoreInterruptedCommit() } }
            Button("Leave As Is", role: .cancel) { state.keepInterruptedCommit() }
        } message: {
            Text("Unswiper quit while moving rejected files. Restore puts the files it already moved back where they were. Leave As Is keeps them in the Trash or quarantine folder (you can still use Restore Last Commit later).")
        }
    }
}
