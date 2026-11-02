import CoreGraphics
import XCTest
import SamoKit
@testable import ShotsCore

private func image(_ width: Int, _ height: Int) throws -> CGImage {
    let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
    let context = try XCTUnwrap(CGContext(
        data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0, space: space,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ))
    context.setFillColor(CGColor(srgbRed: 0.2, green: 0.3, blue: 0.7, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    return try XCTUnwrap(context.makeImage())
}

/// Records what happened, in order.
private final class Log {
    var events: [String] = []
    var copied: Data?
}

private struct FakeClipboard: Clipboard {
    let log: Log
    var works = true
    func put(png: Data) -> Bool {
        log.events.append("copy")
        log.copied = png
        return works
    }
    func put(text: String) -> Bool {
        log.events.append("copy text")
        return works
    }
}

private final class FakeDisk: Disk {
    let log: Log
    var files: [URL: Data] = [:]
    var fails = false
    init(log: Log) { self.log = log }
    func makeFolder(_ url: URL) throws {}
    func exists(_ url: URL) -> Bool { files[url] != nil }
    func write(_ data: Data, to url: URL) throws {
        log.events.append("save")
        if fails { throw CocoaError(.fileWriteNoPermission) }
        files[url] = data
    }
    func replace(_ data: Data, at url: URL) throws {
        log.events.append("replace")
        if fails { throw CocoaError(.fileWriteNoPermission) }
        files[url] = data
    }
}

final class CaptureTests: XCTestCase {
    func testSelectionMath() {
        // Drags in any direction give the same rectangle.
        let r = Selection.rect(from: CGPoint(x: 100, y: 80), to: CGPoint(x: 40, y: 20), square: false)
        XCTAssertEqual(r, CGRect(x: 40, y: 20, width: 60, height: 60))
        XCTAssertEqual(Selection.rect(from: CGPoint(x: 10, y: 10), to: CGPoint(x: 70, y: 30), square: false),
                       CGRect(x: 10, y: 10, width: 60, height: 20))
        // ⇧ keeps a square, growing from the start the way the drag went.
        XCTAssertEqual(Selection.rect(from: CGPoint(x: 100, y: 100), to: CGPoint(x: 40, y: 80), square: true),
                       CGRect(x: 80, y: 80, width: 20, height: 20))
        // Within the display.
        XCTAssertEqual(Selection.clamp(CGRect(x: -10, y: -10, width: 50, height: 50),
                                       to: CGRect(x: 0, y: 0, width: 30, height: 30)),
                       CGRect(x: 0, y: 0, width: 30, height: 30))
        // Points, origin bottom-left, to a Retina image's pixels, origin top-left.
        XCTAssertEqual(Selection.imageRect(CGRect(x: 10, y: 700, width: 100, height: 50), viewHeight: 800, scale: 2),
                       CGRect(x: 20, y: 100, width: 200, height: 100))
        // Arrows nudge by one point, ⇧ by ten.
        XCTAssertEqual(Selection.nudge(CGPoint(x: 5, y: 5), .right, big: false), CGPoint(x: 6, y: 5))
        XCTAssertEqual(Selection.nudge(CGPoint(x: 5, y: 5), .down, big: true), CGPoint(x: 5, y: -5))
        // Moving a selection keeps it whole on the display.
        XCTAssertEqual(Selection.moved(CGRect(x: 10, y: 10, width: 20, height: 20), by: CGVector(dx: 100, dy: -50),
                                       within: CGRect(x: 0, y: 0, width: 100, height: 100)),
                       CGRect(x: 80, y: 0, width: 20, height: 20))
    }

    func testFileNaming() {
        let date = Date(timeIntervalSince1970: 1_793_543_712)  // 2026-11-01 14:35:12 UTC
        let utc = TimeZone(identifier: "UTC") ?? .current
        XCTAssertEqual(FileNaming.name(at: date, format: .png, timeZone: utc), "Shot 2026-11-01 at 14.35.12.png")
        XCTAssertEqual(FileNaming.name(at: date, format: .jpeg, timeZone: utc), "Shot 2026-11-01 at 14.35.12.jpg")
        let folder = URL(fileURLWithPath: "/tmp/shots")
        let taken: Set<String> = ["/tmp/shots/a.png", "/tmp/shots/a (2).png"]
        XCTAssertEqual(FileNaming.unique("b.png", in: folder, exists: { taken.contains($0.path) }).lastPathComponent, "b.png")
        XCTAssertEqual(FileNaming.unique("a.png", in: folder, exists: { taken.contains($0.path) }).lastPathComponent,
                       "a (3).png")
    }

    func testCaptureActionRunsBeforeWindows() throws {
        let log = Log()
        let shot = Shot(image: try image(40, 20), scale: 2, takenAt: Date())
        var settings = Settings()
        settings.after = .both
        let disk = FakeDisk(log: log)
        let outcome = AfterCapture.run(shot, settings: settings, clipboard: FakeClipboard(log: log), disk: disk,
                                       open: { _, then in log.events.append(then.rawValue) })
        // Copied and saved first; only then does the editor open (the default).
        XCTAssertEqual(log.events, ["copy", "save", "edit"])
        XCTAssertEqual(outcome.note, "Copied and saved")
        XCTAssertEqual(shot.savedTo, outcome.savedTo)
        XCTAssertGreaterThan(outcome.bytes, 0)

        // Pin: the pin opens last.
        log.events.removeAll()
        settings.after = .copy
        settings.then = .pin
        let pinned = AfterCapture.run(shot, settings: settings, clipboard: FakeClipboard(log: log), disk: disk,
                                      open: { _, then in log.events.append(then.rawValue) })
        XCTAssertEqual(log.events, ["copy", "pin"])
        XCTAssertEqual(pinned.note, "Copied")
        // Nothing: nothing opens.
        log.events.removeAll()
        settings.then = .nothing
        _ = AfterCapture.run(shot, settings: settings, clipboard: FakeClipboard(log: log), disk: disk,
                             open: { _, then in log.events.append(then.rawValue) })
        XCTAssertEqual(log.events, ["copy"])
    }

    func testEditorOpensAfterCopyAndSave() throws {
        let log = Log()
        let shot = Shot(image: try image(40, 20), scale: 2, takenAt: Date())
        let disk = FakeDisk(log: log)
        var opened: Shot?
        let outcome = AfterCapture.run(shot, settings: Settings(), clipboard: FakeClipboard(log: log), disk: disk,
                                       open: { shot, then in
                                           // By the time the editor opens, the shot is on the clipboard and on disk.
                                           XCTAssertEqual(log.events, ["copy", "save"])
                                           XCTAssertNotNil(shot.savedTo)
                                           XCTAssertEqual(then, .edit)
                                           opened = shot
                                       })
        XCTAssertTrue(opened === shot)
        XCTAssertEqual(outcome.then, .edit)
        // Even when the save fails, the editor opens with the shot, so it isn't lost.
        disk.fails = true
        opened = nil
        log.events.removeAll()
        let failed = AfterCapture.run(shot, settings: Settings(), clipboard: FakeClipboard(log: log), disk: disk,
                                      open: { shot, _ in opened = shot })
        XCTAssertTrue(opened === shot)
        XCTAssertFalse(failed.problems.isEmpty)
    }

    func testCopyAndSaveShareOneEncode() throws {
        let log = Log()
        let shot = Shot(image: try image(64, 48), scale: 2, takenAt: Date())
        let disk = FakeDisk(log: log)
        let outcome = AfterCapture.run(shot, settings: Settings(), clipboard: FakeClipboard(log: log), disk: disk,
                                       open: { _, _ in })
        // Both PNG: the clipboard and the file get the very same bytes.
        let saved = try XCTUnwrap(outcome.savedTo.flatMap { disk.files[$0] })
        XCTAssertEqual(log.copied, saved)
        // JPEG on disk is its own encode.
        var settings = Settings()
        settings.format = .jpeg
        let jpeg = AfterCapture.run(shot, settings: settings, clipboard: FakeClipboard(log: log), disk: disk,
                                    open: { _, _ in })
        let file = try XCTUnwrap(jpeg.savedTo.flatMap { disk.files[$0] })
        XCTAssertNotEqual(log.copied, file)
        XCTAssertEqual(jpeg.savedTo?.pathExtension, "jpg")
    }

    func testFailedSaveIsShown() throws {
        let log = Log()
        let shot = Shot(image: try image(10, 10), scale: 1, takenAt: Date())
        let disk = FakeDisk(log: log)
        disk.fails = true
        let outcome = AfterCapture.run(shot, settings: Settings(), clipboard: FakeClipboard(log: log, works: false),
                                       disk: disk, open: { _, _ in })
        XCTAssertFalse(outcome.copied)
        XCTAssertNil(outcome.savedTo)
        XCTAssertEqual(outcome.problems.count, 2)
        XCTAssertEqual(outcome.note, "Couldn't copy the shot")
        XCTAssertTrue(outcome.problems[1].hasPrefix("Couldn't save the shot to ~/Pictures/Shots"), outcome.problems[1])
    }

    func testRetinaToOneX() throws {
        let log = Log()
        let shot = Shot(image: try image(200, 100), scale: 2, takenAt: Date())
        var settings = Settings()
        settings.after = .save
        settings.retina = false
        let disk = FakeDisk(log: log)
        let outcome = AfterCapture.run(shot, settings: settings, clipboard: FakeClipboard(log: log), disk: disk,
                                       open: { _, _ in })
        let saved = try XCTUnwrap(outcome.savedTo.flatMap { disk.files[$0] })
        let size = try XCTUnwrap(Encoding.size(of: saved))
        XCTAssertEqual(size.width, 100)
        XCTAssertEqual(size.height, 50)
        // At Retina, as captured.
        settings.retina = true
        let full = AfterCapture.run(shot, settings: settings, clipboard: FakeClipboard(log: log), disk: disk,
                                    open: { _, _ in })
        let fullSize = try XCTUnwrap(full.savedTo.flatMap { disk.files[$0] }.flatMap(Encoding.size(of:)))
        XCTAssertEqual(fullSize.width, 200)
    }
}

final class WatchdogTests: XCTestCase {
    func testAnAnswerInTimeIsTaken() {
        let dog = Watchdog()
        let token = dog.begin()
        XCTAssertTrue(dog.isWaiting)
        XCTAssertTrue(dog.answered(token))
        XCTAssertFalse(dog.expired(token), "answered, so the limit passing changes nothing")
        XCTAssertFalse(dog.isWaiting)
    }

    func testALateAnswerIsIgnored() {
        let dog = Watchdog()
        let token = dog.begin()
        XCTAssertTrue(dog.expired(token), "nothing came: give up")
        XCTAssertFalse(dog.answered(token), "and an answer after that is ignored")
        // The next capture waits afresh; the old limit can't end it.
        let next = dog.begin()
        XCTAssertFalse(dog.expired(token))
        XCTAssertTrue(dog.isWaiting)
        XCTAssertTrue(dog.answered(next))
    }
}
