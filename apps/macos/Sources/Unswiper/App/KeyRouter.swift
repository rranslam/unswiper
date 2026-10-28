// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import AppKit
import SwiftUI

/// Sends K, R, U, I, Space, Esc and ←/→ in the main window to the open visit.
///
/// Plain-key `.keyboardShortcut`s on buttons inside the visit page don't fire while the
/// visits list (an NSTableView) has keyboard focus: the table takes letters for type-to-select
/// and Space/arrows itself. A local key monitor sees the key first, for the main window only,
/// and skips text fields and sheets (for example Blur Cutoffs), so typing is never taken.
@MainActor
final class KeyRouter: ObservableObject {
    static let shared = KeyRouter()

    struct Target {
        let token: UUID
        let viewerOpen: () -> Bool
        let perform: (KeyCommand) -> Bool
    }

    /// The window that shows the visits (set by `RootView`).
    weak var window: NSWindow?
    @Published private(set) var target: Target?
    private var monitor: Any?

    func attach(_ target: Target) { self.target = target }

    func detach(_ token: UUID) {
        if target?.token == token { target = nil }
    }

    /// From the menu bar (Photo menu).
    func perform(_ command: KeyCommand) {
        _ = target?.perform(command)
    }

    func install() {
        guard monitor == nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            let handled = MainActor.assumeIsolated { KeyRouter.shared.handle(event) }
            return handled ? nil : event
        }
    }

    private func handle(_ event: NSEvent) -> Bool {
        guard let target, let window, event.window === window, window.isKeyWindow,
              window.attachedSheet == nil, NSApp.modalWindow == nil else { return false }
        // The field editor (and any text view) is an NSText.
        let editing = window.firstResponder is NSText
        let modifiers = !event.modifierFlags.intersection([.command, .control, .option, .shift]).isEmpty
        guard let command = KeyRules.command(
            characters: event.charactersIgnoringModifiers?.lowercased() ?? "",
            keyCode: event.keyCode,
            hasModifiers: modifiers,
            editingText: editing,
            viewerOpen: target.viewerOpen()
        ) else { return false }
        return target.perform(command)
    }
}

/// Tells `KeyRouter` which window is the main one.
struct MainWindowReader: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView { WindowTrackingView() }
    func updateNSView(_ nsView: NSView, context: Context) {}

    final class WindowTrackingView: NSView {
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if let window { KeyRouter.shared.window = window }
        }
    }
}

/// Photo menu: the single-key commands, so they're listed in the menu bar. The keys are
/// shown in the titles rather than as menu key equivalents, which would also fire while
/// typing in a text field.
struct PhotoMenuItems: View {
    @ObservedObject private var router = KeyRouter.shared

    var body: some View {
        Button("Keep (K)") { router.perform(.keep) }
            .needsVisit(router)
        Button("Reject (R)") { router.perform(.reject) }
            .needsVisit(router)
        Button("Clear (U)") { router.perform(.clear) }
            .needsVisit(router)
        Divider()
        Button("View Large / Back to Grid (Space)") { router.perform(.view) }
            .needsVisit(router)
        Button("Previous (←)") { router.perform(.previous) }
            .needsVisit(router)
        Button("Next (→)") { router.perform(.next) }
            .needsVisit(router)
        Divider()
        Button("Show Info (I)") { router.perform(.info) }
            .needsVisit(router)
    }
}

private extension View {
    func needsVisit(_ router: KeyRouter) -> some View { disabled(router.target == nil) }
}

/// Starts as a regular app when run with `swift run` (menu bar, Dock icon, front window).
final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationWillFinishLaunching(_ notification: Notification) {
        if LaunchRules.isUnbundled(bundleURL: Bundle.main.bundleURL) {
            NSApplication.shared.setActivationPolicy(.regular)
            // Dock icon: otherwise a generic one (no .app, no AppIcon.icns).
            MainActor.assumeIsolated {
                if let icon = AboutPanel.icon { NSApplication.shared.applicationIconImage = icon }
            }
        }
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        MainActor.assumeIsolated { KeyRouter.shared.install() }
        if LaunchRules.isUnbundled(bundleURL: Bundle.main.bundleURL) {
            NSApplication.shared.activate(ignoringOtherApps: true)
        }
    }
}
