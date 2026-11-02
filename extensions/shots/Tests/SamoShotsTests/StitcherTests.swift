import CoreGraphics
import XCTest
@testable import ShotsCore

/// A synthetic web page, rows from the top: bands of colour, sparse dark dots unique to every row (like text), and a
/// blank gap in the middle.
private struct Page {
    var width = 400
    let height: Int
    let header = 50
    let footer = 30
    var view = 520

    /// The page's own pixel at `x`, `y` (rows from the top), as RGB.
    func pixel(_ x: Int, _ y: Int) -> (UInt8, UInt8, UInt8) {
        if y >= 900, y < 960 { return (255, 255, 255) }  // a blank gap
        var h = UInt32(truncatingIfNeeded: x &* 73_856_093 ^ y &* 19_349_663)
        h ^= h >> 13
        h = h &* 0x5bd1_e995
        h ^= h >> 15
        if h % 29 == 0 { return (20, 20, 30) }
        let band = UInt8((y / 40) % 6)
        return (230 &- band &* 20, 235, 240 &- band &* 9)
    }

    /// Rows `y` of the header (dark, with a light logo) and footer (mid grey with a button), the same in every frame.
    func headerPixel(_ x: Int, _ y: Int) -> (UInt8, UInt8, UInt8) {
        (x > 20 && x < 120 && y > 15 && y < 35) ? (240, 240, 240) : (36, 42, 51)
    }

    func footerPixel(_ x: Int, _ y: Int) -> (UInt8, UInt8, UInt8) {
        (x > 280 && x < 380 && y > 6 && y < 24) ? (36, 62, 181) : (200, 204, 210)
    }

    func image(rows: Int, _ colour: (Int, Int) -> (UInt8, UInt8, UInt8)) throws -> CGImage {
        var bytes = [UInt8](repeating: 255, count: width * rows * 4)
        for y in 0..<rows {
            for x in 0..<width {
                let (r, g, b) = colour(x, y)
                let i = (y * width + x) * 4
                bytes[i] = r
                bytes[i + 1] = g
                bytes[i + 2] = b
            }
        }
        let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
        let provider = try XCTUnwrap(CGDataProvider(data: Data(bytes) as CFData))
        return try XCTUnwrap(CGImage(
            width: width, height: rows, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4, space: space,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue), provider: provider,
            decode: nil, shouldInterpolate: false, intent: .defaultIntent
        ))
    }

    /// What the area shows with the page scrolled to `offset`: the sticky header, the page, the sticky footer.
    func frame(at offset: Int, sticky: Bool = true) throws -> ScrollFrame {
        let rows = sticky ? header + view + footer : view
        let image = try image(rows: rows) { x, y in
            guard sticky else { return pixel(x, offset + y) }
            if y < header { return headerPixel(x, y) }
            if y >= header + view { return footerPixel(x, y - header - view) }
            return pixel(x, offset + y - header)
        }
        return try XCTUnwrap(ScrollFrame(image))
    }

    /// The whole page as one capture should come out: the header once, every row once, the footer once.
    func expected() throws -> CGImage {
        try image(rows: header + height + footer) { x, y in
            if y < header { return headerPixel(x, y) }
            if y >= header + height { return footerPixel(x, y - header - height) }
            return pixel(x, y - header)
        }
    }
}

final class StitcherTests: XCTestCase {
    func testPageThreeScreensLongStitchesExactly() throws {
        // Three screens and a bit: 1,880 rows of page under a 520-row view.
        let page = Page(height: 1880)
        let stitcher = Stitcher(first: try page.frame(at: 0))
        // Uneven scrolls by hand: a pause, a tiny move, a scroll back, big moves, and to the very end.
        let offsets = [0, 23, 24, 140, 140, 310, 290, 520, 760, 1000, 1180, 1360]
        var steps: [Stitcher.Step] = []
        for offset in offsets {
            steps.append(stitcher.add(try page.frame(at: offset)))
        }
        XCTAssertEqual(steps, [.unchanged, .added(23), .added(1), .added(116), .unchanged, .added(170),
                               .scrolledBack, .added(210), .added(240), .added(240), .added(180), .added(180)])
        // No seams, no repeats: the header once, the page row for row, the footer once.
        let image = try XCTUnwrap(stitcher.image())
        XCTAssertEqual(image.height, page.header + page.height + page.footer)
        XCTAssertTrue(try Pixels(image).changed(from: Pixels(page.expected()), tolerance: 0).isNull,
                      "the stitched page differs from the page")
    }

