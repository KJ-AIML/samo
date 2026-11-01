// The capture overlay on one display (ADR-004 S1): the frozen screen, dimmed, with the selection clear. A crosshair
// and a magnifier follow the pointer; the selection's size is shown in pixels.
//
// Keys, as macOS's own ⌘⇧4 has them: drag to select (⇧ keeps a square; Space held while dragging moves it; arrows
// nudge its corner by a point, ⇧ by ten); release, or Return, captures. Space before dragging switches to windows.
// Esc cancels. S, T, C and R are for later versions.

import AppKit
import ScreenCaptureKit
import ShotsCore

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
    enum Mode {
        case area
        case window
    }

    let frozen: Frozen
    private let palette: Palette
    private unowned let session: OverlaySession

    var mode: Mode = .area {
        didSet { needsDisplay = true }
    }
    var pickable: [Pickable] = [] {
        didSet { needsDisplay = true }
    }

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

    private var selection: CGRect? {
        guard let anchor, let current else { return nil }
        return Selection.clamp(Selection.rect(from: anchor, to: current, square: square), to: bounds)
    }

    private var hovered: Pickable? {
        guard mode == .window, let pointer else { return nil }
        return pickable.first { $0.rect.contains(pointer) }
    }

    // MARK: Drawing

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        ctx.interpolationQuality = .none
        ctx.draw(frozen.image, in: bounds)

        // Dim everything but what would be captured.
        let clear = selection ?? hovered?.rect
        ctx.saveGState()
        ctx.addRect(bounds)
        if let clear { ctx.addRect(clear) }
        ctx.setFillColor(NSColor.black.withAlphaComponent(0.38).cgColor)
        ctx.fillPath(using: .evenOdd)
        ctx.restoreGState()

        if let hovered {
            ctx.setFillColor(palette.accent.color.withAlphaComponent(0.14).cgColor)
            ctx.fill(hovered.rect)
            ctx.setStrokeColor(palette.accent.color.cgColor)
            ctx.setLineWidth(2)
            ctx.stroke(hovered.rect.insetBy(dx: 1, dy: 1))
        }
        if let selection {
            ctx.setStrokeColor(palette.glassText.color.cgColor)
            ctx.setLineWidth(1)
            ctx.stroke(selection.insetBy(dx: -0.5, dy: -0.5))
            let pixels = Selection.imageRect(selection, viewHeight: bounds.height, scale: frozen.scale)
            label("\(Int(pixels.width)) × \(Int(pixels.height))", near: CGPoint(x: selection.maxX, y: selection.minY))
        } else if mode == .area, let pointer {
            ctx.setStrokeColor(palette.glassText.color.withAlphaComponent(0.55).cgColor)
            ctx.setLineWidth(1)
            ctx.move(to: CGPoint(x: 0, y: pointer.y.rounded() + 0.5))
            ctx.addLine(to: CGPoint(x: bounds.maxX, y: pointer.y.rounded() + 0.5))
            ctx.move(to: CGPoint(x: pointer.x.rounded() + 0.5, y: 0))
            ctx.addLine(to: CGPoint(x: pointer.x.rounded() + 0.5, y: bounds.maxY))
            ctx.strokePath()
        }
        if mode == .area, let at = current ?? pointer {
            magnifier(ctx, at: at)
        }
        hint()
    }

    /// A pill of mono text on the readout's glass.
    private func label(_ text: String, near point: CGPoint) {
        let attrs: [NSAttributedString.Key: Any] = [
            .font: Fonts.mono(12, weight: .medium), .foregroundColor: palette.glassText.color,
        ]
        let size = (text as NSString).size(withAttributes: attrs)
        var box = CGRect(x: point.x - size.width - 14, y: point.y - size.height - 12, width: size.width + 12,
                         height: size.height + 6)
        if box.minY < bounds.minY + 4 { box.origin.y = point.y + 6 }
        if box.minX < bounds.minX + 4 { box.origin.x = bounds.minX + 4 }
        let path = NSBezierPath(roundedRect: box, xRadius: Metrics.radiusChip, yRadius: Metrics.radiusChip)
        palette.glass.color.withAlphaComponent(0.92).setFill()
        path.fill()
        (text as NSString).draw(at: CGPoint(x: box.minX + 6, y: box.minY + 3), withAttributes: attrs)
    }

    /// Fourteen pixels around the pointer, eight times over, with the pixel under it outlined.
    private func magnifier(_ ctx: CGContext, at point: CGPoint) {
        let side: CGFloat = 112
        let span = 14
        let px = Selection.imageRect(CGRect(origin: point, size: .zero), viewHeight: bounds.height, scale: frozen.scale)
        let source = CGRect(x: px.minX - CGFloat(span / 2), y: px.minY - CGFloat(span / 2),
                            width: CGFloat(span), height: CGFloat(span))
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
        label("\(Int(px.minX)), \(Int(px.minY))", near: CGPoint(x: frame.maxX, y: frame.minY))
    }

    private func hint() {
        let words = mode == .area
            ? "Drag to capture  ·  Space for a window  ·  Esc to cancel"
            : "Click a window  ·  Space for an area  ·  Esc to cancel"
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
        case .area:
            anchor = p
            current = p
            lastDrag = p
            square = event.modifierFlags.contains(.shift)
        case .window:
            if let hovered {
                session.pick(hovered, on: self)
            }
        }
        needsDisplay = true
    }

    override func mouseDragged(with event: NSEvent) {
        guard mode == .area else { return }
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
        guard mode == .area else { return }
        finishDrag()
    }

    private func finishDrag() {
        defer {
            anchor = nil
            current = nil
            needsDisplay = true
        }
        guard let rect = selection, rect.width >= Selection.minimum, rect.height >= Selection.minimum else { return }
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
        case 49:  // Space
            if anchor != nil {
                spaceHeld = true
            } else if !event.isARepeat {
                session.toggleMode()
            }
        case 36, 76:  // Return, Enter
            finishDrag()
        case 123, 124, 125, 126:
            let arrow: Arrow = [123: .left, 124: .right, 125: .down, 126: .up][event.keyCode] ?? .up
            if let c = current {
                current = Selection.nudge(c, arrow, big: big)
            } else if let p = pointer {
                pointer = Selection.nudge(p, arrow, big: big)
            }
            needsDisplay = true
        case 1, 17, 8, 15:  // S, T, C, R
            session.note("Coming in a later version of Shots")
        default:
            super.keyDown(with: event)
        }
    }

    override func keyUp(with event: NSEvent) {
        if event.keyCode == 49 {
            spaceHeld = false
        }
    }
}
