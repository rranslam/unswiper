// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import AppKit
import Foundation

/// State of the "Import from Google Photos" assistant: the step, the two folders and the
/// options are remembered between launches (the user may wait days for Google's email).
@MainActor
final class TakeoutSetupModel: ObservableObject {
    /// What is remembered (UserDefaults, key `takeoutSetup`).
    struct Saved: Codable, Equatable {
        var step = 0
        var archivesPath: String?
        var libraryPath: String?
        var options = TakeoutHelper.Options()
    }

    static let defaultsKey = "takeoutSetup"
    static let windowID = "takeout"

    let guide: TakeoutGuide
    private let defaults: UserDefaults
    private let helper: () -> URL?

    @Published var saved: Saved { didSet { if saved != oldValue { persist() } } }
    @Published private(set) var checking = false
    @Published private(set) var planEvent: TakeoutHelper.Event?
    @Published private(set) var running: TakeoutRun?
    @Published private(set) var progress: TakeoutHelper.Progress?
    @Published private(set) var progressText = ""
    @Published private(set) var fraction = 0.0
    @Published private(set) var reportLines: [String]?
    @Published private(set) var problem: String?
    private var started = Date()

    init(guide: TakeoutGuide = .shared, defaults: UserDefaults = .standard,
         helper: @escaping () -> URL? = { BlurHelper.locate() }) {
        self.guide = guide
        self.defaults = defaults
        self.helper = helper
        var s = defaults.data(forKey: Self.defaultsKey).flatMap { try? JSONDecoder().decode(Saved.self, from: $0) } ?? Saved()
        s.step = min(max(s.step, 0), max(guide.steps.count - 1, 0))
        self.saved = s
    }

    private func persist() {
        if let d = try? JSONEncoder().encode(saved) { defaults.set(d, forKey: Self.defaultsKey) }
    }

    var step: TakeoutGuide.Step { guide.steps[saved.step] }
    var stepLabel: String { "Step \(saved.step + 1) of \(guide.steps.count)" }
    var archivesURL: URL? { saved.archivesPath.map { URL(fileURLWithPath: $0, isDirectory: true) } }
    var libraryURL: URL? { saved.libraryPath.map { URL(fileURLWithPath: $0, isDirectory: true) } }
    var folderProblem: String? { TakeoutHelper.folderProblem(archives: archivesURL, library: libraryURL) }
    var isRunning: Bool { running != nil }

    /// The dry-run figures in words, or why there are none.
    var figures: [String] {
        if checking { return ["Checking the archives…"] }
        if let p = folderProblem { return [p] }
        switch planEvent {
        case .plan(_, _, _, let lines)?: return lines
        case .error(_, let message)?: return [message]
        default: return []
        }
    }

    var fits: Bool {
        if case .plan(_, let fits, _, _)? = planEvent { return fits }
        return false
    }

    /// Unpack needs a dry run that fits: the free space must be known and enough (a full export
    /// can be huge). The helper refuses too (`space_unknown`, `not_enough_space`).
    var canUnpack: Bool { !isRunning && !checking && folderProblem == nil && fits }

    var canGoNext: Bool {
        guard saved.step + 1 < guide.steps.count else { return false }
        switch step.action {
        case "choose_folders": return !checking && fits
        case "unpack": return !isRunning && reportLines != nil
        default: return true
        }
    }

    func go(_ delta: Int) {
        saved.step = min(max(saved.step + delta, 0), guide.steps.count - 1)
        if step.action == "choose_folders", planEvent == nil { check() }
    }

    func setArchives(_ url: URL) {
        saved.archivesPath = url.path
        planEvent = nil
        check()
    }

    func setLibrary(_ url: URL) {
        saved.libraryPath = url.path
        planEvent = nil
        check()
    }

    func setOptions(_ o: TakeoutHelper.Options) {
        guard o != saved.options else { return }
        saved.options = o
        planEvent = nil
        if !isRunning { check() }
    }

    /// Asks the user for a folder (blocking panel, like Choose Folder).
    func chooseFolder(archives: Bool) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.message = archives
            ? "Choose the folder with the downloaded Google Takeout archives"
            : "Choose or create the folder for your Google Photos library"
        panel.prompt = "Choose"
        if let current = archives ? archivesURL : libraryURL { panel.directoryURL = current }
        guard panel.runModal() == .OK, let url = panel.url else { return }
        if archives { setArchives(url) } else { setLibrary(url) }
    }

    /// The dry run: sizes, counts, space and time, without writing anything.
    func check() {
        guard !checking, !isRunning, folderProblem == nil,
              let archives = archivesURL, let library = libraryURL else { return }
        guard let helper = helper() else {
            planEvent = .error(kind: "helper", message: "The import helper is missing from Unswiper.app. Please reinstall Unswiper.")
            return
        }
        checking = true
        let options = saved.options
        Task {
            let ev: TakeoutHelper.Event
            do {
                ev = try await Task.detached(priority: .userInitiated) {
                    try TakeoutHelper.dryRun(helper: helper, archives: archives, library: library, options: options)
                }.value
            } catch {
                ev = .error(kind: "helper", message: error.localizedDescription)
            }
            checking = false
            if case .error(let kind, _) = ev, kind == "no_archives" {
                planEvent = .error(kind: kind, message: "No .zip or .tgz files in the archives folder. Download the parts from Google's email into this folder (step 5), or choose the folder you saved them in.")
            } else {
                planEvent = ev
            }
        }
    }

    func unpack() {
        guard canUnpack, let archives = archivesURL, let library = libraryURL else { return }
        guard let helper = helper() else {
            problem = "The import helper is missing from Unswiper.app. Please reinstall Unswiper."
            return
        }
        problem = nil
        reportLines = nil
        progress = nil
        fraction = 0
        progressText = "Starting…"
        started = Date()
        let args = TakeoutHelper.importArguments(archives: archives, library: library, options: saved.options, dryRun: false)
        do {
            running = try TakeoutRun(helper: helper, arguments: args, onEvent: { [weak self] ev in
                self?.handle(ev)
            }, onExit: { [weak self] code in
                self?.finished(code)
            })
        } catch {
            problem = "Couldn't start the import: \(error.localizedDescription)"
        }
    }

    private var stopped = false

    func stop() {
        guard let running else { return }
        stopped = true
        running.stop()
    }

    func handle(_ ev: TakeoutHelper.Event) {
        switch ev {
        case .plan:
            planEvent = ev
        case .progress(let p):
            progress = p
            (fraction, progressText) = TakeoutHelper.progressText(p, elapsed: Date().timeIntervalSince(started))
        case .done(_, let lines):
            fraction = 1
            reportLines = lines
        case .error(let kind, let message):
            problem = kind == "cancelled" ? "Stopped. Click Continue Unpacking to continue where it stopped." : message
        }
    }

    func finished(_ code: Int32) {
        running = nil
        if reportLines == nil, problem == nil {
            problem = stopped
                ? "Stopped. Click Continue Unpacking to continue where it stopped."
                : "The import stopped unexpectedly (exit code \(code)). Click Continue Unpacking to try again."
        }
        stopped = false
        // Fresh figures (the free space changed); Continue Unpacking waits for them.
        planEvent = nil
        if folderProblem == nil { check() }
    }

    /// Last step: the assistant reopens on "Choose the folders" next time (for the next export).
    func finish() -> URL? {
        saved.step = guide.steps.firstIndex { $0.action == "choose_folders" } ?? 0
        return libraryURL
    }
}
