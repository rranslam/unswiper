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
                    ReduceView(groupID: id)
                case .commit(let id):
                    CommitView(groupID: id)
                case .compare(let id, let paths):
                    CompareView(groupID: id, paths: paths)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)

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
