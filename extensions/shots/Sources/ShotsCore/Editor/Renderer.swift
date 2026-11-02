// Drawing a document (ADR-004 S3): the shot, its redactions, the spotlight, then every other mark in the order they
// were made. The editor's canvas and every export (copy, save, drag, pin) draw through here, so what is copied is
// what was on screen.

import AppKit
import SamoKit
import CoreGraphics

/// Where things land in the export, in its pixels.
public struct Layout: Equatable {
    /// The export's size.
    public var size: CGSize
    /// Add to a point in the shot to get the point in the export (the crop and the backdrop's padding).
    public var offset: CGVector
    /// The kept part of the shot, in the export.
    public var shotRect: CGRect
    public var padding: CGFloat
    /// The shot's corners and shadow when there is a backdrop.
    public var cornerRadius: CGFloat
    public var shadowBlur: CGFloat

    /// Nothing laid out yet.
    public static let empty = Layout(size: .zero, offset: .zero, shotRect: .zero, padding: 0, cornerRadius: 0,
                                     shadowBlur: 0)

    /// A point in the shot, in the export.
    public func toExport(_ p: CGPoint) -> CGPoint {
        CGPoint(x: p.x + offset.dx, y: p.y + offset.dy)
    }

    /// A point in the export, in the shot.
    public func toShot(_ p: CGPoint) -> CGPoint {
        CGPoint(x: p.x - offset.dx, y: p.y - offset.dy)
    }
}

public enum Renderer {
    /// How a document lays out: cropped, then padded for its backdrop.
    public static func layout(_ doc: Document) -> Layout {
        layout(visible: doc.visibleRect, backdrop: doc.canvas.backdrop, scale: doc.scale)
    }

    public static func layout(visible: CGRect, backdrop: Backdrop?, scale: CGFloat) -> Layout {
        let padding = backdrop == nil ? 0 : Backdrop.padding(for: visible.size, scale: scale)
        let shotRect = CGRect(x: padding, y: padding, width: visible.width, height: visible.height)
        return Layout(
            size: CGSize(width: visible.width + 2 * padding, height: visible.height + 2 * padding),
            offset: CGVector(dx: padding - visible.minX, dy: padding - visible.minY),
            shotRect: shotRect, padding: padding,
            cornerRadius: backdrop == nil ? 0 : 10 * scale, shadowBlur: backdrop == nil ? 0 : 24 * scale
        )
    }

    /// The whole export into `context`, whose user space is the export's pixels.
    public static func draw(_ doc: Document, base: CGImage, in context: CGContext, redaction: Redaction) {
        let layout = layout(doc)
        if let backdrop = doc.canvas.backdrop {
            Backdrop.drawGround(backdrop, size: layout.size, in: context)
            Backdrop.drawShadow(layout, in: context)
        }
        context.saveGState()
        if layout.cornerRadius > 0 {
            context.addPath(CGPath(roundedRect: layout.shotRect, cornerWidth: layout.cornerRadius,
                                   cornerHeight: layout.cornerRadius, transform: nil))
            context.clip()
        } else {
            context.clip(to: layout.shotRect)
        }
        context.translateBy(x: layout.offset.dx, y: layout.offset.dy)
        context.interpolationQuality = .none
        context.draw(base, in: doc.fullRect)
        drawMarks(doc.canvas.marks, scale: doc.scale, base: base, redaction: redaction, in: context)
        context.restoreGState()
    }

    /// The export as an image: Retina as captured, or 1x.
    public static func export(_ doc: Document, base: CGImage, redaction: Redaction, retina: Bool) -> CGImage? {
        let layout = layout(doc)
        guard let context = bitmap(width: Int(layout.size.width.rounded()), height: Int(layout.size.height.rounded()),
                                   like: base)
        else { return nil }
        draw(doc, base: base, in: context, redaction: redaction)
        guard let image = context.makeImage() else { return nil }
        return retina || doc.scale <= 1 ? image : Encoding.scaled(image, by: 1 / doc.scale)
    }

    /// An 8-bit RGBA context in the shot's colour space (sRGB if it has none that fits).
    static func bitmap(width: Int, height: Int, like base: CGImage) -> CGContext? {
        let space = base.colorSpace.flatMap { $0.model == .rgb ? $0 : nil } ?? CGColorSpace(name: CGColorSpace.sRGB)
        guard let space, width > 0, height > 0 else { return nil }
        return CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0, space: space,
                         bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    }

    // MARK: Marks

    /// Marks in the shot's coordinates: redactions first, as part of the picture, then the spotlight's dimming,
    /// then the rest in the order they were made. `skipping` is a text mark being typed (the editor shows a field).
    public static func drawMarks(
        _ marks: [Mark], scale: CGFloat, base: CGImage, redaction: Redaction, in context: CGContext,
        skipping: Int? = nil
    ) {
        for mark in marks where mark.kind.redacts && mark.id != skipping {
            drawRedaction(mark, scale: scale, base: base, redaction: redaction, in: context)
        }
        let spots = marks.filter { $0.kind == .spotlight && !$0.box.isEmpty }
        if !spots.isEmpty {
            drawSpotlight(spots, scale: scale, size: CGSize(width: base.width, height: base.height), in: context)
        }
        let numbers = Document.stepNumbers(marks)
        for mark in marks where !mark.kind.redacts && mark.kind != .spotlight && mark.id != skipping {
            draw(mark, scale: scale, number: numbers[mark.id] ?? 0, in: context)
        }
    }

