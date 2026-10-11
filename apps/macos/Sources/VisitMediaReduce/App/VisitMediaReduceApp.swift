import SwiftUI

@main
struct VisitMediaReduceApp: App {
    @StateObject private var state = AppState()

    var body: some Scene {
        WindowGroup("Visit Media Reduce") {
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
