// The marks, and the pointer and keys that make them. A drag is one undo step. A click on a mark selects it, and
// a drag then moves it; the selected mark's handles reshape it. A drawing tool can still draw inside a rectangle;
// the select tool picks it there. Measuring (M) draws over the shot but is never a mark: Esc clears it, and it is
// never exported.

import AppKit
import ShotsCore

final class MarksView: NSView {
    weak var canvas: CanvasView?
    private var drag: Drag = .none
    private(set) var text: TextMarkEditor?
    /// Measuring: where the pointer is, and a box being or last measured, in the shot's pixels.
    private(set) var hover: CGPoint?
    private(set) var measureBox: CGRect?

    enum Drag {
        case none
        case drawing(id: Int, start: CGPoint)
        case moving(id: Int, last: CGPoint)
        case resizing(id: Int, handle: Int)
        case cropNew(start: CGPoint)
        case cropMove(last: CGPoint)
        case cropResize(opposite: CGPoint)
        case measuring(start: CGPoint)
    }

    private var editor: EditorWindow? { canvas?.editor }

    override var acceptsFirstResponder: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    override var isOpaque: Bool { false }

    /// How near a click must be, in the shot's pixels: six points on screen at any zoom.
    private var tolerance: CGFloat {
        guard let editor else { return 6 }
        return 6 * editor.document.scale / max(editor.scroll.magnification, 0.05)
    }

    // MARK: Drawing

    override func draw(_ dirtyRect: NSRect) {
        guard let canvas, let editor, let ctx = NSGraphicsContext.current?.cgContext else { return }
        let doc = editor.document
        let layout = canvas.layoutInfo
        ctx.saveGState()
        ctx.scaleBy(x: 1 / doc.scale, y: 1 / doc.scale)
        if layout.cornerRadius > 0 {
            ctx.addPath(CGPath(roundedRect: layout.shotRect, cornerWidth: layout.cornerRadius,
                               cornerHeight: layout.cornerRadius, transform: nil))
            ctx.clip()
        } else {
            ctx.clip(to: layout.shotRect)
        }
        ctx.translateBy(x: layout.offset.dx, y: layout.offset.dy)
        Renderer.drawMarks(doc.canvas.marks, scale: doc.scale, base: editor.shot.image, redaction: editor.redaction,
                           in: ctx, skipping: text?.id)
        ctx.restoreGState()

        if let box = canvas.cropBox {
            drawCrop(box, in: ctx)
        } else if editor.tool == .measure {
            drawMeasure(in: ctx)
        } else if let id = doc.selected, let mark = doc.mark(id), text?.id != id {
            drawSelection(mark, in: ctx)
        }
    }

    // MARK: Measuring

    /// The pixel under a shot point, rows from the top, if it is on the shot.
    private func gridPixel(_ p: CGPoint) -> (x: Int, y: Int)? {
        guard let editor, let grid = editor.grid else { return nil }
        let x = Int(p.x.rounded(.down))
        let y = grid.height - 1 - Int(p.y.rounded(.down))
        guard x >= 0, y >= 0, x < grid.width, y < grid.height else { return nil }
        return (x, y)
    }

    /// The gaps around the pointer, as a span in pixels.
    func span(at p: CGPoint) -> Span? {
        guard let grid = editor?.grid, let px = gridPixel(p) else { return nil }
        return Edges.around(grid, x: px.x, y: px.y)
    }

