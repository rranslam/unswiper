// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import Foundation

/// Runs save jobs on a background queue so writing library.json never blocks the window.
/// `schedule` waits `delay` and runs only the newest job (older pending ones are dropped);
/// `runNow` drops the pending job and runs right away, after any job that is running.
/// Jobs run one at a time, in order.
final class CoalescedSaver: @unchecked Sendable {
    private let queue = DispatchQueue(label: "unswiper.library-save", qos: .utility)
    private let lock = NSLock()
    private var pending: (() -> Void)?
    private var generation = 0
    var delay: TimeInterval

    init(delay: TimeInterval) {
        self.delay = delay
    }

    /// Runs `job` after the delay unless a newer job or `runNow` comes first.
    func schedule(_ job: @escaping () -> Void) {
        lock.lock()
        generation += 1
        let g = generation
        pending = job
        lock.unlock()
        queue.asyncAfter(deadline: .now() + delay) { [weak self] in
            self?.runPending(ifGeneration: g)
        }
    }

    private func runPending(ifGeneration g: Int) {
        lock.lock()
        guard g == generation, let job = pending else {
            lock.unlock()
            return
        }
        pending = nil
        lock.unlock()
        job()
    }

    /// Drops the pending job without running it.
    func cancelPending() {
        lock.lock()
        generation += 1
        pending = nil
        lock.unlock()
    }

    /// True while a scheduled job is waiting for its delay.
    var hasPending: Bool {
        lock.lock()
        defer { lock.unlock() }
        return pending != nil
    }

    /// Runs `job` now on the save queue and waits for it (quit, tests, before a helper reads
    /// library.json). Blocks the caller.
    func runNow(_ job: @escaping () -> Void) {
        cancelPending()
        queue.sync(execute: job)
    }

    /// Like `runNow`, without blocking the caller's thread.
    func runNowAsync(_ job: @escaping () -> Void) async {
        cancelPending()
        await withCheckedContinuation { (c: CheckedContinuation<Void, Never>) in
            queue.async {
                job()
                c.resume()
            }
        }
    }

    /// Waits until jobs already running or queued have finished.
    func waitUntilIdle() {
        queue.sync {}
    }
}
