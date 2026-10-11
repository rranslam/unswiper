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
    }
}