    func testWithoutStickyBandsToo() throws {
        let page = Page(height: 1400)
        let stitcher = Stitcher(first: try page.frame(at: 0, sticky: false))
        for offset in [100, 330, 600, 880] {
            XCTAssertNotEqual(stitcher.add(try page.frame(at: offset, sticky: false)), .lost, "\(offset)")
        }
        let image = try XCTUnwrap(stitcher.image())
        XCTAssertEqual(image.height, 880 + page.view)
        let expected = try page.image(rows: 880 + page.view) { x, y in page.pixel(x, y) }
        XCTAssertTrue(try Pixels(image).changed(from: Pixels(expected), tolerance: 0).isNull)
    }

    func testAJumpStopsAndKeepsWhatItHas() throws {
        let page = Page(height: 1880)
        let stitcher = Stitcher(first: try page.frame(at: 0))
        XCTAssertEqual(stitcher.add(try page.frame(at: 200)), .added(200))
        // The page jumped far below: nothing overlaps.
        XCTAssertEqual(stitcher.add(try page.frame(at: 1300)), .lost)
        let image = try XCTUnwrap(stitcher.image())
        XCTAssertEqual(image.height, page.header + 200 + page.view + page.footer)
    }

    func testBlankPagesDontRepeat() throws {
        let page = Page(height: 1880)
        let blank = try XCTUnwrap(ScrollFrame(page.image(rows: 600) { _, _ in (255, 255, 255) }))
        let stitcher = Stitcher(first: blank)
        // Scrolling a blank page shows the same frame: nothing to add, nothing repeated.
        XCTAssertEqual(stitcher.add(blank), .unchanged)
        XCTAssertEqual(stitcher.height, 600)
        // The blank gap scrolled past keeps its place in the page, matched by the rows with detail.
        let gap = Stitcher(first: try page.frame(at: 700))
        XCTAssertEqual(gap.add(try page.frame(at: 760)), .added(60))
    }

    func testCapStopsIt() throws {
        let page = Page(height: 1880)
        let stitcher = Stitcher(first: try page.frame(at: 0), maxHeight: 900)
        XCTAssertEqual(stitcher.add(try page.frame(at: 200)), .added(200))
        XCTAssertEqual(stitcher.add(try page.frame(at: 400)), .full)
        XCTAssertTrue(stitcher.finished)
        XCTAssertEqual(stitcher.add(try page.frame(at: 600)), .full)
        XCTAssertEqual(try XCTUnwrap(stitcher.image()).height, 900)
        // Frames of another size never fit.
        let other = Stitcher(first: try page.frame(at: 0))
        XCTAssertEqual(other.add(try page.frame(at: 10, sticky: false)), .lost)
    }

    func testStitchingIsQuickEnough() throws {
        // A Retina-sized area, 1,600 × 1,200 pixels: fingerprinting and matching a frame fit in a frame's time at
        // 20 a second.
        let page = Page(width: 1600, height: 3000, view: 1120)
        let first = try page.frame(at: 0)
        let image = try page.image(rows: 1200) { x, y in
            y < page.header ? page.headerPixel(x, y)
                : y >= page.header + page.view ? page.footerPixel(x, y - page.header - page.view)
                : page.pixel(x, 90 + y - page.header)
        }
        let stitcher = Stitcher(first: first)
        let start = Date()
        let next = try XCTUnwrap(ScrollFrame(image))
        let printed = Date()
        XCTAssertEqual(stitcher.add(next), .added(90))
        let done = Date()
        print("shots: fingerprints \(Int(printed.timeIntervalSince(start) * 1000)) ms, match \(Int(done.timeIntervalSince(printed) * 1000)) ms")
        #if DEBUG
        // Unoptimised test builds are about fifty times slower; the helper Samo runs is a release build.
        XCTAssertLessThan(done.timeIntervalSince(start), 1)
        #else
        XCTAssertLessThan(done.timeIntervalSince(start), 0.05)
        #endif
    }
}
