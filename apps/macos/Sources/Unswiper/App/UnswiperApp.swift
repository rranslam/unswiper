// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import SwiftUI

@main
struct UnswiperApp: App {
    @StateObject private var state = AppState()

    var body: some Scene {
        WindowGroup("Unswiper") {
            RootView()
                .environmentObject(state)
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
        }
    }
}
