// Selection maths for the capture overlay, in the overlay's own coordinates: points, origin bottom-left (AppKit's).
// A captured display image is in pixels, origin top-left.

import CoreGraphics

public enum Arrow {
    case up, down, left, right
}

public enum Selection {
    /// Below this many points across, a drag is a click: nothing is captured.
    public static let minimum: CGFloat = 4

    /// The rectangle from where the drag began to where it is now. With `square`, the shorter side wins, growing from
    /// the start in the drag's direction.
    public static func rect(from a: CGPoint, to b: CGPoint, square: Bool) -> CGRect {
        var dx = b.x - a.x
        var dy = b.y - a.y
        if square {
            let side = min(abs(dx), abs(dy))
            dx = dx < 0 ? -side : side
            dy = dy < 0 ? -side : side
        }
        return CGRect(x: min(a.x, a.x + dx), y: min(a.y, a.y + dy), width: abs(dx), height: abs(dy))
    }

    /// `r` kept within `bounds`.
    public static func clamp(_ r: CGRect, to bounds: CGRect) -> CGRect {
        r.intersection(bounds).standardized
    }

    /// `r` (overlay points) as a rectangle of the display's image (pixels, origin top-left), whole pixels.
    public static func imageRect(_ r: CGRect, viewHeight: CGFloat, scale: CGFloat) -> CGRect {
        CGRect(
            x: (r.minX * scale).rounded(),
            y: ((viewHeight - r.maxY) * scale).rounded(),
            width: (r.width * scale).rounded(),
            height: (r.height * scale).rounded()
        )
    }

    /// The pointer moved by an arrow key: one point, or ten with ⇧.
    public static func nudge(_ p: CGPoint, _ arrow: Arrow, big: Bool) -> CGPoint {
        let step: CGFloat = big ? 10 : 1
        switch arrow {
        case .up: return CGPoint(x: p.x, y: p.y + step)
        case .down: return CGPoint(x: p.x, y: p.y - step)
        case .left: return CGPoint(x: p.x - step, y: p.y)
        case .right: return CGPoint(x: p.x + step, y: p.y)
        }
    }

    /// `r` moved by `delta`, kept whole inside `bounds` (Space held while dragging moves the selection).
    public static func moved(_ r: CGRect, by delta: CGVector, within bounds: CGRect) -> CGRect {
        var x = r.minX + delta.dx
        var y = r.minY + delta.dy
        x = max(bounds.minX, min(x, bounds.maxX - r.width))
        y = max(bounds.minY, min(y, bounds.maxY - r.height))
        return CGRect(x: x, y: y, width: r.width, height: r.height)
    }
}
