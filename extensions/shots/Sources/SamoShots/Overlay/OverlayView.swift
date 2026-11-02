// The capture overlay on one display (ADR-004 S1, S4): the frozen screen, with a crosshair and a magnifier.
//
// Keys, as macOS's own ⇧⌘4 has them: drag to select (⇧ keeps a square; Space held while dragging moves it; arrows
// nudge its corner by a point, ⇧ by ten); release, or Return, captures. Before dragging: Space switches to windows,
// T to copying text, C to picking a colour, R to the ruler (pressed again, back to capturing). Esc cancels. S is
// for a later version.
//
// Picking a colour and measuring show the screen undimmed, since they need its true colours.

import AppKit
import ScreenCaptureKit
import ShotsCore
import ShotsUI

final class OverlayPanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }
}

/// A window the pointer can pick, in this display's local points.
struct Pickable {
    let rect: CGRect
    let window: SCWindow
}

final class OverlayView: NSView {
    let frozen: Frozen
    private let palette: Palette
    private unowned let session: OverlaySession

    var mode: OverlayMode = .area {
        didSet { needsDisplay = true }
    }
    var pickable: [Pickable] = [] {
        didSet { needsDisplay = true }
    }

    /// The frozen screen's pixels in sRGB, made when picking a colour or measuring first needs them.
    private(set) lazy var grid: PixelGrid? = PixelGrid(frozen.image)

    private var pointer: CGPoint?
    private var anchor: CGPoint?
    private var current: CGPoint?
    private var lastDrag: CGPoint?
    private var spaceHeld = false
    private var square = false

    init(frozen: Frozen, palette: Palette, session: OverlaySession) {
        self.frozen = frozen
        self.palette = palette
        self.session = session
        super.init(frame: CGRect(origin: .zero, size: frozen.screen.frame.size))
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var acceptsFirstResponder: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func updateTrackingAreas() {
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(
            rect: bounds, options: [.mouseMoved, .activeAlways, .inVisibleRect, .cursorUpdate], owner: self
        ))
        super.updateTrackingAreas()
    }

    override func cursorUpdate(with event: NSEvent) {
        NSCursor.crosshair.set()
    }

    var dragging: Bool { anchor != nil }

    private var selection: CGRect? {
        guard let anchor, let current else { return nil }
        return Selection.clamp(Selection.rect(from: anchor, to: current, square: square), to: bounds)
    }

    private var hovered: Pickable? {
        guard mode == .window, let pointer else { return nil }
        return pickable.first { $0.rect.contains(pointer) }
    }

    /// The image pixel under a point.
    private func pixel(_ p: CGPoint) -> (x: Int, y: Int) {
        Selection.pixel(at: p, viewHeight: bounds.height, scale: frozen.scale,
                        size: (frozen.image.width, frozen.image.height))
    }

    // MARK: Drawing

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        ctx.interpolationQuality = .none
        ctx.draw(frozen.image, in: bounds)

        if mode.dims {
            // Dim everything but what would be captured.
            let clear = selection ?? hovered?.rect
            ctx.saveGState()
            ctx.addRect(bounds)
            if let clear { ctx.addRect(clear) }
            ctx.setFillColor(NSColor.black.withAlphaComponent(0.38).cgColor)
            ctx.fillPath(using: .evenOdd)
            ctx.restoreGState()
        }

