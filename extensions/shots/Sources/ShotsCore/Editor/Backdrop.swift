// The backdrop around an exported shot: padding, a two-stop ground from Talvyn's colours, rounded corners and a
// soft shadow.

import CoreGraphics

extension Backdrop {
    /// Padding for a shot this big: 8 % of its shorter side, from 24 to 96 points.
    public static func padding(for size: CGSize, scale: CGFloat) -> CGFloat {
        (min(max(min(size.width, size.height) * 0.08, 24 * scale), 96 * scale)).rounded()
    }

    public static func drawGround(_ backdrop: Backdrop, size: CGSize, in context: CGContext) {
        let ground = backdrop.ground
        guard let space = CGColorSpace(name: CGColorSpace.sRGB),
              let gradient = CGGradient(colorsSpace: space,
                                        colors: [ground.top.color.cgColor, ground.bottom.color.cgColor] as CFArray,
                                        locations: [0, 1])
        else { return }
        context.saveGState()
        context.clip(to: CGRect(origin: .zero, size: size))
        context.drawLinearGradient(gradient, start: CGPoint(x: 0, y: size.height), end: .zero, options: [])
        context.restoreGState()
    }

    public static func drawShadow(_ layout: Layout, in context: CGContext) {
        context.saveGState()
        context.setShadow(offset: CGSize(width: 0, height: -layout.shadowBlur / 4), blur: layout.shadowBlur,
                          color: CGColor(gray: 0, alpha: 0.35))
        context.addPath(CGPath(roundedRect: layout.shotRect, cornerWidth: layout.cornerRadius,
                               cornerHeight: layout.cornerRadius, transform: nil))
        context.setFillColor(CGColor(gray: 0, alpha: 1))
        context.fillPath()
        context.restoreGState()
    }
}
