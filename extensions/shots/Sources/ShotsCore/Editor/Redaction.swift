// Blur and pixelate (ADR-004 S3), made from the shot's own pixels. Strong enough that text under them can't be
// read; only the export is saved, so nothing in a saved file can be un-blurred. Patches are cached per area, a few
// at a time, and belong to one editor window, so they go when it closes (R17).

import CoreGraphics
import CoreImage
import Foundation

public final class Redaction {
    /// One Core Image context for every window: making one costs time and memory.
    private static let context = CIContext(options: [.cacheIntermediates: false])
    private static let keep = 16

    private struct Key: Equatable {
        let kind: Mark.Kind
        let rect: CGRect
    }

    private var cache: [(key: Key, patch: CGImage)] = []

    public init() {}

    /// Pixelate's block, in pixels.
    public static func block(scale: CGFloat) -> CGFloat { max(10, 8 * scale) }

    /// Blur's strength, in pixels: more for a bigger area.
    public static func sigma(for rect: CGRect, scale: CGFloat) -> CGFloat {
        min(60, max(6 * scale, min(rect.width, rect.height) / 10))
    }

    /// The redacted pixels for `rect`, in the shot's coordinates (origin bottom-left), `rect.size` big.
    public func patch(_ kind: Mark.Kind, rect: CGRect, base: CGImage, scale: CGFloat) -> CGImage? {
        let key = Key(kind: kind, rect: rect)
        if let hit = cache.firstIndex(where: { $0.key == key }) {
            let entry = cache.remove(at: hit)
            cache.append(entry)
            return entry.patch
        }
        // The image's rows run from the top.
        let source = CGRect(x: rect.minX, y: CGFloat(base.height) - rect.maxY, width: rect.width, height: rect.height)
        guard let crop = base.cropping(to: source) else { return nil }
        let made: CGImage?
        switch kind {
        case .pixelate: made = Redaction.pixelate(crop, block: Redaction.block(scale: scale))
        case .blur: made = Redaction.blur(crop, sigma: Redaction.sigma(for: rect, scale: scale))
        default: made = nil
        }
        guard let made else { return nil }
        cache.append((key, made))
        if cache.count > Redaction.keep { cache.removeFirst(cache.count - Redaction.keep) }
        return made
    }

    /// Drops every cached patch.
    public func clear() {
        cache.removeAll()
    }

    public var cached: Int { cache.count }

    /// Each block becomes the average of its pixels.
    static func pixelate(_ image: CGImage, block: CGFloat) -> CGImage? {
        let small = (width: max(1, Int((CGFloat(image.width) / block).rounded(.up))),
                     height: max(1, Int((CGFloat(image.height) / block).rounded(.up))))
        guard let down = Renderer.bitmap(width: small.width, height: small.height, like: image) else { return nil }
        down.interpolationQuality = .high
        down.draw(image, in: CGRect(x: 0, y: 0, width: small.width, height: small.height))
        guard let tiny = down.makeImage(),
              let up = Renderer.bitmap(width: image.width, height: image.height, like: image)
        else { return nil }
        up.interpolationQuality = .none
        // Whole blocks from the top left, as the eye reads them; the last row and column may be cut.
        let drawn = CGSize(width: CGFloat(small.width) * block, height: CGFloat(small.height) * block)
        up.draw(tiny, in: CGRect(x: 0, y: CGFloat(image.height) - drawn.height, width: drawn.width,
                                 height: drawn.height))
        return up.makeImage()
    }

    static func blur(_ image: CGImage, sigma: CGFloat) -> CGImage? {
        let extent = CGRect(x: 0, y: 0, width: image.width, height: image.height)
        let blurred = CIImage(cgImage: image).clampedToExtent().applyingGaussianBlur(sigma: Double(sigma))
            .cropped(to: extent)
        return context.createCGImage(blurred, from: extent)
    }
}
