import SwiftUI

struct CommitView: View {
    @EnvironmentObject private var state: AppState
    let groupID: VisitGroup.ID
    @State private var isCommitting = false
    @State private var confirming = false

    private var group: VisitGroup? {
        state.groups.first { $0.id == groupID }
    }

    private var members: [MediaItem] {
        guard let group else { return [] }
        return state.items(in: group)
    }

    private var rejects: [MediaItem] { members.filter { $0.decision == .reject } }
    private var keeps: [MediaItem] { members.filter { $0.decision == .keep } }
    private var undecided: [MediaItem] { members.filter { $0.decision == .undecided } }
    private var rejectFileCount: Int { rejects.reduce(0) { $0 + $1.fileCount } }
    private var rejectBytes: Int64 { rejects.reduce(0) { $0 + $1.totalBytes } }

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Button {
                state.path = .reduce(groupID)
            } label: {
                Label("Back", systemImage: "chevron.left")
            }
            .buttonStyle(.borderless)
            .disabled(state.isBusy)

            Text("Review & commit")
                .font(.largeTitle.weight(.semibold))

            if let group {
                Text(group.title)
                    .foregroundStyle(.secondary)
            }

            VStack(alignment: .leading, spacing: 10) {
                labeled("Reject / move", "\(rejects.count) items · \(ByteFormat.string(rejectBytes))")
                labeled("Keep", "\(keeps.count) items")
                labeled("Undecided (left as-is)", "\(undecided.count) items")
                labeled("Mode", state.config.commitMode.label)
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 10))

            if !undecided.isEmpty {
                Label("\(undecided.count) still undecided — they will not be moved.", systemImage: "exclamationmark.triangle")
                    .foregroundStyle(.orange)
            }

            if state.config.commitMode == .dryRun {
                Text("Dry run records the count only; files stay in place.")
                    .foregroundStyle(.secondary)
            }

            HStack {
                Button("Cancel") {
                    state.path = .reduce(groupID)
                }
                .keyboardShortcut(.cancelAction)
                .disabled(state.isBusy)

                Spacer()

                if let busy = state.busyMessage {
                    ProgressView().controlSize(.small)
                    Text(busy).foregroundStyle(.secondary)
                }

                Button(commitLabel + "…") {
                    confirming = true
                }
                .buttonStyle(.borderedProminent)
                .disabled(rejects.isEmpty || isCommitting || state.isBusy)
            }
            .confirmationDialog(
                "\(commitLabel): \(rejects.count) items (\(rejectFileCount) files, \(ByteFormat.string(rejectBytes)))?",
                isPresented: $confirming,
                titleVisibility: .visible
            ) {
                Button(commitLabel, role: state.config.commitMode == .dryRun ? nil : .destructive) {
                    isCommitting = true
                    Task {
                        await state.commitGroup(groupID)
                        isCommitting = false
                    }
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text(state.config.commitMode == .dryRun
                     ? "No files will be moved."
                     : "Files are validated first; a manifest is saved so you can use Restore Last Commit.")
            }

            Spacer()
        }
        .padding(32)
        .frame(maxWidth: 640)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var commitLabel: String {
        switch state.config.commitMode {
        case .trash: return "Move to Trash"
        case .quarantine: return "Move to Quarantine"
        case .dryRun: return "Dry Run"
        }
    }

    private func labeled(_ title: String, _ value: String) -> some View {
        HStack {
            Text(title)
            Spacer()
            Text(value).fontWeight(.medium)
        }
    }
}
