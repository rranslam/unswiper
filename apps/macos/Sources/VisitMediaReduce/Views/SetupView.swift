import SwiftUI

struct SetupView: View {
    @EnvironmentObject private var state: AppState

    var body: some View {
        VStack(spacing: 28) {
            Spacer()
            Image(systemName: "photo.on.rectangle.angled")
                .font(.system(size: 56))
                .foregroundStyle(.secondary)
            Text("Visit Media Reduce")
                .font(.largeTitle.weight(.semibold))
            Text("Group photos and videos by visit, then keep what matters — not unrelated swipes.")
                .font(.title3)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 480)

            VStack(alignment: .leading, spacing: 12) {
                Text("1. Choose a local folder")
                Text("2. Index into visit groups (by time)")
                Text("3. Open a group, keep a shortlist, trash the rest")
            }
            .font(.body)
            .foregroundStyle(.secondary)

            Button("Choose Library Folder…") {
                state.pickFolder()
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)

            if let root = state.config.rootPath {
                Text(root)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .truncationMode(.middle)
            }

            Spacer()
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
