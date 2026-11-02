// The editor's geometry: boxes from drags, ⇧'s straight lines and squares, the arrow's shape, what a click hits,
// and the handles that resize a mark. Everything is in the shot's pixels.

import AppKit
import CoreGraphics

public enum Geometry {
    public static func box(_ a: CGPoint, _ b: CGPoint) -> CGRect {
        CGRect(x: min(a.x, b.x), y: min(a.y, b.y), width: abs(b.x - a.x), height: abs(b.y - a.y))
    }

    /// `end` turned to the nearest 45° from `start` (⇧ on a line or an arrow).
    public static func snapped(_ end: CGPoint, from start: CGPoint) -> CGPoint {
        let dx = end.x - start.x
        let dy = end.y - start.y
        let length = hypot(dx, dy)
        guard length > 0 else { return end }
        let step = CGFloat.pi / 4
        let angle = (atan2(dy, dx) / step).rounded() * step
        return CGPoint(x: start.x + cos(angle) * length, y: start.y + sin(angle) * length)
    }

    /// `end` moved so the box from `start` is square (⇧ on a box), growing the way the drag went.
    public static func squared(_ end: CGPoint, from start: CGPoint) -> CGPoint {
        let side = max(abs(end.x - start.x), abs(end.y - start.y))
        return CGPoint(x: start.x + (end.x >= start.x ? side : -side), y: start.y + (end.y >= start.y ? side : -side))
    }