        if let hovered {
            ctx.setFillColor(palette.accent.color.withAlphaComponent(0.14).cgColor)
            ctx.fill(hovered.rect)
            ctx.setStrokeColor(palette.accent.color.cgColor)
            ctx.setLineWidth(2)
            ctx.stroke(hovered.rect.insetBy(dx: 1, dy: 1))
        }
        if mode == .ruler {
            drawRuler(ctx)
        } else if let selection {
            ctx.setStrokeColor(palette.glassText.color.cgColor)
            ctx.setLineWidth(1)
            ctx.stroke(selection.insetBy(dx: -0.5, dy: -0.5))
            let pixels = Selection.imageRect(selection, viewHeight: bounds.height, scale: frozen.scale)
            label("\(Int(pixels.width)) × \(Int(pixels.height))", near: CGPoint(x: selection.maxX, y: selection.minY))
        } else if mode != .window, let pointer {
            crosshair(ctx, at: pointer)
        }
        if mode.magnifies, let at = current ?? pointer {
            magnifier(ctx, at: at)
        }
        hint()
    }

    private func crosshair(_ ctx: CGContext, at pointer: CGPoint) {
        ctx.setStrokeColor(palette.glassText.color.withAlphaComponent(0.55).cgColor)
        ctx.setLineWidth(1)
        ctx.move(to: CGPoint(x: 0, y: pointer.y.rounded() + 0.5))
        ctx.addLine(to: CGPoint(x: bounds.maxX, y: pointer.y.rounded() + 0.5))
        ctx.move(to: CGPoint(x: pointer.x.rounded() + 0.5, y: 0))
        ctx.addLine(to: CGPoint(x: pointer.x.rounded() + 0.5, y: bounds.maxY))
        ctx.strokePath()
    }

    /// A pill of mono text on the readout's glass. `swatch` adds a square of that colour before the text; `above`
    /// centres it above the point.
    private func label(_ text: String, near point: CGPoint, swatch: RGBA? = nil, above: Bool = false) {
        let attrs: [NSAttributedString.Key: Any] = [
            .font: Fonts.mono(12, weight: .medium), .foregroundColor: palette.glassText.color,
        ]
        let size = (text as NSString).size(withAttributes: attrs)
        let lead: CGFloat = swatch == nil ? 0 : size.height + 4
        var box = CGRect(x: point.x - size.width - lead - 14, y: point.y - size.height - 12,
                         width: size.width + lead + 12, height: size.height + 6)
        if above { box.origin = CGPoint(x: point.x - box.width / 2, y: point.y + 6) }
        if box.minY < bounds.minY + 4 { box.origin.y = point.y + 6 }
        if box.maxY > bounds.maxY - 4 { box.origin.y = bounds.maxY - 4 - box.height }
        if box.minX < bounds.minX + 4 { box.origin.x = bounds.minX + 4 }
        if box.maxX > bounds.maxX - 4 { box.origin.x = bounds.maxX - 4 - box.width }
        let path = NSBezierPath(roundedRect: box, xRadius: Metrics.radiusChip, yRadius: Metrics.radiusChip)
        palette.glass.color.withAlphaComponent(0.92).setFill()
        path.fill()
        if let swatch {
            let square = CGRect(x: box.minX + 6, y: box.minY + 3, width: size.height, height: size.height)
            swatch.color.setFill()
            NSBezierPath(roundedRect: square, xRadius: 2, yRadius: 2).fill()
            palette.glassLine.color.setStroke()
            NSBezierPath(roundedRect: square.insetBy(dx: 0.5, dy: 0.5), xRadius: 2, yRadius: 2).stroke()
        }
        (text as NSString).draw(at: CGPoint(x: box.minX + 6 + lead, y: box.minY + 3), withAttributes: attrs)
    }

    /// Fourteen pixels around the pointer, eight times over, with the pixel under it outlined. Picking a colour, it
    /// says the colour; otherwise where the pixel is.
    private func magnifier(_ ctx: CGContext, at point: CGPoint) {
        let side: CGFloat = 112
        let span = 14
        let px = pixel(point)
        let source = CGRect(x: px.x - span / 2, y: px.y - span / 2, width: span, height: span)
        guard let crop = frozen.image.cropping(to: source) else { return }
        var origin = CGPoint(x: point.x + 18, y: point.y - side - 18)
        if origin.x + side > bounds.maxX - 4 { origin.x = point.x - side - 18 }
        if origin.y < bounds.minY + 4 { origin.y = point.y + 18 }
        let frame = CGRect(origin: origin, size: CGSize(width: side, height: side))
        ctx.saveGState()
        let clip = CGPath(roundedRect: frame, cornerWidth: Metrics.radiusControl, cornerHeight: Metrics.radiusControl,
                          transform: nil)
        ctx.addPath(clip)
        ctx.clip()
        ctx.interpolationQuality = .none
        ctx.draw(crop, in: frame)
        let cell = side / CGFloat(span)
        ctx.setStrokeColor(palette.accent.color.cgColor)
        ctx.setLineWidth(1.5)
        ctx.stroke(CGRect(x: frame.midX - cell / 2, y: frame.midY - cell / 2, width: cell, height: cell))
        ctx.restoreGState()
        ctx.addPath(clip)
        ctx.setStrokeColor(palette.glassLine.color.cgColor)
        ctx.setLineWidth(1)
        ctx.strokePath()
        if mode == .colour, let colour = grid?.pixel(px.x, px.y) {
            label(colour.hex, near: CGPoint(x: frame.maxX, y: frame.minY), swatch: colour.rgba)
        } else {
            label("\(px.x), \(px.y)", near: CGPoint(x: frame.maxX, y: frame.minY))
        }
    }

    /// The ruler: hovering, lines to the nearest edges each way with their lengths; dragging, the box's size.
    private func drawRuler(_ ctx: CGContext) {
        let ink = palette.phosphor.color.cgColor
        if let selection {
            ctx.setStrokeColor(ink)
            ctx.setLineWidth(1)
            ctx.stroke(selection.insetBy(dx: -0.5, dy: -0.5))
            let pixels = Selection.imageRect(selection, viewHeight: bounds.height, scale: frozen.scale)
            label(Ruler.size(Int(pixels.width), Int(pixels.height), scale: frozen.scale),
                  near: CGPoint(x: selection.maxX, y: selection.minY))
            return
        }
        guard let pointer, let grid else { return }
        let px = pixel(pointer)
        guard let span = Edges.around(grid, x: px.x, y: px.y) else { return }
        let scale = frozen.scale
        let left = CGFloat(px.x - span.left) / scale
        let right = CGFloat(px.x + span.right + 1) / scale
        let top = bounds.height - CGFloat(px.y - span.up) / scale
        let bottom = bounds.height - CGFloat(px.y + span.down + 1) / scale
        let y = pointer.y.rounded() + 0.5
        let x = pointer.x.rounded() + 0.5
        ctx.setStrokeColor(ink)
        ctx.setLineWidth(1)
        ctx.move(to: CGPoint(x: left, y: y))
        ctx.addLine(to: CGPoint(x: right, y: y))
        ctx.move(to: CGPoint(x: x, y: bottom))
        ctx.addLine(to: CGPoint(x: x, y: top))
        // Ticks at the edges.
        for edge in [left, right] {
            ctx.move(to: CGPoint(x: edge, y: y - 4))
            ctx.addLine(to: CGPoint(x: edge, y: y + 4))
        }
        for edge in [top, bottom] {
            ctx.move(to: CGPoint(x: x - 4, y: edge))
            ctx.addLine(to: CGPoint(x: x + 4, y: edge))
        }
        ctx.strokePath()
        label(Ruler.label(span.width, scale: scale), near: CGPoint(x: (left + right) / 2, y: y), above: true)
        label(Ruler.label(span.height, scale: scale), near: CGPoint(x: x - 8, y: (top + bottom) / 2))
    }

    private func hint() {
        let words = mode.hint
        let attrs: [NSAttributedString.Key: Any] = [
            .font: Fonts.body(Metrics.textMeta, weight: .medium), .foregroundColor: palette.glassText2.color,
        ]
        let size = (words as NSString).size(withAttributes: attrs)
        let box = CGRect(x: bounds.midX - size.width / 2 - 14, y: bounds.minY + 28, width: size.width + 28,
                         height: size.height + 14)
        palette.glass.color.withAlphaComponent(0.92).setFill()
        NSBezierPath(roundedRect: box, xRadius: Metrics.radiusControl, yRadius: Metrics.radiusControl).fill()
        (words as NSString).draw(at: CGPoint(x: box.minX + 14, y: box.minY + 7), withAttributes: attrs)
    }

    // MARK: Mouse

    private func local(_ event: NSEvent) -> CGPoint {
        convert(event.locationInWindow, from: nil)
    }

    override func mouseMoved(with event: NSEvent) {
        pointer = local(event)
        needsDisplay = true
    }

    override func mouseDown(with event: NSEvent) {
        let p = local(event)
        switch mode {
        case .area, .text, .ruler:
            anchor = p
            current = p
            lastDrag = p
            square = event.modifierFlags.contains(.shift)
        case .window:
            if let hovered {
                session.pick(hovered, on: self)
            }
        case .colour:
            let px = pixel(p)
            if let colour = grid?.pixel(px.x, px.y) {
                session.pickColour(colour, css: event.modifierFlags.contains(.shift))
            }
        }
        needsDisplay = true
    }

    override func mouseDragged(with event: NSEvent) {
        guard mode.drags, anchor != nil else { return }
        let p = local(event)
        if spaceHeld, let rect = selection, let last = lastDrag {
            let moved = Selection.moved(rect, by: CGVector(dx: p.x - last.x, dy: p.y - last.y), within: bounds)
            anchor = moved.origin
            current = CGPoint(x: moved.maxX, y: moved.maxY)
        } else {
            current = p
        }
        lastDrag = p
        pointer = p
        square = event.modifierFlags.contains(.shift)
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        guard mode.drags, anchor != nil else { return }
        finishDrag()
    }

    private func finishDrag() {
        defer {
            anchor = nil
            current = nil
            needsDisplay = true
        }
        let rect = selection
        if mode == .ruler {
            // A drag measures a box; a click, the gaps around the pointer.
            if let rect, rect.width >= 1, rect.height >= 1 {
                let pixels = Selection.imageRect(rect, viewHeight: bounds.height, scale: frozen.scale)
                session.measured(Ruler.size(Int(pixels.width), Int(pixels.height), scale: frozen.scale))
            } else if let pointer, let grid {
                let px = pixel(pointer)
                if let span = Edges.around(grid, x: px.x, y: px.y) {
                    session.measured(Ruler.size(span.width, span.height, scale: frozen.scale))
                }
            }
            return
        }
        guard let rect, rect.width >= Selection.minimum, rect.height >= Selection.minimum else { return }
        session.finish(rect, on: self)
    }

    // MARK: Keys

    override func flagsChanged(with event: NSEvent) {
        square = event.modifierFlags.contains(.shift)
        needsDisplay = true
    }

    override func keyDown(with event: NSEvent) {
        let big = event.modifierFlags.contains(.shift)
        switch event.keyCode {
        case 53:  // Esc
            session.cancel()
        case 49 where dragging:  // Space while dragging moves the selection.
            spaceHeld = true
        case 36, 76:  // Return, Enter
            if mode.drags, anchor != nil { finishDrag() }
        case 123, 124, 125, 126:
            let arrow: Arrow = [123: .left, 124: .right, 125: .down, 126: .up][event.keyCode] ?? .up
            if let c = current {
                current = Selection.nudge(c, arrow, big: big)
            } else if let p = pointer {
                pointer = Selection.nudge(p, arrow, big: big)
            }
            needsDisplay = true
        default:
            guard !event.isARepeat, let key = event.charactersIgnoringModifiers?.first else {
                super.keyDown(with: event)
                return
            }
            session.key(key, dragging: dragging)
        }
    }

    override func keyUp(with event: NSEvent) {
        if event.keyCode == 49 {
            spaceHeld = false
        }
    }
}
