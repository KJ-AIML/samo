// Colours and pixels for the colour picker and the ruler (ADR-004 S4). A shot's pixels are read once, converted to
// sRGB (what CSS and design tools use) from the display's colour space, with rows from the top as the eye reads.

import CoreGraphics
import Foundation

/// One pixel's colour, in sRGB.
public struct Pixel: Equatable {
    public var r: UInt8
    public var g: UInt8
    public var b: UInt8

    public init(r: UInt8, g: UInt8, b: UInt8) {
        self.r = r
        self.g = g
        self.b = b
    }

    /// `#F15B2A`.
    public var hex: String {
        String(format: "#%02X%02X%02X", r, g, b)
    }

    /// `rgb(241, 91, 42)`.
    public var css: String {
        "rgb(\(r), \(g), \(b))"
    }

    public var rgba: RGBA {
        RGBA(r: Double(r) / 255, g: Double(g) / 255, b: Double(b) / 255, a: 1)
    }

    /// Whether `other` is within `tolerance` of this on every channel.
    public func near(_ other: Pixel, tolerance: Int) -> Bool {
        abs(Int(r) - Int(other.r)) <= tolerance && abs(Int(g) - Int(other.g)) <= tolerance
            && abs(Int(b) - Int(other.b)) <= tolerance
    }
}

/// A shot's pixels in sRGB, rows from the top. Made when the colour picker or the ruler first needs it, and let go
/// with the window that made it (R17).
public final class PixelGrid {
    public let width: Int
    public let height: Int
    private let bytes: [UInt8]

    public init?(_ image: CGImage) {
        guard let space = CGColorSpace(name: CGColorSpace.sRGB), image.width > 0, image.height > 0 else { return nil }
        width = image.width
        height = image.height
        var buffer = [UInt8](repeating: 0, count: image.width * image.height * 4)
        let drawn: Bool = buffer.withUnsafeMutableBytes { raw in
            guard let context = CGContext(
                data: raw.baseAddress, width: image.width, height: image.height, bitsPerComponent: 8,
                bytesPerRow: image.width * 4, space: space,
                bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue
            ) else { return false }
            context.interpolationQuality = .none
            context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
            return true
        }
        guard drawn else { return nil }
        bytes = buffer
    }

    /// The pixel at `x`, `y` (rows from the top), or `nil` outside.
    public func pixel(_ x: Int, _ y: Int) -> Pixel? {
        guard x >= 0, y >= 0, x < width, y < height else { return nil }
        let i = (y * width + x) * 4
        return Pixel(r: bytes[i], g: bytes[i + 1], b: bytes[i + 2])
    }
}