    private func drawMeasure(in ctx: CGContext) {
        guard let canvas, let editor else { return }
        let scale = editor.document.scale
        let px = 1 / max(editor.scroll.magnification, 0.05)
        let ink = editor.palette.accent.color.cgColor
        ctx.saveGState()
        ctx.setStrokeColor(ink)
        ctx.setLineWidth(px)
        if let box = measureBox {
            let rect = canvas.viewRect(forShot: box)
            ctx.stroke(rect)
            ctx.restoreGState()
            pill(Ruler.size(Int(box.width.rounded()), Int(box.height.rounded()), scale: scale),
                 at: CGPoint(x: rect.midX, y: rect.maxY + 6 * px), px: px)
            return
        }
        guard let hover, let span = span(at: hover), let grid = editor.grid, let pixel = gridPixel(hover) else {
            ctx.restoreGState()
            return
        }
        // The run's ends in shot pixels (origin bottom-left), then on the canvas.
        let row = CGFloat(grid.height - 1 - pixel.y)
        let left = canvas.viewPoint(forShot: CGPoint(x: CGFloat(pixel.x - span.left), y: row + 0.5))
        let right = canvas.viewPoint(forShot: CGPoint(x: CGFloat(pixel.x + span.right + 1), y: row + 0.5))
        let top = canvas.viewPoint(forShot: CGPoint(x: CGFloat(pixel.x) + 0.5, y: row + 1 + CGFloat(span.up)))
        let bottom = canvas.viewPoint(forShot: CGPoint(x: CGFloat(pixel.x) + 0.5, y: row - CGFloat(span.down)))
        ctx.move(to: left)
        ctx.addLine(to: right)
        ctx.move(to: bottom)
        ctx.addLine(to: top)
        for end in [left, right] {
            ctx.move(to: CGPoint(x: end.x, y: end.y - 4 * px))
            ctx.addLine(to: CGPoint(x: end.x, y: end.y + 4 * px))
        }
        for end in [top, bottom] {
            ctx.move(to: CGPoint(x: end.x - 4 * px, y: end.y))
            ctx.addLine(to: CGPoint(x: end.x + 4 * px, y: end.y))
        }
        ctx.strokePath()
        ctx.restoreGState()
        pill(Ruler.label(span.width, scale: scale), at: CGPoint(x: (left.x + right.x) / 2, y: left.y + 6 * px), px: px)
        pill(Ruler.label(span.height, scale: scale), at: CGPoint(x: top.x + 8 * px, y: (top.y + bottom.y) / 2), px: px,
             leftAligned: true)
    }

    /// A mono label on the readout's glass, the same size on screen at any zoom (`px` is a screen point here).
    private func pill(_ words: String, at point: CGPoint, px: CGFloat, leftAligned: Bool = false) {
        guard let editor else { return }
        let palette = editor.palette
        let attrs: [NSAttributedString.Key: Any] = [
            .font: Fonts.mono(12 * px, weight: .medium), .foregroundColor: palette.glassText.color,
        ]
        let size = (words as NSString).size(withAttributes: attrs)
        let box = CGRect(x: leftAligned ? point.x : point.x - size.width / 2 - 6 * px, y: point.y,
                         width: size.width + 12 * px, height: size.height + 6 * px)
        palette.glass.color.withAlphaComponent(0.92).setFill()
        NSBezierPath(roundedRect: box, xRadius: Metrics.radiusChip * px, yRadius: Metrics.radiusChip * px).fill()
        (words as NSString).draw(at: CGPoint(x: box.minX + 6 * px, y: box.minY + 3 * px), withAttributes: attrs)
    }

    /// Copies a measurement, as the overlay's ruler does.
    private func copyMeasure(_ words: String) {
        guard let editor else { return }
        if editor.hooks.clipboard.put(text: words) {
            editor.hooks.note("Copied \(words)", false)
        } else {
            editor.hooks.note("Couldn't copy the size", true)
        }
    }

    func clearMeasure() {
        measureBox = nil
        needsDisplay = true
    }

    // MARK: Following the pointer

