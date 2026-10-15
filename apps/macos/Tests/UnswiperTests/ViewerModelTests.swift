// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

import XCTest
@testable import Unswiper

final class ViewerModelTests: XCTestCase {
    let paths = ["/a.jpg", "/b.mov", "/c.heic"]

    func testOpenStepNoWrapAndPosition() {
        var m = ViewerModel()
        m.open("/a.jpg", in: paths)
        XCTAssertEqual(m.position(in: paths)?.0, 1)
        XCTAssertFalse(m.step(-1, in: paths))
        XCTAssertTrue(m.step(1, in: paths))
        XCTAssertEqual(m.current, "/b.mov")
        XCTAssertTrue(m.step(1, in: paths))
        XCTAssertFalse(m.step(1, in: paths))
        XCTAssertEqual(m.position(in: paths)?.0, 3)
        XCTAssertEqual(m.position(in: paths)?.1, 3)
        m.close()
        XCTAssertFalse(m.isOpen)
    }

    func testOpenOnSelectionUsesGridOrderAndIgnoresHidden() {
        var m = ViewerModel()
        m.openOnSelection(["/c.heic", "/b.mov"], in: paths)
        XCTAssertEqual(m.current, "/b.mov")
        m.openOnSelection(["/hidden.jpg"], in: paths)
        XCTAssertNil(m.current)
        m.open("/hidden.jpg", in: paths)
        XCTAssertNil(m.current)
    }

    func testClosesWhenItemFilteredOut() {
        var m = ViewerModel()
        m.open("/b.mov", in: paths)
        m.check(["/b.mov"])
        XCTAssertEqual(m.current, "/b.mov")
        m.check(["/a.jpg"])
        XCTAssertNil(m.current)
    }

    func testMotionClipAndPlayablePath() {
        XCTAssertEqual(ViewerRules.motionClip(kind: .photo, sidecarPaths: ["/x.AAE", "/x.MOV"]), "/x.MOV")
        XCTAssertNil(ViewerRules.motionClip(kind: .photo, sidecarPaths: ["/x.xmp"]))
        XCTAssertNil(ViewerRules.motionClip(kind: .video, sidecarPaths: ["/x.mov"]))
        XCTAssertEqual(ViewerRules.playablePath(kind: .video, path: "/v.mp4", sidecarPaths: [], liveMotion: false), "/v.mp4")
        XCTAssertNil(ViewerRules.playablePath(kind: .photo, path: "/x.heic", sidecarPaths: ["/x.mov"], liveMotion: false))
        XCTAssertEqual(ViewerRules.playablePath(kind: .photo, path: "/x.heic", sidecarPaths: ["/x.mov"], liveMotion: true), "/x.mov")
    }

    func testLoopAndDuration() {
        XCTAssertTrue(ViewerRules.shouldLoop(duration: 3, isMotion: false))
        XCTAssertFalse(ViewerRules.shouldLoop(duration: 10, isMotion: false))
        XCTAssertFalse(ViewerRules.shouldLoop(duration: 0, isMotion: false))
        XCTAssertTrue(ViewerRules.shouldLoop(duration: 30, isMotion: true))
        XCTAssertEqual(ViewerRules.formatDuration(7), "0:07")
        XCTAssertEqual(ViewerRules.formatDuration(65), "1:05")
        XCTAssertEqual(ViewerRules.formatDuration(3723), "1:02:03")
    }
}
