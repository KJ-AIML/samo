import CoreGraphics
import XCTest
@testable import ShotsCore

/// An image's pixels as sRGB RGBA bytes, read with `y` from the bottom, as the editor's coordinates run.
struct Pixels {
    let width: Int
    let height: Int
    private let bytes: [UInt8]

    init(_ image: CGImage) throws {
        width = image.width
        height = image.height
        var buffer = [UInt8](repeating: 0, count: width * height * 4)
        let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
        let drawn: Bool = buffer.withUnsafeMutableBytes { raw in
            guard let context = CGContext(
                data: raw.baseAddress, width: image.width, height: image.height, bitsPerComponent: 8,
                bytesPerRow: image.width * 4, space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            ) else { return false }
            context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
            return true
        }
        XCTAssertTrue(drawn)
        bytes = buffer
    }

    /// Red, green, blue, alpha at `(x, y)`.
    func at(_ x: Int, _ y: Int) -> [UInt8] {
        let i = ((height - 1 - y) * width + x) * 4
        return Array(bytes[i..<(i + 4)])
    }

    func luminance(_ x: Int, _ y: Int) -> Double {
        let p = at(x, y)
        return (0.2126 * Double(p[0]) + 0.7152 * Double(p[1]) + 0.0722 * Double(p[2])) / 255
    }

    /// The smallest box holding every pixel that differs from `other`'s by more than `tolerance`.
    func changed(from other: Pixels, tolerance: Int = 2) -> CGRect {
        var box = CGRect.null
        for y in 0..<height {
            for x in 0..<width {
                let a = at(x, y)
                let b = other.at(x, y)
                if zip(a, b).contains(where: { abs(Int($0) - Int($1)) > tolerance }) {
                    box = box.union(CGRect(x: x, y: y, width: 1, height: 1))
                }
            }
        }
        return box
    }

    /// The spread of luminance inside `rect`: 0 for flat.
    func spread(in rect: CGRect) -> Double {
        var values: [Double] = []
        for y in Int(rect.minY)..<Int(rect.maxY) {
            for x in Int(rect.minX)..<Int(rect.maxX) {
                values.append(luminance(x, y))
            }
        }
        let mean = values.reduce(0, +) / Double(max(values.count, 1))
        return (values.map { ($0 - mean) * ($0 - mean) }.reduce(0, +) / Double(max(values.count, 1))).squareRoot()
    }
}

