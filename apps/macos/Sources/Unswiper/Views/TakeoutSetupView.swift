// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import SwiftUI

/// The "Import from Google Photos" assistant (plan §19.4): one step at a time, from opening Google
/// Takeout to opening the unpacked library. Texts come from the shared guide.
struct TakeoutSetupView: View {
    @EnvironmentObject private var state: AppState
    @EnvironmentObject private var model: TakeoutSetupModel
    @Environment(\.openURL) private var openURL
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text(model.stepLabel)
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                    Text(model.step.title)
                        .font(.title2.weight(.semibold))
                    MarkdownDocument(markdown: model.step.body)
                    actions
                }
                .frame(maxWidth: 640, alignment: .leading)
                .padding(28)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            Divider()
            HStack {
                Button("Back") { model.go(-1) }
                    .disabled(model.saved.step == 0 || model.isRunning)
                Spacer()
                if model.saved.step + 1 < model.guide.steps.count {
                    Button("Next") { model.go(1) }
                        .keyboardShortcut(.defaultAction)
                        .disabled(!model.canGoNext)
                }
            }
            .padding(.horizontal, 28)
            .padding(.vertical, 14)
        }
        .frame(minWidth: 620, minHeight: 560)
        .onAppear {
            if model.step.action == "choose_folders" { model.check() }
        }
        .onDisappear {
            // Closing the window stops a running unpack; it continues next time.
            model.stop()
        }
    }

    @ViewBuilder private var actions: some View {
        switch model.step.action {
        case "open_takeout":
            HStack(spacing: 12) {
                Button("Open Google Takeout") {
                    if let u = URL(string: model.guide.takeoutUrl) { openURL(u) }
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                Button("Open takeout.google.com") {
                    if let u = URL(string: model.guide.fallbackUrl) { openURL(u) }
                }
                .buttonStyle(.link)
            }
        case "choose_folders":
            VStack(alignment: .leading, spacing: 12) {
                folderRow("Archives folder", model.archivesURL) { model.chooseFolder(archives: true) }
                folderRow("Library folder", model.libraryURL) { model.chooseFolder(archives: false) }
                options
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(Array(model.figures.enumerated()), id: \.offset) { _, line in
                        Text(line)
                            .foregroundStyle(TakeoutHelper.isSpaceWarning(line) ? Color.red : Color.primary)
                            .fixedSize(horizontal: false, vertical: true)
                            .textSelection(.enabled)
                    }
                }
                .padding(.top, 4)
                HStack {
                    Button("Check Again") { model.check() }
                        .disabled(model.checking || model.isRunning || model.folderProblem != nil)
                    if model.checking { ProgressView().controlSize(.small) }
                }
            }
        case "unpack":
            VStack(alignment: .leading, spacing: 12) {
                options
                ProgressView(value: model.fraction)
                if model.isRunning {
                    Text(model.progressText).font(.callout).foregroundStyle(.secondary)
                } else if let lines = model.reportLines {
                    ForEach(Array(lines.enumerated()), id: \.offset) { _, l in
                        Text(l).fixedSize(horizontal: false, vertical: true).textSelection(.enabled)
                    }
                } else if let p = model.problem {
                    Text(p).foregroundStyle(.orange).fixedSize(horizontal: false, vertical: true)
                }
                HStack(spacing: 12) {
                    Button(model.problem == nil ? "Unpack" : "Continue Unpacking") { model.unpack() }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.large)
                        .disabled(!model.canUnpack)
                    if model.isRunning {
                        Button("Stop") { model.stop() }.controlSize(.large)
                    }
                }
            }
        case "index":
            Button("Open Library") {
                if let lib = model.finish() {
                    state.openLibrary(lib)
                }
                dismiss()
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
        default:
            EmptyView()
        }
    }

    private var options: some View {
        VStack(alignment: .leading, spacing: 6) {
            Toggle("Delete each archive after unpacking", isOn: Binding(
                get: { model.saved.options.deleteArchives },
                set: { var o = model.saved.options; o.deleteArchives = $0; model.setOptions(o) }
            ))
            Toggle("Also unpack album copies (complete album folders, needs more space)", isOn: Binding(
                get: { model.saved.options.keepDuplicates },
                set: { var o = model.saved.options; o.keepDuplicates = $0; model.setOptions(o) }
            ))
        }
        .toggleStyle(.checkbox)
        .disabled(model.isRunning)
    }

    private func folderRow(_ title: String, _ url: URL?, choose: @escaping () -> Void) -> some View {
        HStack(spacing: 12) {
            Text(title).font(.headline).frame(width: 130, alignment: .leading)
            Text(url?.path ?? "Not chosen yet")
                .foregroundStyle(url == nil ? Color.secondary : Color.primary)
                .lineLimit(1)
                .truncationMode(.middle)
                .frame(maxWidth: .infinity, alignment: .leading)
            Button("Choose…", action: choose)
                .disabled(model.isRunning)
        }
    }
}