    public static func distance(_ p: CGPoint, toSegment a: CGPoint, _ b: CGPoint) -> CGFloat {
        let dx = b.x - a.x
        let dy = b.y - a.y
        let lengthSquared = dx * dx + dy * dy
        guard lengthSquared > 0 else { return hypot(p.x - a.x, p.y - a.y) }
        let t = max(0, min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / lengthSquared))
        return hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy))
    }

    // MARK: Sizes

    /// A mark's line width in pixels.
    public static func lineWidth(_ mark: Mark, scale: CGFloat) -> CGFloat {
        switch mark.kind {
        case .highlighter: return mark.style.width.highlighter * scale
        case .arrow: return mark.style.width.stroke * 1.25 * scale
        default: return mark.style.width.stroke * scale
        }
    }

    public static func stepDiameter(_ mark: Mark, scale: CGFloat) -> CGFloat {
        mark.style.width.stepDiameter * scale
    }

    /// Text marks' face: the body face, semibold, at the mark's size in pixels.
    public static func textFont(_ mark: Mark, scale: CGFloat) -> NSFont {
        Fonts.body(mark.style.width.textSize * scale, weight: .semibold)
    }

    public static func textAttributes(_ mark: Mark, scale: CGFloat) -> [NSAttributedString.Key: Any] {
        [.font: textFont(mark, scale: scale), .foregroundColor: mark.style.colour.color]
    }

    /// The text's box: its top left corner is the mark's point.
    public static func textBox(_ mark: Mark, scale: CGFloat) -> CGRect {
        guard let topLeft = mark.points.first else { return .null }
        let shown = mark.text.isEmpty ? " " : mark.text
        let size = (shown as NSString).boundingRect(
            with: CGSize(width: CGFloat.greatestFiniteMagnitude, height: .greatestFiniteMagnitude),
            options: [.usesLineFragmentOrigin], attributes: textAttributes(mark, scale: scale)
        ).size
        return CGRect(x: topLeft.x, y: topLeft.y - ceil(size.height), width: ceil(size.width), height: ceil(size.height))
    }

    /// Everything a mark paints, stroke and halo included.
    public static func bounds(_ mark: Mark, scale: CGFloat) -> CGRect {
        let width = lineWidth(mark, scale: scale)
        switch mark.kind {
        case .arrow:
            guard mark.points.count > 1 else { return .null }
            let head = arrowHead(width: width, length: hypot(mark.points[1].x - mark.points[0].x,
                                                             mark.points[1].y - mark.points[0].y))
            return box(mark.points[0], mark.points[1]).insetBy(dx: -head.width, dy: -head.width)
        case .line, .pen, .highlighter:
            guard let first = mark.points.first else { return .null }
            let all = mark.points.reduce(CGRect(origin: first, size: .zero)) { $0.union(CGRect(origin: $1, size: .zero)) }
            return all.insetBy(dx: -width, dy: -width)
        case .rectangle, .ellipse:
            return mark.box.insetBy(dx: -width, dy: -width)
        case .blur, .pixelate, .spotlight:
            return mark.box
        case .step:
            guard let centre = mark.points.first else { return .null }
            let r = stepDiameter(mark, scale: scale) / 2 + scale
            return CGRect(x: centre.x - r, y: centre.y - r, width: 2 * r, height: 2 * r)
        case .text:
            // The renderer's halo is a shadow blurred by a sixth of the size; it fades out within a quarter.
            let halo = textFont(mark, scale: scale).pointSize / 4 + scale
            return textBox(mark, scale: scale).insetBy(dx: -halo, dy: -halo)
        }
    }

    // MARK: Hits

    /// Whether a click at `p` lands on `mark`. Outlines and lines count within `tolerance`; the inside of a box
    /// only when `interiors` (the select tool), so a drawing tool can still draw inside a rectangle.
    public static func hits(_ mark: Mark, _ p: CGPoint, scale: CGFloat, tolerance: CGFloat, interiors: Bool) -> Bool {
        let near = lineWidth(mark, scale: scale) / 2 + tolerance
        switch mark.kind {
        case .arrow, .line:
            guard mark.points.count > 1 else { return false }
            let extra = mark.kind == .arrow ? arrowHead(width: lineWidth(mark, scale: scale), length: .infinity).width / 2 : 0
            return distance(p, toSegment: mark.points[0], mark.points[1]) <= near + extra
        case .pen, .highlighter:
            guard let first = mark.points.first else { return false }
            if mark.points.count == 1 { return hypot(p.x - first.x, p.y - first.y) <= near }
            return zip(mark.points, mark.points.dropFirst()).contains { distance(p, toSegment: $0, $1) <= near }
        case .rectangle:
            let outer = mark.box.insetBy(dx: -near, dy: -near)
            let inner = mark.box.insetBy(dx: near, dy: near)
            return outer.contains(p) && (interiors || inner.isNull || inner.isEmpty || !inner.contains(p))
        case .ellipse:
            let box = mark.box
            guard box.width > 0, box.height > 0 else { return box.insetBy(dx: -near, dy: -near).contains(p) }
            // Distance from the outline, roughly: how far the point's normalised radius is from 1.
            let nx = (p.x - box.midX) / (box.width / 2)
            let ny = (p.y - box.midY) / (box.height / 2)
            let r = hypot(nx, ny)
            let off = abs(r - 1) * min(box.width, box.height) / 2
            return off <= near || (interiors && r <= 1)
        case .blur, .pixelate, .spotlight:
            let outer = mark.box.insetBy(dx: -tolerance, dy: -tolerance)
            let inner = mark.box.insetBy(dx: tolerance, dy: tolerance)
            return outer.contains(p) && (interiors || inner.isNull || inner.isEmpty || !inner.contains(p))
        case .step:
            guard let centre = mark.points.first else { return false }
            return hypot(p.x - centre.x, p.y - centre.y) <= stepDiameter(mark, scale: scale) / 2 + tolerance
        case .text:
            return textBox(mark, scale: scale).insetBy(dx: -tolerance, dy: -tolerance).contains(p)
        }
    }

    // MARK: Moving and resizing

    /// The points a person can drag to reshape a mark: a segment's ends, a box's corners. Others only move.
    public static func handles(_ mark: Mark) -> [CGPoint] {
        if mark.kind.isSegment, mark.points.count > 1 { return [mark.points[0], mark.points[1]] }
        if mark.kind.isBox {
            let b = mark.box
            return [CGPoint(x: b.minX, y: b.minY), CGPoint(x: b.maxX, y: b.minY), CGPoint(x: b.maxX, y: b.maxY),
                    CGPoint(x: b.minX, y: b.maxY)]
        }
        return []
    }

    public static func moved(_ mark: Mark, by delta: CGVector) -> Mark {
        var moved = mark
        moved.points = mark.points.map { CGPoint(x: $0.x + delta.dx, y: $0.y + delta.dy) }
        return moved
    }

    /// `mark` with handle `index` dragged to `point`. A box keeps the opposite corner; ⇧ keeps it square, or a
    /// segment at 45°.
    public static func resized(_ mark: Mark, handle index: Int, to point: CGPoint, square: Bool) -> Mark {
        var resized = mark
        if mark.kind.isSegment, mark.points.count > 1, index < 2 {
            let other = mark.points[1 - index]
            resized.points[index] = square ? snapped(point, from: other) : point
        } else if mark.kind.isBox {
            let corners = handles(mark)
            guard index < corners.count else { return mark }
            let opposite = corners[(index + 2) % 4]
            resized.points = [opposite, square ? squared(point, from: opposite) : point]
        }
        return resized
    }

    // MARK: The arrow

    /// The head for a shaft `width` wide, as `width` across and `height` along the arrow; shrunk for a short one.
    public static func arrowHead(width: CGFloat, length: CGFloat) -> CGSize {
        let full = max(width * 3, 9)
        let headLength = min(full, length * 0.6)
        return CGSize(width: headLength, height: headLength)
    }

    /// A filled arrow from `start` to its tip at `end`: the shaft tapers from the tail and meets a pointed head.
    public static func arrowPath(from start: CGPoint, to end: CGPoint, width: CGFloat) -> CGPath {
        let path = CGMutablePath()
        let dx = end.x - start.x
        let dy = end.y - start.y
        let length = hypot(dx, dy)
        guard length > 0.5 else { return path }
        let u = CGPoint(x: dx / length, y: dy / length)
        let n = CGPoint(x: -u.y, y: u.x)
        let head = arrowHead(width: width, length: length)
        let base = CGPoint(x: end.x - u.x * head.height, y: end.y - u.y * head.height)
        func at(_ p: CGPoint, _ side: CGFloat) -> CGPoint { CGPoint(x: p.x + n.x * side, y: p.y + n.y * side) }
        let tail = width * 0.2
        let neck = width * 0.5
        path.move(to: at(start, tail))
        path.addLine(to: at(base, neck))
        path.addLine(to: at(base, head.width / 2))
        path.addLine(to: end)
        path.addLine(to: at(base, -head.width / 2))
        path.addLine(to: at(base, -neck))
        path.addLine(to: at(start, -tail))
        path.closeSubpath()
        return path
    }
}
