import CoreGraphics
import XCTest
import SamoKit
@testable import ShotsCore

/// A button on a white page, with rows from the top: the button at `button`, its label block at `label`, and
/// optionally a soft (anti-aliased) one-pixel edge around both and faint noise inside.
private func buttonPage(button: CGRect, label: CGRect, soft: Bool = false) throws -> CGImage {
    let (width, height) = (300, 200)
    let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
    let context = try XCTUnwrap(CGContext(
        data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0, space: space,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ))
    // Rows from the top, as the tests read them.
    func top(_ r: CGRect) -> CGRect {
        CGRect(x: r.minX, y: CGFloat(height) - r.maxY, width: r.width, height: r.height)
    }
    context.setFillColor(CGColor(srgbRed: 1, green: 1, blue: 1, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    let blue = CGColor(srgbRed: 0x24 / 255.0, green: 0x3E / 255.0, blue: 0xB5 / 255.0, alpha: 1)
    if soft {
        // Half-way pixels around the button and the label.
        context.setFillColor(CGColor(srgbRed: 0.57, green: 0.62, blue: 0.85, alpha: 1))
        context.fill(top(button.insetBy(dx: -1, dy: -1)))
    }
    context.setFillColor(blue)
    context.fill(top(button))
    if soft {
        context.setFillColor(CGColor(srgbRed: 0.07, green: 0.12, blue: 0.35, alpha: 1))
        context.fill(top(label.insetBy(dx: -1, dy: -1)))
        // Faint noise in the button: within the tolerance.
        context.setFillColor(CGColor(srgbRed: 0x27 / 255.0, green: 0x3B / 255.0, blue: 0xB8 / 255.0, alpha: 1))
        for x in stride(from: Int(button.minX) + 3, to: Int(button.maxX) - 3, by: 7) {
            context.fill(top(CGRect(x: x, y: Int(button.minY) + 3, width: 1, height: 1)))
        }
    }
    context.setFillColor(CGColor(srgbRed: 0, green: 0, blue: 0, alpha: 1))
    context.fill(top(label))
    return try XCTUnwrap(context.makeImage())
}

final class MeasureTests: XCTestCase {
    func testHexIsSRGBFromP3() throws {
        // A Display P3 shot (as a wide-gamut Mac captures) of what a page drew as #F15B2A.
        let p3 = try XCTUnwrap(CGColorSpace(name: CGColorSpace.displayP3))
        let context = try XCTUnwrap(CGContext(
            data: nil, width: 4, height: 4, bitsPerComponent: 8, bytesPerRow: 0, space: p3,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ))
        context.setFillColor(CGColor(srgbRed: 0xF1 / 255.0, green: 0x5B / 255.0, blue: 0x2A / 255.0, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: 4, height: 4))
        let pixel = try XCTUnwrap(PixelGrid(XCTUnwrap(context.makeImage()))?.pixel(1, 1))
        // Within one step on each channel (P3 at 8 bits rounds).
        XCTAssertTrue(pixel.near(Pixel(r: 0xF1, g: 0x5B, b: 0x2A), tolerance: 1), pixel.hex)
        XCTAssertEqual(Pixel(r: 0xF1, g: 0x5B, b: 0x2A).hex, "#F15B2A")
        XCTAssertEqual(Pixel(r: 241, g: 91, b: 42).css, "rgb(241, 91, 42)")
        XCTAssertEqual(Pixel(r: 0, g: 10, b: 255).hex, "#000AFF")
    }

    func testPointToPixelOnRetina() {
        let size = (width: 400, height: 200)
        // Points, origin bottom-left, on a display 100 points tall at 2×: pixels, rows from the top.
        XCTAssertTrue(Selection.pixel(at: CGPoint(x: 10.4, y: 5.6), viewHeight: 100, scale: 2, size: size)
            == (20, 188))
        XCTAssertTrue(Selection.pixel(at: CGPoint(x: 0, y: 100), viewHeight: 100, scale: 2, size: size) == (0, 0))
        // Never outside the image.
        XCTAssertTrue(Selection.pixel(at: CGPoint(x: 500, y: -3), viewHeight: 100, scale: 2, size: size) == (399, 199))
    }

    func testPaddingMeasuresExactly() throws {
        // A 200 × 80 button with 32 px of padding left and right and 24 px above and below its label.
        let button = CGRect(x: 50, y: 60, width: 200, height: 80)
        let label = CGRect(x: 82, y: 84, width: 136, height: 32)
        let grid = try XCTUnwrap(PixelGrid(buttonPage(button: button, label: label)))
        // In the left padding: from the button's edge to the label's.
        let left = try XCTUnwrap(Edges.around(grid, x: 60, y: 100))
        XCTAssertEqual(left.width, 32)
        XCTAssertEqual(left.height, 80, "nothing else in that column")
        // Above the label: the top padding.
        let above = try XCTUnwrap(Edges.around(grid, x: 150, y: 70))
        XCTAssertEqual(above.height, 24)
        XCTAssertEqual(above.width, 200)
        XCTAssertEqual(above.up, 10)
        XCTAssertEqual(above.down, 13)
        // On the label itself: its size.
        XCTAssertEqual(Edges.around(grid, x: 100, y: 90).map { [$0.width, $0.height] }, [136, 32])
    }

    func testSoftEdgesAreOneEdge() throws {
        let button = CGRect(x: 50, y: 60, width: 200, height: 80)
        let label = CGRect(x: 82, y: 84, width: 136, height: 32)
        let grid = try XCTUnwrap(PixelGrid(buttonPage(button: button, label: label, soft: true)))
        // The blended pixels stop the ruler; the faint noise inside doesn't.
        let left = try XCTUnwrap(Edges.around(grid, x: 60, y: 100))
        XCTAssertEqual(left.width, 31, "the label's blended edge is its own pixel")
        let above = try XCTUnwrap(Edges.around(grid, x: 150, y: 70))
        XCTAssertEqual(above.width, 200, "noise within the tolerance is the same colour")
        XCTAssertEqual(above.height, 23)
    }

    func testEdgesStopAtTheImage() throws {
        let grid = try XCTUnwrap(PixelGrid(plain(120, 80)))
        XCTAssertEqual(Edges.around(grid, x: 10, y: 10), Span(left: 10, right: 109, up: 10, down: 69))
        XCTAssertNil(Edges.around(grid, x: 120, y: 0))
    }

    func testRulerLabelsInPixelsAndPoints() {
        XCTAssertEqual(Ruler.label(32, scale: 2), "32 px · 16 pt")
        XCTAssertEqual(Ruler.label(33, scale: 2), "33 px · 16.5 pt")
        XCTAssertEqual(Ruler.label(32, scale: 1), "32 px")
        XCTAssertEqual(Ruler.size(120, 40, scale: 2), "120 × 40 px · 60 × 20 pt")
    }
}
