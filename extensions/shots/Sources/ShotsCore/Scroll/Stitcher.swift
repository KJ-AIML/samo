// Scrolling capture's stitching (ADR-004 S5): each new frame of the area is matched against the last frame used, and
// the rows the scroll revealed are added below. Rows are compared by fingerprints (a few column sums each), only
// rows with detail count, so blank margins can't fake a match, and rows that stay put at the top and bottom (a
// sticky header or footer) are kept once. Strips are copied out of their frames, so a frame is let go once matched
// (R17).

import CoreGraphics
import Foundation

/// One frame of the area: its picture and a fingerprint per row.
public struct ScrollFrame {
    public let image: CGImage
    public let width: Int
    public let height: Int
    /// Per row, the sum of red, green and blue in each of `buckets` columns.
    let prints: [[Int32]]
    /// Per row, the sum of its prints.
    let sums: [Int]
    /// Per row: has detail (its columns differ), so it can tell one place from another.
    let detailed: [Bool]
    let bucketWidth: Int

    static let buckets = 32

    public init?(_ image: CGImage) {
        guard let space = CGColorSpace(name: CGColorSpace.sRGB), image.width >= 8, image.height >= 8 else { return nil }
        let (w, h) = (image.width, image.height)
        var bytes = [UInt8](repeating: 0, count: w * h * 4)
        let drawn: Bool = bytes.withUnsafeMutableBytes { raw in
            guard let context = CGContext(
                data: raw.baseAddress, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4, space: space,
                bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue
            ) else { return false }
            context.interpolationQuality = .none
            context.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
            return true
        }
        guard drawn else { return nil }
        let buckets = min(ScrollFrame.buckets, w)
        let bucketWidth = w / buckets
        var prints: [[Int32]] = []
        prints.reserveCapacity(h)
        var sums: [Int] = []
        var detailed: [Bool] = []
        for y in 0..<h {
            var row = [Int32](repeating: 0, count: buckets)
            let start = y * w * 4
            for x in 0..<w {
                let i = start + x * 4
                let bucket = min(x / bucketWidth, buckets - 1)
                row[bucket] += Int32(bytes[i]) + Int32(bytes[i + 1]) + Int32(bytes[i + 2])
            }
            let total = row.reduce(0) { $0 + Int($1) }
            // Detail: the columns differ by more than a few levels a pixel, or the row isn't one flat colour.
            let low = row.min() ?? 0
            let high = row.max() ?? 0
            prints.append(row)
            sums.append(total)
            detailed.append(Int(high - low) > bucketWidth * 18)
        }
        self.image = image
        width = w
        height = h
        self.prints = prints
        self.sums = sums
        self.detailed = detailed
        self.bucketWidth = bucketWidth
    }

    /// Whether row `a` here and row `b` of `other` look the same.
    func same(_ a: Int, _ other: ScrollFrame, _ b: Int) -> Bool {
        guard abs(sums[a] - other.sums[b]) <= width * 3 else { return false }
        let tolerance = Int32(bucketWidth * 6)
        return zip(prints[a], other.prints[b]).allSatisfy { abs($0 - $1) <= tolerance }
    }
}

public final class Stitcher {
    /// What adding a frame did.
    public enum Step: Equatable {
        /// This many new rows were added below.
        case added(Int)
        /// Nothing moved.
        case unchanged
        /// The page went back up: ignored.
        case scrolledBack
        /// No overlap with the last frame (the page jumped): stop, keeping what came before.
        case lost
        /// The cap was reached: stop.
        case full
    }

    public static let maxHeight = 30_000
    public static let maxBytes = 400 * 1024 * 1024

    public let width: Int
    private let maxHeight: Int
    private let maxBytes: Int
    private var reference: ScrollFrame
    private var strips: [CGImage] = []
    private var footer: CGImage?
    public private(set) var finished = false

    public init(first: ScrollFrame, maxHeight: Int = Stitcher.maxHeight, maxBytes: Int = Stitcher.maxBytes) {
        reference = first
        width = first.width
        self.maxHeight = maxHeight
        self.maxBytes = maxBytes
    }

    /// The height of the picture so far.
    public var height: Int {
        strips.isEmpty ? reference.height : strips.reduce(0) { $0 + $1.height } + (footer?.height ?? 0)
    }

    /// The picture so far, as strips top to bottom (for the preview).
    public var parts: [CGImage] {
        strips.isEmpty ? [reference.image] : strips + (footer.map { [$0] } ?? [])
    }

