import SwiftUI

struct ReduceView: View {
    @EnvironmentObject private var state: AppState
    let groupID: VisitGroup.ID

    @State private var selected: Set<String> = []
    @State private var filter: MediaFilter = .all
    @State private var keepTarget: Int = 10

    enum MediaFilter: String, CaseIterable, Identifiable {
        case all, photos, videos
        var id: String { rawValue }
        var label: String {
            switch self {
            case .all: return "All"
            case .photos: return "Photos"
            case .videos: return "Videos"
            }
        }
    }

    private var group: VisitGroup? {
        state.groups.first { $0.id == groupID }
    }

    private var members: [MediaItem] {
        guard let group else { return [] }
        let all = state.items(in: group)
        switch filter {
        case .all: return all
        case .photos: return all.filter { $0.kind == .photo }
        case .videos: return all.filter { $0.kind == .video }
        }
    }

    private var counts: (kept: Int, rejected: Int, left: Int) {
        group?.progress(in: state.items) ?? (0, 0, 0)
    }

    var body: some View {
        Group {
            if let group {
                VStack(spacing: 0) {
                    header(group)
                    Divider()
                    grid
                    Divider()
                    bottomBar(group)
                }
            } else {
                ContentUnavailableView("Visit not found", systemImage: "questionmark.folder")
            }
        }
        .onAppear {
            keepTarget = state.keepTarget(for: groupID)
        }
    }

    private func header(_ group: VisitGroup) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Button {
                    state.path = .home
                } label: {
                    Label("Visits", systemImage: "chevron.left")
                }
                .buttonStyle(.borderless)
                Spacer()
                Text("Local")
                    .font(.caption)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 3)
                    .background(.quaternary, in: Capsule())
            }
            Text(group.title)
                .font(.title2.weight(.semibold))
            HStack(spacing: 16) {
                Text("\(group.itemCount) items · \(ByteFormat.string(group.totalBytes))")
                Text("Keep \(keepTarget)")
                Stepper("", value: $keepTarget, in: 1...max(1, group.itemCount))
                    .labelsHidden()
                    .onChange(of: keepTarget) { _, v in state.setKeepTarget(v, for: groupID) }
                Text("\(counts.kept) kept · \(counts.rejected) reject · \(counts.left) left")
                    .foregroundStyle(.secondary)
                if counts.kept >= keepTarget {
                    Text("Keep target reached")
                        .font(.caption)
                        .foregroundStyle(.orange)
                }
                Spacer()
                Picker("Filter", selection: $filter) {
                    ForEach(MediaFilter.allCases) { f in
                        Text(f.label).tag(f)
                    }
                }
                .pickerStyle(.segmented)
                .frame(maxWidth: 260)
            }
            .font(.subheadline)
        }
        .padding()
    }

    private var grid: some View {
        ScrollView {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 110), spacing: 8)], spacing: 8) {
                ForEach(members) { item in
                    MediaCell(
                        item: item,
                        selected: selected.contains(item.path),
                        onTap: { toggle(item.path) }
                    )
                    .contextMenu {
                        Button("Keep") { state.updateDecision(paths: [item.path], decision: .keep) }
                        Button("Reject") { state.updateDecision(paths: [item.path], decision: .reject) }
                        Button("Clear") { state.updateDecision(paths: [item.path], decision: .undecided) }
                    }
                }
            }
            .padding()
        }
    }

    private func bottomBar(_ group: VisitGroup) -> some View {
        HStack {
            Text("\(selected.count) selected")
                .foregroundStyle(.secondary)
            Spacer()
            Button("Keep") { apply(.keep) }
                .keyboardShortcut("k", modifiers: [])
                .disabled(selected.isEmpty)
            Button("Reject") { apply(.reject) }
                .keyboardShortcut("r", modifiers: [])
                .disabled(selected.isEmpty)
            Button("Compare") { openCompare() }
                .disabled(selected.count < 2 || selected.count > 4)
            Button("Review & commit…") {
                state.path = .commit(group.id)
            }
            .buttonStyle(.borderedProminent)
            .disabled(counts.rejected == 0)
            .keyboardShortcut(.return, modifiers: .command)
        }
        .padding()
    }

    private func toggle(_ path: String) {
        if selected.contains(path) { selected.remove(path) } else { selected.insert(path) }
    }

    private func toggleFocused() {
        // Space toggles all selected already; if none, no-op
    }

    private func apply(_ decision: Decision) {
        guard !selected.isEmpty else { return }
        state.updateDecision(paths: Array(selected), decision: decision)
        selected.removeAll()
    }

    private func openCompare() {
        guard selected.count >= 2, selected.count <= 4 else { return }
        state.path = .compare(groupID, Array(selected))
    }
}

struct MediaCell: View {
    let item: MediaItem
    let selected: Bool
    let onTap: () -> Void

    var body: some View {
        VStack(spacing: 4) {
            ZStack(alignment: .topTrailing) {
                ThumbnailView(url: item.fileURL, size: 110)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                    .opacity(item.decision == .reject ? 0.35 : 1)
                    .overlay {
                        RoundedRectangle(cornerRadius: 6)
                            .strokeBorder(selected ? Color.accentColor : .clear, lineWidth: 3)
                    }
                decisionBadge
            }
            Text(item.fileName)
                .font(.caption2)
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .onTapGesture(perform: onTap)
        .help(item.path)
    }

    @ViewBuilder
    private var decisionBadge: some View {
        switch item.decision {
        case .keep:
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(.white, .green)
                .padding(4)
        case .reject:
            Image(systemName: "xmark.circle.fill")
                .foregroundStyle(.white, .red)
                .padding(4)
        case .undecided:
            EmptyView()
        }
    }
}
