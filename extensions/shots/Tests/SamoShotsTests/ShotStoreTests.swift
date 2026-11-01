import CoreGraphics
import XCTest
@testable import ShotsCore

private final class MemoryDisk: Disk {
    var files: [URL: Data] = [:]
    func makeFolder(_ url: URL) throws {}
    func exists(_ url: URL) -> Bool { files[url] != nil }
    func write(_ data: Data, to url: URL) throws { files[url] = data }
    func replace(_ data: Data, at url: URL) throws { files[url] = data }
}

private func image() throws -> CGImage {
    let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
    let context = try XCTUnwrap(CGContext(
        data: nil, width: 8, height: 8, bitsPerComponent: 8, bytesPerRow: 0, space: space,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ))
    return try XCTUnwrap(context.makeImage())
}

final class ShotStoreTests: XCTestCase {
    func testShotReleasedWhenWindowsClose() throws {
        let store = ShotStore()
        weak var gone: Shot?
        do {
            let shot = Shot(image: try image(), scale: 2, takenAt: Date())
            gone = shot
            store.retain(shot)  // a pin
            store.retain(shot)  // and an editor
            store.release(shot)
        }
        XCTAssertNotNil(gone, "one window still shows it")
        XCTAssertEqual(store.live, 1)
        if let shot = gone { store.release(shot) }
        XCTAssertNil(gone, "the last window closed, so the image is freed")
        XCTAssertEqual(store.live, 0)
    }

    func testShutdownSavesUnsaved() throws {
        let store = ShotStore()
        let pinnedOnly = Shot(image: try image(), scale: 2, takenAt: Date(timeIntervalSince1970: 1_793_543_712))
        let saved = Shot(image: try image(), scale: 2, takenAt: Date())
        saved.savedTo = URL(fileURLWithPath: "/tmp/already.png")
        store.retain(pinnedOnly)
        store.retain(saved)
        let disk = MemoryDisk()
        let written = store.saveUnsaved(settings: Settings(), disk: disk)
        XCTAssertEqual(written.count, 1, "only the shot never saved")
        XCTAssertTrue(written[0].lastPathComponent.hasSuffix(" (unsaved).png"), written[0].lastPathComponent)
        XCTAssertEqual(pinnedOnly.savedTo, written[0])
        XCTAssertNotNil(disk.files[written[0]])
        // Saving again writes nothing more.
        XCTAssertTrue(store.saveUnsaved(settings: Settings(), disk: disk).isEmpty)
    }

    func testOpacityStaysInRange() {
        XCTAssertEqual(PinMath.opacity(1, scrolledBy: 10), 1)
        XCTAssertEqual(PinMath.opacity(1, scrolledBy: -10), 0.8, accuracy: 0.0001)
        XCTAssertEqual(PinMath.opacity(0.3, scrolledBy: -100), 0.2)
        // A Retina shot pins at its size in points, shrunk to fit the screen.
        XCTAssertEqual(PinMath.size(pixels: CGSize(width: 800, height: 400), scale: 2, screen: CGSize(width: 1440, height: 900)),
                       CGSize(width: 400, height: 200))
        XCTAssertEqual(PinMath.size(pixels: CGSize(width: 5120, height: 2880), scale: 2, screen: CGSize(width: 1440, height: 900)),
                       CGSize(width: 1152, height: 648))
    }
}
