// Turning a shot into a file's bytes, and Retina into 1x.

import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

public enum Encoding {
    /// `image` as PNG or JPEG. `scale` is its pixels per point, written as its density (144 dpi for Retina), as
    /// macOS's own screenshots are, so Preview and Shots open it at its real size.
    public static func data(_ image: CGImage, format: Settings.Format, scale: CGFloat = 1) -> Data? {
        let data = NSMutableData()
        let type = (format == .png ? UTType.png : UTType.jpeg).identifier as CFString
        guard let destination = CGImageDestinationCreateWithData(data, type, 1, nil) else { return nil }
        var options: [CFString: Any] = format == .jpeg ? [kCGImageDestinationLossyCompressionQuality: 0.9] : [:]
        options[kCGImagePropertyDPIWidth] = 72 * max(scale, 1)
        options[kCGImagePropertyDPIHeight] = 72 * max(scale, 1)
        CGImageDestinationAddImage(destination, image, options as CFDictionary)
        guard CGImageDestinationFinalize(destination) else { return nil }
        return data as Data
    }

    /// A saved shot and its pixels per point, from its density. A file without one (Shots 0.1.0 wrote none) is
    /// taken as `fallbackScale`, this Mac's screen.
    public static func load(_ url: URL, fallbackScale: CGFloat) -> (image: CGImage, scale: CGFloat)? {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
        else { return nil }
        let props = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any]
        guard let dpi = props?[kCGImagePropertyDPIWidth] as? Double, dpi > 0 else {
            return (image, max(fallbackScale, 1))
        }
        return (image, max(1, (dpi / 72).rounded()))
    }

    /// `image` scaled by `factor` (0.5 turns Retina into 1x).
    public static func scaled(_ image: CGImage, by factor: CGFloat) -> CGImage? {
        let width = max(1, Int((CGFloat(image.width) * factor).rounded()))
        let height = max(1, Int((CGFloat(image.height) * factor).rounded()))
        guard let space = image.colorSpace ?? CGColorSpace(name: CGColorSpace.sRGB),
              let context = CGContext(
                  data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0, space: space,
                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
              )
        else { return nil }
        context.interpolationQuality = .high
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        return context.makeImage()
    }

    /// The pixel size of encoded image bytes (for tests and checks).
    public static func size(of data: Data) -> (width: Int, height: Int)? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let props = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let w = props[kCGImagePropertyPixelWidth] as? Int,
              let h = props[kCGImagePropertyPixelHeight] as? Int
        else { return nil }
        return (w, h)
    }
}
