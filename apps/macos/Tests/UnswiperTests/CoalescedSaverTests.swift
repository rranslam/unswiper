// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import XCTest
@testable import Unswiper

/// Thread-safe list of what the saver ran.
private final class Log: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [Int] = []
    func add(_ v: Int) { lock.lock(); values.append(v); lock.unlock() }
    var all: [Int] { lock.lock(); defer { lock.unlock() }; return values }
}

final class CoalescedSaverTests: XCTestCase {
    func testOnlyTheNewestScheduledJobRuns() {
        let saver = CoalescedSaver(delay: 0.05)
        let log = Log()
        let done = expectation(description: "saved")
        for i in 1...20 {
            saver.schedule { log.add(i); if i == 20 { done.fulfill() } }
        }
        XCTAssertTrue(log.all.isEmpty, "nothing before the delay")
        wait(for: [done], timeout: 2)
        Thread.sleep(forTimeInterval: 0.1)
        XCTAssertEqual(log.all, [20])
        XCTAssertFalse(saver.hasPending)
    }

    func testRunNowWaitsAndDropsThePendingJob() {
        let saver = CoalescedSaver(delay: 0.1)
        let log = Log()
        saver.schedule { log.add(1) }
        XCTAssertTrue(saver.hasPending)
        saver.runNow { log.add(2) }
        XCTAssertEqual(log.all, [2], "runNow returns after its job ran")
        Thread.sleep(forTimeInterval: 0.3)
        XCTAssertEqual(log.all, [2], "the dropped job never runs")
    }

    func testJobsRunOneAtATimeOffTheCallingThread() {
        let saver = CoalescedSaver(delay: 0)
        let log = Log()
        let caller = Thread.current
        let done = expectation(description: "saved")
        saver.schedule {
            XCTAssertNotEqual(Thread.current, caller)
            Thread.sleep(forTimeInterval: 0.1) // a slow write
            log.add(1)
            done.fulfill()
        }
        Thread.sleep(forTimeInterval: 0.02) // let it start
        saver.runNow { log.add(2) } // waits behind the running job
        XCTAssertEqual(log.all, [1, 2])
        wait(for: [done], timeout: 1)
    }

    func testRunNowAsyncDoesNotBlock() async {
        let saver = CoalescedSaver(delay: 10)
        let log = Log()
        saver.schedule { log.add(1) }
        await saver.runNowAsync { log.add(2) }
        XCTAssertEqual(log.all, [2])
        XCTAssertFalse(saver.hasPending)
    }
}

final class ProgressTextTests: XCTestCase {
    func testTimeLeftAndEta() {
        XCTAssertEqual(ProgressText.timeLeft(10), "less than a minute left")
        XCTAssertEqual(ProgressText.timeLeft(8 * 60), "about 8 min left")
        XCTAssertEqual(ProgressText.timeLeft(125 * 60), "about 2 h 5 min left")
        XCTAssertNil(ProgressText.eta(done: 2, total: 100, elapsed: 10))
        XCTAssertNil(ProgressText.eta(done: 50, total: 100, elapsed: 1))
        XCTAssertEqual(ProgressText.eta(done: 50, total: 100, elapsed: 10) ?? 0, 10, accuracy: 0.001)
    }

    func testIndexPhases() {
        let finding = ProgressText.index(.init(filesSeen: 500, mediaFound: 400), elapsedDating: 0)
        XCTAssertNil(finding.0)
        XCTAssertTrue(finding.1.hasPrefix("Looking for photos and videos"))
        let dating = ProgressText.index(.init(filesSeen: 500, mediaFound: 400, dated: 100, toDate: 400), elapsedDating: 60)
        XCTAssertEqual(dating.0 ?? 0, 0.25, accuracy: 0.001)
        XCTAssertTrue(dating.1.contains("100 of 400"))
        XCTAssertTrue(dating.1.hasSuffix("about 3 min left"))
    }

    func testBlurText() {
        let r = ProgressText.blur(done: 50, total: 200, elapsed: 30, paused: false, label: " in this visit")
        XCTAssertEqual(r.0, 0.25, accuracy: 0.001)
        XCTAssertEqual(r.1, "Checking sharpness in this visit 50 of 200 · about 2 min left")
        XCTAssertEqual(ProgressText.blur(done: 50, total: 200, elapsed: 30, paused: true).1, "Paused · 50 of 200 checked")
    }

    func testBlurHelperProgressLines() {
        XCTAssertTrue(BlurHelper.parseProgress("progress 3 10")! == (3, 10))
        XCTAssertNil(BlurHelper.parseProgress("error: nope"))
        var seen: [Int] = []
        let lines = BlurHelper.StderrLines { d, _ in seen.append(d) }
        lines.feed(Data("progress 0 5\nwarn".utf8))
        lines.feed(Data("ing: x\nprogress 5 5\n".utf8))
        XCTAssertEqual(seen, [0, 5])
        XCTAssertEqual(lines.finish(), "warning: x\n")
        XCTAssertTrue(BlurHelper.arguments(library: URL(fileURLWithPath: "/l.json"), sensitivity: .normal,
                                           onlyList: nil, progress: true).contains("--progress"))
    }

    func testThrottle() {
        let t = ProgressThrottle(interval: 0.1)
        let now = Date()
        XCTAssertTrue(t.ready(now: now))
        XCTAssertFalse(t.ready(now: now.addingTimeInterval(0.05)))
        XCTAssertTrue(t.ready(final: true, now: now.addingTimeInterval(0.06)))
        XCTAssertTrue(t.ready(now: now.addingTimeInterval(0.2)))
    }
}