    override func updateTrackingAreas() {
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseMoved, .mouseEnteredAndExited,
                                                               .activeInKeyWindow, .inVisibleRect], owner: self))
        super.updateTrackingAreas()
    }

    override func mouseMoved(with event: NSEvent) {
        let p = shotPoint(event)
        pointer(at: p)
    }

    override func mouseExited(with event: NSEvent) {
        hover = nil
        editor?.pointerMoved(shot: nil)
        if editor?.tool == .measure { needsDisplay = true }
    }

    /// The pointer is over the shot at `p`: the footer's colour follows it, and so does the ruler.
    func pointer(at p: CGPoint) {
        guard let editor else { return }
        let onShot = editor.document.fullRect.contains(p)
        hover = onShot ? p : nil
        editor.pointerMoved(shot: onShot ? p : nil)
        if editor.tool == .measure, measureBox == nil { needsDisplay = true }
    }

    private func drawSelection(_ mark: Mark, in ctx: CGContext) {
        guard let canvas, let editor else { return }
        let accent = editor.palette.accent.color.cgColor
        let outline = canvas.viewRect(forShot: Geometry.bounds(mark, scale: editor.document.scale)).insetBy(dx: -2, dy: -2)
        ctx.saveGState()
        let px = 1 / max(editor.scroll.magnification, 0.05)
        ctx.setStrokeColor(accent)
        ctx.setLineWidth(px)
        ctx.setLineDash(phase: 0, lengths: [4 * px, 3 * px])
        ctx.stroke(outline)
        ctx.setLineDash(phase: 0, lengths: [])
        for handle in Geometry.handles(mark) {
            let p = canvas.viewPoint(forShot: handle)
            let r = 4 * px
            let square = CGRect(x: p.x - r, y: p.y - r, width: 2 * r, height: 2 * r)
            ctx.setFillColor(NSColor.white.cgColor)
            ctx.fill(square)
            ctx.setLineWidth(1.5 * px)
            ctx.stroke(square)
        }
        ctx.restoreGState()
    }

    private func drawCrop(_ box: CGRect, in ctx: CGContext) {
        guard let canvas, let editor else { return }
        let rect = canvas.viewRect(forShot: box)
        let px = 1 / max(editor.scroll.magnification, 0.05)
        ctx.saveGState()
        ctx.addRect(bounds)
        ctx.addRect(rect)
        ctx.setFillColor(NSColor.black.withAlphaComponent(0.5).cgColor)
        ctx.fillPath(using: .evenOdd)
        ctx.setStrokeColor(NSColor.white.cgColor)
        ctx.setLineWidth(px)
        ctx.stroke(rect)
        // Corner brackets: white on a dark edge, so they show on any shot.
        let arm = min(18 * px, rect.width / 3, rect.height / 3)
        for (colour, width) in [(NSColor.black.withAlphaComponent(0.45), 5 * px), (NSColor.white, 3 * px)] {
            ctx.setStrokeColor(colour.cgColor)
            ctx.setLineWidth(width)
            for (corner, dx, dy) in [(CGPoint(x: rect.minX, y: rect.minY), 1.0, 1.0),
                                     (CGPoint(x: rect.maxX, y: rect.minY), -1.0, 1.0),
                                     (CGPoint(x: rect.maxX, y: rect.maxY), -1.0, -1.0),
                                     (CGPoint(x: rect.minX, y: rect.maxY), 1.0, -1.0)] {
                ctx.move(to: CGPoint(x: corner.x + dx * arm, y: corner.y))
                ctx.addLine(to: corner)
                ctx.addLine(to: CGPoint(x: corner.x, y: corner.y + dy * arm))
            }
            ctx.strokePath()
        }
        ctx.restoreGState()
    }

    // MARK: The pointer

    override func resetCursorRects() {
        guard let editor else { return }
        switch editor.tool {
        case .select: addCursorRect(bounds, cursor: .arrow)
        case .text: addCursorRect(bounds, cursor: .iBeam)
        default: addCursorRect(bounds, cursor: .crosshair)
        }
    }

    private func shotPoint(_ event: NSEvent) -> CGPoint {
        canvas?.shotPoint(forView: convert(event.locationInWindow, from: nil)) ?? .zero
    }

    override func mouseDown(with event: NSEvent) {
        guard let editor, let canvas else { return }
        // A click while typing ends the typing, and only that.
        if text != nil {
            endTextEditing()
            return
        }
        window?.makeFirstResponder(self)
        let doc = editor.document
        let p = shotPoint(event)
        if let box = canvas.cropBox {
            cropDown(at: p, box: box)
            return
        }
        if editor.tool == .measure {
            measureBox = nil
            drag = .measuring(start: p)
            needsDisplay = true
            return
        }
        // Double-click a text mark to type in it.
        if event.clickCount == 2, let id = doc.hitTest(p, tolerance: tolerance, interiors: true),
           doc.mark(id)?.kind == .text {
            beginTextEditing(id, isNew: false)
            return
        }
        // A handle of the selected mark.
        if let id = doc.selected, let mark = doc.mark(id),
           let handle = Geometry.handles(mark).firstIndex(where: { hypot($0.x - p.x, $0.y - p.y) <= tolerance * 1.2 }) {
            drag = .resizing(id: id, handle: handle)
            return
        }
        // A mark.
        if let id = doc.hitTest(p, tolerance: tolerance, interiors: editor.tool == .select) {
            doc.selected = id
            drag = .moving(id: id, last: p)
            editor.changed()
            return
        }
        doc.selected = nil
        guard let kind = editor.tool.kind else {
            editor.changed()
            return
        }
        let id = doc.newId()
        let points = kind == .step || kind == .text || kind.isPath ? [p] : [p, p]
        doc.live { $0.marks.append(Mark(id: id, kind: kind, points: points, style: editor.style)) }
        switch kind {
        case .text:
            beginTextEditing(id, isNew: true)
        case .step:
            doc.selected = id
            drag = .moving(id: id, last: p)
        default:
            drag = .drawing(id: id, start: p)
        }
        editor.changed()
    }

    override func mouseDragged(with event: NSEvent) {
        guard let editor else { return }
        let doc = editor.document
        let p = shotPoint(event)
        let shift = event.modifierFlags.contains(.shift)
        switch drag {
        case .none:
            return
        case let .drawing(id, start):
            doc.updateLive(id) { mark in
                if mark.kind.isPath {
                    if shift {
                        mark.points = [start, p]
                    } else if let last = mark.points.last,
                              hypot(p.x - last.x, p.y - last.y) >= 1.5 * doc.scale / max(editor.scroll.magnification, 0.05) {
                        mark.points.append(p)
                    }
                } else if mark.kind.isSegment {
                    mark.points = [start, shift ? Geometry.snapped(p, from: start) : p]
                } else {
                    mark.points = [start, shift ? Geometry.squared(p, from: start) : p]
                }
            }
        case let .moving(id, last):
            doc.updateLive(id) { $0 = Geometry.moved($0, by: CGVector(dx: p.x - last.x, dy: p.y - last.y)) }
            drag = .moving(id: id, last: p)
        case let .resizing(id, handle):
            doc.updateLive(id) { $0 = Geometry.resized($0, handle: handle, to: p, square: shift) }
        case let .cropNew(start):
            canvas?.cropBox = Geometry.box(start, clamp(shift ? Geometry.squared(p, from: start) : p))
        case let .cropMove(last):
            guard let box = canvas?.cropBox else { return }
            var moved = box.offsetBy(dx: p.x - last.x, dy: p.y - last.y)
            moved.origin.x = min(max(moved.minX, 0), doc.size.width - moved.width)
            moved.origin.y = min(max(moved.minY, 0), doc.size.height - moved.height)
            canvas?.cropBox = moved
            drag = .cropMove(last: p)
        case let .cropResize(opposite):
            canvas?.cropBox = Geometry.box(opposite, clamp(shift ? Geometry.squared(p, from: opposite) : p))
        case let .measuring(start):
            measureBox = Geometry.box(clamp(start), clamp(shift ? Geometry.squared(p, from: start) : p))
        }
        needsDisplay = true
        editor.footer.refresh()
    }

    override func mouseUp(with event: NSEvent) {
        guard let editor else { return }
        let doc = editor.document
        switch drag {
        case let .drawing(id, _):
            if let mark = doc.mark(id), MarksView.tooSmall(mark, scale: doc.scale) {
                doc.cancelLive()
            } else {
                doc.end()
                doc.selected = id
            }
        case .moving, .resizing:
            doc.end()
        case .cropNew, .cropMove, .cropResize:
            if let box = canvas?.cropBox, box.width < 4 || box.height < 4 {
                canvas?.cropBox = doc.visibleRect
            }
        case let .measuring(start):
            // A drag measures a box; a click, the gaps around the pointer. Either is copied.
            if let box = measureBox, box.width >= 1, box.height >= 1 {
                copyMeasure(Ruler.size(Int(box.width.rounded()), Int(box.height.rounded()), scale: doc.scale))
            } else if let span = span(at: start) {
                measureBox = nil
                copyMeasure(Ruler.size(span.width, span.height, scale: doc.scale))
            }
        case .none:
            break
        }
        drag = .none
        editor.changed()
    }

    /// A click without a drag makes no line or box (a pen's dot is fine).
    static func tooSmall(_ mark: Mark, scale: CGFloat) -> Bool {
        if mark.kind.isPath { return false }
        if mark.kind.isSegment, mark.points.count > 1 {
            return hypot(mark.points[1].x - mark.points[0].x, mark.points[1].y - mark.points[0].y) < 3 * scale
        }
        if mark.kind.isBox { return mark.box.width < 3 * scale || mark.box.height < 3 * scale }
        return false
    }

    private func clamp(_ p: CGPoint) -> CGPoint {
        guard let size = editor?.document.size else { return p }
        return CGPoint(x: min(max(p.x, 0), size.width), y: min(max(p.y, 0), size.height))
    }

    private func cropDown(at p: CGPoint, box: CGRect) {
        let corners = [CGPoint(x: box.minX, y: box.minY), CGPoint(x: box.maxX, y: box.minY),
                       CGPoint(x: box.maxX, y: box.maxY), CGPoint(x: box.minX, y: box.maxY)]
        if let i = corners.firstIndex(where: { hypot($0.x - p.x, $0.y - p.y) <= tolerance * 2 }) {
            drag = .cropResize(opposite: corners[(i + 2) % 4])
        } else if box.contains(p), box != editor?.document.fullRect {
            drag = .cropMove(last: p)
        } else {
            drag = .cropNew(start: clamp(p))
        }
    }

    // MARK: Keys

    override func keyDown(with event: NSEvent) {
        guard let editor, let canvas else { return }
        let doc = editor.document
        let flags = event.modifierFlags.intersection([.command, .control, .option])
        switch event.keyCode {
        case 53:  // Esc
            if case .none = drag {} else {
                doc.cancelLive()
                drag = .none
            }
            if measureBox != nil {
                clearMeasure()
            } else if canvas.cropBox != nil {
                canvas.cropBox = doc.visibleRect
                editor.select(.arrow)
            } else if doc.selected != nil {
                doc.selected = nil
            } else {
                editor.requestClose()
            }
            editor.changed()
        case 36, 76:  // Return
            if canvas.cropBox != nil {
                editor.select(.arrow)
            } else if let id = doc.selected, doc.mark(id)?.kind == .text {
                beginTextEditing(id, isNew: false)
            }
        case 48:  // Tab: the colour under the pointer.
            editor.copyColour(css: event.modifierFlags.contains(.shift))
        case 51, 117:  // Delete
            if let id = doc.selected {
                doc.delete(id)
                editor.changed()
            }
        case 123, 124, 125, 126:  // Arrows nudge the selection by a point, ⇧ by ten.
            guard let id = doc.selected else { return }
            let step = (event.modifierFlags.contains(.shift) ? 10 : 1) * doc.scale
            let delta: CGVector = [123: CGVector(dx: -step, dy: 0), 124: CGVector(dx: step, dy: 0),
                                   125: CGVector(dx: 0, dy: -step), 126: CGVector(dx: 0, dy: step)][event.keyCode]
                ?? .zero
            doc.update(id) { $0 = Geometry.moved($0, by: delta) }
            editor.changed()
        default:
            guard flags.isEmpty, let key = event.charactersIgnoringModifiers?.first else {
                super.keyDown(with: event)
                return
            }
            if key == "[" {
                editor.setStyle(Style(colour: editor.style.colour, width: editor.style.width.smaller))
            } else if key == "]" {
                editor.setStyle(Style(colour: editor.style.colour, width: editor.style.width.bigger))
            } else if let tool = Tool.forKey(key) {
                editor.select(tool)
            } else {
                super.keyDown(with: event)
            }
        }
    }

    // MARK: Text

    func beginTextEditing(_ id: Int, isNew: Bool) {
        guard let editor, let canvas else { return }
        endTextEditing()
        if !isNew { editor.document.live { _ in } }
        editor.document.selected = id
        text = TextMarkEditor(id: id, isNew: isNew, editor: editor, canvas: canvas)
        needsDisplay = true
    }

    /// Ends typing, keeping what was typed (an empty mark goes).
    func endTextEditing() {
        guard let text else { return }
        self.text = nil
        text.finish()
        needsDisplay = true
        editor?.changed()
    }
}