    static func drawRedaction(_ mark: Mark, scale: CGFloat, base: CGImage, redaction: Redaction,
                              in context: CGContext) {
        let rect = mark.box.integral.intersection(CGRect(x: 0, y: 0, width: base.width, height: base.height))
        guard !rect.isNull, rect.width >= 1, rect.height >= 1,
              let patch = redaction.patch(mark.kind, rect: rect, base: base, scale: scale)
        else { return }
        context.saveGState()
        context.interpolationQuality = .none
        context.draw(patch, in: rect)
        context.restoreGState()
    }

    static func drawSpotlight(_ spots: [Mark], scale: CGFloat, size: CGSize, in context: CGContext) {
        context.saveGState()
        context.addRect(CGRect(origin: .zero, size: size).insetBy(dx: -size.width, dy: -size.height))
        for spot in spots {
            let r = min(6 * scale, spot.box.width / 2, spot.box.height / 2)
            context.addPath(CGPath(roundedRect: spot.box, cornerWidth: r, cornerHeight: r, transform: nil))
        }
        context.setFillColor(CGColor(gray: 0, alpha: 0.55))
        context.fillPath(using: .evenOdd)
        context.restoreGState()
    }

    /// A colour that reads on top of `colour`: Ink on light ones, white on the rest.
    public static func contrast(_ colour: RGBA) -> RGBA {
        let luminance = 0.2126 * colour.r + 0.7152 * colour.g + 0.0722 * colour.b
        return luminance > 0.6 ? Swatch.ink : Swatch.white
    }

    static func draw(_ mark: Mark, scale: CGFloat, number: Int, in context: CGContext) {
        let colour = mark.style.colour.color.cgColor
        let width = Geometry.lineWidth(mark, scale: scale)
        context.saveGState()
        defer { context.restoreGState() }
        context.setLineCap(.round)
        context.setLineJoin(.round)
        switch mark.kind {
        case .arrow:
            guard mark.points.count > 1 else { return }
            context.addPath(Geometry.arrowPath(from: mark.points[0], to: mark.points[1], width: width))
            context.setFillColor(colour)
            context.fillPath()
        case .line:
            guard mark.points.count > 1 else { return }
            context.move(to: mark.points[0])
            context.addLine(to: mark.points[1])
            context.setStrokeColor(colour)
            context.setLineWidth(width)
            context.strokePath()
        case .rectangle:
            context.setStrokeColor(colour)
            context.setLineWidth(width)
            context.stroke(mark.box)
        case .ellipse:
            context.setStrokeColor(colour)
            context.setLineWidth(width)
            context.strokeEllipse(in: mark.box)
        case .pen, .highlighter:
            guard let path = smoothPath(mark.points) else { return }
            context.addPath(path)
            context.setStrokeColor(mark.kind == .highlighter ? mark.style.colour.color.withAlphaComponent(0.4).cgColor
                                                             : colour)
            context.setLineWidth(width)
            if mark.kind == .highlighter { context.setLineCap(.butt) }
            context.strokePath()
        case .step:
            drawStep(mark, scale: scale, number: number, in: context)
        case .text:
            drawText(mark, scale: scale, in: context)
        case .blur, .pixelate, .spotlight:
            break
        }
    }

    /// A freehand line through `points`, smoothed through their midpoints.
    static func smoothPath(_ points: [CGPoint]) -> CGPath? {
        guard let first = points.first else { return nil }
        let path = CGMutablePath()
        path.move(to: first)
        if points.count == 1 {
            path.addLine(to: first)
            return path
        }
        for i in 1..<points.count {
            let mid = CGPoint(x: (points[i - 1].x + points[i].x) / 2, y: (points[i - 1].y + points[i].y) / 2)
            path.addQuadCurve(to: mid, control: points[i - 1])
        }
        if let last = points.last { path.addLine(to: last) }
        return path
    }

    static func drawStep(_ mark: Mark, scale: CGFloat, number: Int, in context: CGContext) {
        guard let centre = mark.points.first else { return }
        let d = Geometry.stepDiameter(mark, scale: scale)
        let disc = CGRect(x: centre.x - d / 2, y: centre.y - d / 2, width: d, height: d)
        let ink = contrast(mark.style.colour)
        context.setFillColor(mark.style.colour.color.cgColor)
        context.fillEllipse(in: disc)
        let ring = 1.5 * scale
        context.setStrokeColor(ink.color.withAlphaComponent(0.9).cgColor)
        context.setLineWidth(ring)
        context.strokeEllipse(in: disc.insetBy(dx: ring / 2, dy: ring / 2))
        let label = "\(number)" as NSString
        let attrs: [NSAttributedString.Key: Any] = [
            .font: Fonts.mono(d * (number > 9 ? 0.42 : 0.5), weight: .medium), .foregroundColor: ink.color,
        ]
        let size = label.size(withAttributes: attrs)
        withAppKit(context) {
            label.draw(at: CGPoint(x: centre.x - size.width / 2, y: centre.y - size.height / 2), withAttributes: attrs)
        }
    }

    static func drawText(_ mark: Mark, scale: CGFloat, in context: CGContext) {
        guard !mark.text.isEmpty else { return }
        let box = Geometry.textBox(mark, scale: scale)
        let attrs = Geometry.textAttributes(mark, scale: scale)
        let font = Geometry.textFont(mark, scale: scale)
        // A soft halo in the contrasting colour, so text reads on any background.
        context.setShadow(offset: .zero, blur: font.pointSize / 7,
                          color: contrast(mark.style.colour).color.withAlphaComponent(0.8).cgColor)
        withAppKit(context) {
            (mark.text as NSString).draw(with: box, options: [.usesLineFragmentOrigin], attributes: attrs)
        }
    }

    /// Runs `body` with AppKit drawing into `context` (for text).
    static func withAppKit(_ context: CGContext, _ body: () -> Void) {
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: context, flipped: false)
        body()
        NSGraphicsContext.restoreGraphicsState()
    }
}
