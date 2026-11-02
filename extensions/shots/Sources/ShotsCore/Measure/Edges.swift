// The ruler (ADR-004 S4): from the pixel under the pointer, how far the same colour runs left, right, up and down.
// An edge is the first pixel whose colour differs by more than a tolerance on any channel, so an anti-aliased edge
// still stops the ruler, and a screenshot's faint noise doesn't.

import CoreGraphics
import SamoKit
import Foundation

/// How far the pointer's colour runs each way, in pixels, not counting the pointer's own pixel.
public struct Span: Equatable {
    public var left: Int
    public var right: Int
    public var up: Int
    public var down: Int

    public init(left: Int, right: Int, up: Int, down: Int) {
        self.left = left
        self.right = right
        self.up = up
        self.down = down
    }

    /// The width of the run across the pointer: from one edge to the other.
    public var width: Int { left + right + 1 }
    public var height: Int { up + down + 1 }
}

public enum Edges {
    /// Out of 255, on any channel.
    public static let tolerance = 16

    public static func around(_ grid: PixelGrid, x: Int, y: Int, tolerance: Int = tolerance) -> Span? {
        guard let start = grid.pixel(x, y) else { return nil }
        func run(_ dx: Int, _ dy: Int) -> Int {
            var n = 0
            var (cx, cy) = (x + dx, y + dy)
            while let p = grid.pixel(cx, cy), p.near(start, tolerance: tolerance) {
                n += 1
                cx += dx
                cy += dy
            }
            return n
        }
        return Span(left: run(-1, 0), right: run(1, 0), up: run(0, -1), down: run(0, 1))
    }
}

public enum Ruler {
    /// `32 px`, or on Retina `32 px · 16 pt` (CSS pixels are points).
    public static func label(_ pixels: Int, scale: CGFloat) -> String {
        guard scale > 1 else { return "\(pixels) px" }
        return "\(pixels) px · \(points(pixels, scale: scale)) pt"
    }

    /// `120 × 40 px`, or on Retina `120 × 40 px · 60 × 20 pt`.
    public static func size(_ width: Int, _ height: Int, scale: CGFloat) -> String {
        guard scale > 1 else { return "\(width) × \(height) px" }
        return "\(width) × \(height) px · \(points(width, scale: scale)) × \(points(height, scale: scale)) pt"
    }

    /// Pixels as points: whole when they are, else to a tenth.
    static func points(_ pixels: Int, scale: CGFloat) -> String {
        let value = Double(pixels) / Double(scale)
        return value == value.rounded() ? String(Int(value)) : String(format: "%.1f", value)
    }
}
