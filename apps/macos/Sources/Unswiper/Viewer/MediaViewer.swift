// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import AppKit
import AVFoundation
import AVKit
import SwiftUI

/// Session-wide mute choice: starts muted every launch (plan §16.8).
@MainActor
final class ViewerSession: ObservableObject {
    static let shared = ViewerSession()
    @Published var muted = true
}

/// Full-window viewer for one item: a video (AVPlayerView), a Live Photo motion clip, or a large still.
struct MediaViewer: View {
    @EnvironmentObject private var state: AppState
    @AppStorage(ViewerRules.liveMotionKey) private var liveMotion = false
    let item: MediaItem
    let position: (Int, Int)?
    let onClose: () -> Void
    let onStep: (Int) -> Void

    @State private var failed = false

    private var playPath: String? {
        ViewerRules.playablePath(kind: item.kind, path: item.path,
                                 sidecarPaths: item.sidecars.map(\.path), liveMotion: liveMotion)
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Button { onClose() } label: { Label("Grid", systemImage: "square.grid.2x2") }
                Text(item.fileName).font(.headline).lineLimit(1).truncationMode(.middle)
                badge
                Spacer()
                if let p = position { Text("\(p.0) of \(p.1)").foregroundStyle(.secondary) }
                Button("Open in Default App") { NSWorkspace.shared.open(item.fileURL) }
            }
            .padding()
            Divider()
            ZStack {
                Color.black
                content
                HStack {
                    navButton("chevron.left") { onStep(-1) }
                    Spacer()
                    navButton("chevron.right") { onStep(1) }
                }
                .padding()
            }
        }
        .onChange(of: item.path) { _, _ in failed = false }
        // Space/Esc close, arrows step, K/R/U decide on the shown item.
        .background {
            Group {
                Button("") { onClose() }.keyboardShortcut(.space, modifiers: [])
                Button("") { onClose() }.keyboardShortcut(.escape, modifiers: [])
                Button("") { onStep(-1) }.keyboardShortcut(.leftArrow, modifiers: [])
                Button("") { onStep(1) }.keyboardShortcut(.rightArrow, modifiers: [])
                Button("") { decide(.keep) }.keyboardShortcut("k", modifiers: [])
                Button("") { decide(.reject) }.keyboardShortcut("r", modifiers: [])
                Button("") { decide(.undecided) }.keyboardShortcut("u", modifiers: [])
                Button("") { state.infoOpen.toggle() }.keyboardShortcut("i", modifiers: [])
            }
            .opacity(0)
            .accessibilityHidden(true)
        }
    }

    @ViewBuilder
    private var content: some View {
        if failed {
            ContentUnavailableView {
                Label("This Video Can't Be Played Here", systemImage: "film")
            } description: {
                Text("Click Open in Default App to watch it in another app.")
            }
            .foregroundStyle(.white)
        } else if let playPath {
            PlayerView(url: URL(fileURLWithPath: playPath),
                       isMotion: item.kind == .photo,
                       onFailure: { failed = true })
                .id(playPath)
        } else {
            StillView(url: item.fileURL)
        }
    }

    @ViewBuilder
    private var badge: some View {
        switch item.decision {
        case .keep: Text("Keep").font(.caption).padding(.horizontal, 6).background(.green.opacity(0.3), in: Capsule())
        case .reject: Text("Reject").font(.caption).padding(.horizontal, 6).background(.red.opacity(0.3), in: Capsule())
        case .undecided: EmptyView()
        }
    }

    private func navButton(_ icon: String, action: @escaping () -> Void) -> some View {
        Button(action: action) { Image(systemName: icon).font(.title2).padding(8) }
            .buttonStyle(.plain)
            .foregroundStyle(.white)
            .background(.black.opacity(0.4), in: Circle())
    }

    private func decide(_ decision: Decision) {
        state.updateDecision(paths: [item.path], decision: decision)
    }
}

/// AVPlayerView with inline controls: autoplays muted (session choice), loops short clips.
struct PlayerView: NSViewRepresentable {
    let url: URL
    let isMotion: Bool
    let onFailure: () -> Void

    final class Coordinator: NSObject {
        var player: AVQueuePlayer?
        var looper: AVPlayerLooper?
        var observations: [NSKeyValueObservation] = []
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> AVPlayerView {
        let view = AVPlayerView()
        view.controlsStyle = .inline
        view.showsFullScreenToggleButton = true
        let item = AVPlayerItem(url: url)
        let player = AVQueuePlayer(playerItem: item)
        player.isMuted = ViewerSession.shared.muted
        let c = context.coordinator
        c.player = player
        c.observations.append(item.observe(\.status) { item, _ in
            DispatchQueue.main.async {
                switch item.status {
                case .readyToPlay:
                    let seconds = item.duration.seconds
                    if c.looper == nil, ViewerRules.shouldLoop(duration: seconds.isFinite ? seconds : 0, isMotion: isMotion) {
                        c.looper = AVPlayerLooper(player: player, templateItem: AVPlayerItem(url: url))
                    }
                case .failed: onFailure()
                default: break
                }
            }
        })
        c.observations.append(player.observe(\.isMuted) { p, _ in
            let muted = p.isMuted
            DispatchQueue.main.async { ViewerSession.shared.muted = muted }
        })
        view.player = player
        player.play()
        return view
    }

    func updateNSView(_ nsView: AVPlayerView, context: Context) {}

    static func dismantleNSView(_ nsView: AVPlayerView, coordinator: Coordinator) {
        coordinator.player?.pause()
        coordinator.observations.removeAll()
        coordinator.looper = nil
        nsView.player = nil
    }
}

/// "▶ 0:07" badge for video tiles; loads the duration asynchronously.
struct PlayBadge: View {
    let url: URL
    @State private var label = "▶"

    var body: some View {
        Text(label)
            .font(.caption2.monospacedDigit())
            .foregroundStyle(.white)
            .padding(.horizontal, 5)
            .padding(.vertical, 2)
            .background(.black.opacity(0.6), in: Capsule())
            .padding(4)
            .task(id: url) {
                if let d = try? await AVURLAsset(url: url).load(.duration), d.seconds.isFinite, d.seconds > 0 {
                    label = "▶ " + ViewerRules.formatDuration(d.seconds)
                }
            }
    }
}

/// A large still preview, scaled to fit.
struct StillView: View {
    let url: URL
    @State private var image: NSImage?

    var body: some View {
        Group {
            if let image {
                Image(nsImage: image).resizable().aspectRatio(contentMode: .fit)
            } else {
                ProgressView()
            }
        }
        .task(id: url) {
            image = nil
            let u = url
            image = await Task.detached { NSImage(contentsOf: u) }.value
        }
    }
}
