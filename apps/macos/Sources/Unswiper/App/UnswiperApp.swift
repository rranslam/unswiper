// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import SwiftUI

@main
struct UnswiperApp: App {
    @StateObject private var state = AppState()
    @AppStorage(ViewerRules.liveMotionKey) private var liveMotion = false

    init() {
        // Diagnostic used by the release checks: prints the loaded help topics and exits.
        if CommandLine.arguments.contains("--help-selftest") {
            let topics = HelpLibrary.topics
            print("help-dir: \(HelpLibrary.helpDirectory?.path ?? "MISSING")")
            print("help-topics: \(topics.count)")
            topics.forEach { print(" - \($0.title)") }
            exit(topics.count > 1 ? 0 : 1)
        }
        BundleIDMigration.runAtLaunch()
    }

    var body: some Scene {
        WindowGroup("Unswiper") {
            RootView()
                .environmentObject(state)
                .modifier(OpenHelpOnLaunch())
        }
        .commands {
            CommandGroup(replacing: .newItem) {}
            CommandMenu("Library") {
                Button("Choose Folder…") {
                    state.path = .setup
                }
                Button("Re-index") {
                    Task { await state.reindex() }
                }
                .keyboardShortcut("r", modifiers: [.command, .shift])
            }
            CommandGroup(after: .toolbar) {
                Toggle("Play Live Photo Motion", isOn: $liveMotion)
            }
            CommandGroup(replacing: .help) {
                HelpMenuItems()
            }
        }

        Window("Unswiper Help", id: "help") {
            HelpView()
        }
        .defaultSize(width: 900, height: 640)
    }
}

private struct HelpMenuItems: View {
    @Environment(\.openWindow) private var openWindow
    @Environment(\.openURL) private var openURL

    var body: some View {
        Button("Unswiper Help") { show(nil) }
            .keyboardShortcut("?", modifiers: .command)
        Button("Keyboard Shortcuts") { show("keyboard") }
        Button("Getting Started") { show("getting-started") }
        Divider()
        Button("Release Notes (GitHub Releases)") { openURL(HelpLibrary.releasesURL) }
        Button("Report an Issue…") { openURL(HelpLibrary.issuesURL) }
    }

    private func show(_ key: String?) {
        if let key, let id = HelpLibrary.topicID(containing: key) {
            HelpRouter.shared.selection = id
        }
        openWindow(id: "help")
    }
}

/// `--show-help` launch argument opens the Help window (used for testing).
private struct OpenHelpOnLaunch: ViewModifier {
    @Environment(\.openWindow) private var openWindow
    func body(content: Content) -> some View {
        content.onAppear {
            if CommandLine.arguments.contains("--show-help") { openWindow(id: "help") }
        }
    }
}
