// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import AppKit
import SwiftUI

@main
struct UnswiperApp: App {
    @StateObject private var state = AppState()
    @StateObject private var takeout = TakeoutSetupModel()
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
            CommandGroup(replacing: .appInfo) {
                Button("About Unswiper") { AboutPanel.show() }
            }
            CommandGroup(replacing: .newItem) {}
            CommandMenu("Library") {
                Button("Choose Folder…") {
                    state.path = .setup
                }
                ImportGooglePhotosMenuItem()
                Button("Re-index") {
                    Task { await state.reindex() }
                }
                .keyboardShortcut("r", modifiers: [.command, .shift])
            }
            CommandGroup(after: .toolbar) {
                Toggle("Play Live Photo Motion", isOn: $liveMotion)
                Divider()
                Toggle("Show Inspector", isOn: $state.infoOpen)
                    .keyboardShortcut("i", modifiers: .command)
                Toggle("Show Location in Photo Info", isOn: Binding(
                    get: { state.showLocation },
                    set: { state.setShowLocation($0) }
                ))
                Divider()
                Picker("Grouping", selection: Binding(
                    get: { state.config.grouping },
                    set: { mode in Task { @MainActor in state.setGroupMode(mode) } }
                )) {
                    ForEach(GroupMode.allCases) { m in Text(m.label).tag(m) }
                }
                .disabled(state.isBusy)
            }
            CommandMenu("Blurry Photos") {
                Toggle("Detect Blurry Photos", isOn: Binding(
                    get: { state.blurEnabled },
                    set: { state.setBlurEnabled($0) }
                ))
                Picker("Blur Sensitivity", selection: Binding(
                    get: { state.blurSensitivity },
                    set: { state.setBlurSensitivity($0) }
                )) {
                    ForEach(BlurRules.Sensitivity.allCases) { s in Text(s.label).tag(s) }
                }
                .disabled(!state.blurEnabled)
                Button("Check for Blurry Photos Now") {
                    state.checkBlur(visitFirst: state.selectedGroupID, userAsked: true)
                }
                .disabled(!state.blurEnabled)
                Divider()
                Button("Not Blurry: Clear Suggestions in This Visit") {
                    if let id = state.selectedGroupID { state.dismissSuggestions(in: id) }
                }
                .disabled(state.selectedGroupID == nil)
                Button("Reject All Blurry Suggestions in This Visit") {
                    if let id = state.selectedGroupID { state.confirmSuggestions(in: id) }
                }
                .disabled(state.selectedGroupID == nil)
            }
            CommandGroup(replacing: .help) {
                HelpMenuItems()
            }
        }

        Window("Unswiper Help", id: "help") {
            HelpView()
        }
        .defaultSize(width: 900, height: 640)

        Window("Import from Google Photos", id: TakeoutSetupModel.windowID) {
            TakeoutSetupView()
                .environmentObject(state)
                .environmentObject(takeout)
        }
        .defaultSize(width: 720, height: 640)
    }
}

/// Library ▸ Import from Google Photos… (plan §19.4).
private struct ImportGooglePhotosMenuItem: View {
    @Environment(\.openWindow) private var openWindow
    var body: some View {
        Button("Import from Google Photos…") { openWindow(id: TakeoutSetupModel.windowID) }
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
        Button("Import from Google Photos") { show("google-photos") }
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

/// About Unswiper with the third-party notices (GeoNames CC BY 4.0, plan §20.3) as credits.
enum AboutPanel {
    /// Short attribution used when the bundled THIRD_PARTY_NOTICES.txt is missing (swift run).
    static let fallback = "Place names: GeoNames (www.geonames.org), CC BY 4.0. See THIRD_PARTY_NOTICES."

    static func noticesText(bundle: Bundle = .main) -> String {
        if let url = bundle.url(forResource: "THIRD_PARTY_NOTICES", withExtension: "txt"),
           let text = try? String(contentsOf: url, encoding: .utf8), !text.isEmpty {
            return text
        }
        return fallback
    }

    @MainActor static func show() {
        let credits = NSAttributedString(
            string: noticesText(),
            attributes: [.font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize),
                         .foregroundColor: NSColor.labelColor]
        )
        NSApplication.shared.orderFrontStandardAboutPanel(options: [.credits: credits])
        NSApplication.shared.activate(ignoringOtherApps: true)
    }
}