/// A shot of black and white stripes, `stripe` pixels wide: as hard to blur away as text.
func stripes(_ width: Int, _ height: Int, stripe: Int = 3) throws -> CGImage {
    let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
    let context = try XCTUnwrap(CGContext(
        data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0, space: space,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ))
    context.setFillColor(CGColor(gray: 1, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    context.setFillColor(CGColor(gray: 0, alpha: 1))
    for x in stride(from: 0, to: width, by: stripe * 2) {
        context.fill(CGRect(x: x, y: 0, width: stripe, height: height))
    }
    return try XCTUnwrap(context.makeImage())
}

func plain(_ width: Int, _ height: Int, gray: CGFloat = 1) throws -> CGImage {
    let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
    let context = try XCTUnwrap(CGContext(
        data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0, space: space,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ))
    context.setFillColor(CGColor(gray: gray, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    return try XCTUnwrap(context.makeImage())
}

final class RendererTests: XCTestCase {
    private func render(_ doc: Document, _ base: CGImage, retina: Bool = true) throws -> Pixels {
        try Pixels(XCTUnwrap(Renderer.export(doc, base: base, redaction: Redaction(), retina: retina)))
    }

    func testEveryToolLeavesAMark() throws {
        let base = try stripes(240, 160)
        let empty = try render(Document(size: CGSize(width: 240, height: 160), scale: 2), base)
        let shapes: [(Mark.Kind, [CGPoint], String)] = [
            (.arrow, [CGPoint(x: 40, y: 40), CGPoint(x: 160, y: 100)], ""),
            (.line, [CGPoint(x: 40, y: 40), CGPoint(x: 160, y: 100)], ""),
            (.rectangle, [CGPoint(x: 60, y: 40), CGPoint(x: 150, y: 110)], ""),
            (.ellipse, [CGPoint(x: 60, y: 40), CGPoint(x: 150, y: 110)], ""),
            (.highlighter, [CGPoint(x: 50, y: 80), CGPoint(x: 90, y: 90), CGPoint(x: 170, y: 70)], ""),
            (.pen, [CGPoint(x: 50, y: 80), CGPoint(x: 90, y: 90), CGPoint(x: 170, y: 70)], ""),
            (.step, [CGPoint(x: 120, y: 80)], ""),
            (.text, [CGPoint(x: 30, y: 120)], "Here"),
            (.blur, [CGPoint(x: 60, y: 40), CGPoint(x: 150, y: 110)], ""),
            (.pixelate, [CGPoint(x: 60, y: 40), CGPoint(x: 150, y: 110)], ""),
        ]
        XCTAssertEqual(Set(shapes.map(\.0)), Set(Mark.Kind.allCases).subtracting([.spotlight]))
        for (kind, points, text) in shapes {
            let doc = Document(size: CGSize(width: 240, height: 160), scale: 2)
            let id = doc.add(kind, points: points, style: .standard, text: text)
            let changed = try render(doc, base).changed(from: empty)
            let mark = try XCTUnwrap(doc.mark(id))
            XCTAssertFalse(changed.isNull, "\(kind) drew nothing")
            // Nothing outside what it says it paints (a pixel either way for anti-aliasing).
            let bounds = Geometry.bounds(mark, scale: 2).insetBy(dx: -1, dy: -1)
            XCTAssertTrue(bounds.contains(changed), "\(kind) painted \(changed), outside \(bounds)")
        }
    }

    func testArrowsPointTheRightWay() throws {
        let base = try plain(200, 100)
        let doc = Document(size: CGSize(width: 200, height: 100), scale: 2)
        doc.add(.arrow, points: [CGPoint(x: 20, y: 50), CGPoint(x: 180, y: 50)], style: .standard)
        let pixels = try render(doc, base)
        func painted(column x: Int) -> Int {
            (0..<100).filter { pixels.luminance(x, $0) < 0.9 }.count
        }
        let head = Geometry.arrowHead(width: Geometry.lineWidth(Mark(id: 0, kind: .arrow, points: [], style: .standard),
                                                                scale: 2), length: 160)
        // The head, near the end, is wider than the shaft near the start, and the tip reaches the end.
        XCTAssertGreaterThan(painted(column: Int(180 - head.height) + 1), painted(column: 30) * 2)
        XCTAssertGreaterThan(painted(column: 177), 0)
        XCTAssertEqual(painted(column: 185), 0)
    }

    func testRedactionDestroysTheText() throws {
        let base = try stripes(300, 200)
        let area = CGRect(x: 40, y: 40, width: 200, height: 120)
        let before = try Pixels(base)
        XCTAssertGreaterThan(before.spread(in: area), 0.4)
        for kind in [Mark.Kind.blur, .pixelate] {
            let doc = Document(size: CGSize(width: 300, height: 200), scale: 2)
            doc.add(kind, points: [CGPoint(x: area.minX, y: area.minY), CGPoint(x: area.maxX, y: area.maxY)],
                    style: .standard)
            let after = try render(doc, base)
            // Inside, away from the edges: flat, the stripes gone.
            XCTAssertLessThan(after.spread(in: area.insetBy(dx: 20, dy: 20)), 0.05, "\(kind) left the stripes")
            // Outside: untouched.
            XCTAssertEqual(after.at(10, 10), before.at(10, 10))
            XCTAssertEqual(after.at(290, 190), before.at(290, 190))
        }
        // Pixelate's blocks are each one colour, over a picture that isn't flat.
        let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
        let context = try XCTUnwrap(CGContext(data: nil, width: 64, height: 64, bitsPerComponent: 8, bytesPerRow: 0,
                                              space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        let colours = [CGColor(srgbRed: 1, green: 0, blue: 0, alpha: 1), CGColor(srgbRed: 0, green: 0, blue: 1, alpha: 1)]
        for y in 0..<64 {
            context.setFillColor(colours[(y / 5) % 2])
            context.fill(CGRect(x: 0, y: y, width: 64, height: 1))
        }
        let gradient = try XCTUnwrap(context.makeImage())
        let blocks = try Pixels(XCTUnwrap(Redaction.pixelate(gradient, block: 16)))
        for by in 0..<4 {
            for bx in 0..<4 {
                let first = blocks.at(bx * 16, 63 - by * 16)
                for (dx, dy) in [(15, 0), (0, 15), (15, 15), (7, 8)] {
                    XCTAssertEqual(blocks.at(bx * 16 + dx, 63 - by * 16 - dy), first, "block \(bx),\(by)")
                }
            }
        }
    }

    func testSpotlightDimsOutside() throws {
        let base = try plain(200, 120)
        let doc = Document(size: CGSize(width: 200, height: 120), scale: 2)
        doc.add(.spotlight, points: [CGPoint(x: 50, y: 30), CGPoint(x: 150, y: 90)], style: .standard)
        let pixels = try render(doc, base)
        XCTAssertGreaterThan(pixels.luminance(100, 60), 0.99)
        XCTAssertLessThan(pixels.luminance(10, 10), 0.6)
        XCTAssertLessThan(pixels.luminance(190, 110), 0.6)
        // Two spotlights: both bright, the rest dim.
        doc.add(.spotlight, points: [CGPoint(x: 160, y: 95), CGPoint(x: 195, y: 115)], style: .standard)
        let two = try render(doc, base)
        XCTAssertGreaterThan(two.luminance(180, 105), 0.99)
        XCTAssertLessThan(two.luminance(10, 10), 0.6)
    }

    func testCropSetsTheSize() throws {
        let base = try stripes(200, 100)
        let doc = Document(size: CGSize(width: 200, height: 100), scale: 2)
        doc.setCrop(CGRect(x: 9, y: 10, width: 100, height: 50))
        let image = try XCTUnwrap(Renderer.export(doc, base: base, redaction: Redaction(), retina: true))
        XCTAssertEqual(image.width, 100)
        XCTAssertEqual(image.height, 50)
        // The export's corner is the shot's (9, 10).
        let whole = try Pixels(base)
        let cropped = try Pixels(image)
        XCTAssertEqual(cropped.at(0, 0), whole.at(9, 10))
        XCTAssertEqual(cropped.at(5, 20), whole.at(14, 30))
        // A mark in the kept part moves with it.
        doc.add(.step, points: [CGPoint(x: 59, y: 35)], style: .standard)
        let marked = try render(doc, base)
        // On its disc, beside the number.
        XCTAssertNotEqual(marked.at(68, 25), cropped.at(68, 25))
    }

    func testBackdropPads() throws {
        let base = try plain(400, 200, gray: 0.5)
        let doc = Document(size: CGSize(width: 400, height: 200), scale: 2)
        doc.setBackdrop(.white)
        let layout = Renderer.layout(doc)
        XCTAssertEqual(layout.padding, 48)
        let image = try XCTUnwrap(Renderer.export(doc, base: base, redaction: Redaction(), retina: true))
        XCTAssertEqual(image.width, 400 + 96)
        XCTAssertEqual(image.height, 200 + 96)
        let pixels = try Pixels(image)
        // The corner is the ground; the middle is the shot.
        XCTAssertGreaterThan(pixels.luminance(2, image.height - 3), 0.95)
        XCTAssertEqual(pixels.luminance(image.width / 2, image.height / 2), try Pixels(base).luminance(200, 100),
                       accuracy: 0.01)
        // The shot's corner is rounded: just inside it is the ground, not the shot.
        XCTAssertGreaterThan(pixels.luminance(49, image.height - 50), 0.6)
        // Points map both ways.
        XCTAssertEqual(layout.toExport(.zero), CGPoint(x: 48, y: 48))
        XCTAssertEqual(layout.toShot(CGPoint(x: 48, y: 48)), .zero)
    }

    func testOneXHalvesThePixels() throws {
        let base = try stripes(200, 100)
        let doc = Document(size: CGSize(width: 200, height: 100), scale: 2)
        doc.add(.rectangle, points: [CGPoint(x: 20, y: 20), CGPoint(x: 120, y: 80)], style: .standard)
        let half = try XCTUnwrap(Renderer.export(doc, base: base, redaction: Redaction(), retina: false))
        XCTAssertEqual(half.width, 100)
        XCTAssertEqual(half.height, 50)
        let full = try XCTUnwrap(Renderer.export(doc, base: base, redaction: Redaction(), retina: true))
        XCTAssertEqual(full.width, 200)
    }

    func testRedactionCacheIsBounded() throws {
        let base = try stripes(300, 200)
        let redaction = Redaction()
        for i in 0..<40 {
            _ = redaction.patch(.blur, rect: CGRect(x: i, y: 10, width: 50, height: 50), base: base, scale: 2)
        }
        XCTAssertLessThanOrEqual(redaction.cached, 16)
        redaction.clear()
        XCTAssertEqual(redaction.cached, 0)
    }
}