    public func add(_ frame: ScrollFrame) -> Step {
        guard !finished else { return .full }
        guard frame.width == width, frame.height == reference.height else { return .lost }
        let h = frame.height
        // Rows that stayed put at the top and bottom: a sticky header and footer, or nothing moved at all.
        var header = 0
        while header < h, frame.same(header, reference, header) { header += 1 }
        if header == h { return .unchanged }
        var footer = 0
        while footer < h - header, frame.same(h - 1 - footer, reference, h - 1 - footer) { footer += 1 }
        header = min(header, h / 3)
        footer = min(footer, h / 3)
        let top = header
        let middle = h - header - footer
        let minOverlap = max(24, middle / 6)
        guard middle > minOverlap else { return .unchanged }
        // Mostly the same where it was: something small changed (a caret, a spinner), not a scroll.
        if score(frame, shift: 0, top: top, middle: middle).map({ $0 >= 0.9 }) ?? true { return .unchanged }
        for d in 1...(middle - minOverlap) where matches(frame, shift: d, top: top, middle: middle) {
            return append(frame, shift: d, footer: footer)
        }
        for d in 1...(middle - minOverlap) where matches(frame, shift: -d, top: top, middle: middle) {
            return .scrolledBack
        }
        return .lost
    }

    /// The share of detailed rows that match when the new frame's content sits `shift` rows higher (a scroll down)
    /// than the reference's; `nil` if too few detailed rows overlap to tell.
    private func score(_ frame: ScrollFrame, shift: Int, top: Int, middle: Int) -> Double? {
        let overlap = middle - abs(shift)
        var tested = 0
        var matched = 0
        for i in 0..<overlap {
            let (new, old) = shift >= 0 ? (top + i, top + i + shift) : (top + i - shift, top + i)
            guard frame.detailed[new] else { continue }
            tested += 1
            if frame.same(new, reference, old) { matched += 1 }
        }
        return tested >= 8 ? Double(matched) / Double(tested) : nil
    }

    /// Whether `shift` lines up: quick on sums with an early out, then checked row by row.
    private func matches(_ frame: ScrollFrame, shift: Int, top: Int, middle: Int) -> Bool {
        let overlap = middle - abs(shift)
        var tested = 0
        var missed = 0
        for i in 0..<overlap {
            let (new, old) = shift >= 0 ? (top + i, top + i + shift) : (top + i - shift, top + i)
            guard frame.detailed[new] else { continue }
            tested += 1
            if abs(frame.sums[new] - reference.sums[old]) > frame.width * 3 {
                missed += 1
                if missed > 2 + tested / 40 { return false }
            }
        }
        guard tested >= 8, missed <= tested / 40 else { return false }
        return (score(frame, shift: shift, top: top, middle: middle) ?? 0) >= 0.97
    }

    private func append(_ frame: ScrollFrame, shift d: Int, footer: Int) -> Step {
        let h = frame.height
        if strips.isEmpty {
            // The first move: the first frame down to its footer.
            guard let base = Stitcher.copy(reference.image, rows: 0..<(h - footer)) else { return .lost }
            strips.append(base)
        }
        var rows = d
        var step = Step.added(d)
        // Room below what is kept so far; at the cap the footer is left off, so it doesn't count.
        let body = strips.reduce(0) { $0 + $1.height }
        let room = min(maxHeight, maxBytes / (width * 4)) - body
        if rows >= room {
            rows = max(0, room)
            step = .full
            finished = true
        }
        if rows > 0, let strip = Stitcher.copy(frame.image, rows: (h - footer - d)..<(h - footer - d + rows)) {
            strips.append(strip)
        }
        self.footer = footer > 0 && !finished ? Stitcher.copy(frame.image, rows: (h - footer)..<h) : nil
        reference = frame
        return step
    }

    /// Rows of an image (from the top), copied into their own memory, so the frame can go.
    static func copy(_ image: CGImage, rows: Range<Int>) -> CGImage? {
        guard !rows.isEmpty,
              let crop = image.cropping(to: CGRect(x: 0, y: rows.lowerBound, width: image.width, height: rows.count)),
              let space = CGColorSpace(name: CGColorSpace.sRGB),
              let context = CGContext(data: nil, width: crop.width, height: crop.height, bitsPerComponent: 8,
                                      bytesPerRow: 0, space: space,
                                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return nil }
        context.interpolationQuality = .none
        context.draw(crop, in: CGRect(x: 0, y: 0, width: crop.width, height: crop.height))
        return context.makeImage()
    }

    /// The whole picture, top to bottom.
    public func image() -> CGImage? {
        let parts = parts
        let total = parts.reduce(0) { $0 + $1.height }
        guard let space = CGColorSpace(name: CGColorSpace.sRGB), total > 0,
              let context = CGContext(data: nil, width: width, height: total, bitsPerComponent: 8, bytesPerRow: 0,
                                      space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return nil }
        context.interpolationQuality = .none
        var y = total
        for part in parts {
            y -= part.height
            context.draw(part, in: CGRect(x: 0, y: y, width: part.width, height: part.height))
        }
        return context.makeImage()
    }
}
