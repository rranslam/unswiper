// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import SwiftUI

struct RootView: View {
    @EnvironmentObject private var state: AppState

    var body: some View {
        VStack(spacing: 0) {
            Group {
                switch state.path {
                case .setup:
                    SetupView()
                case .home:
                    GroupsView()
                case .reduce(let id):
                    // A different visit is a fresh view: grid, no selection, no large photo.
                    ReduceView(groupID: id)
                        .id(id)
                case .commit(let id):
                    CommitView(groupID: id)
                case .compare(let id, let paths):
                    CompareView(groupID: id, paths: paths)
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
        .frame(minWidth: 900, minHeight: 620)
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
