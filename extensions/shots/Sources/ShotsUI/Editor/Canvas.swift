// The editor's canvas: the backdrop, the shot as a layer's contents (drawn by the GPU, so only marks redraw), and
// the marks on top, where the pointer draws. Its size is the export's at real size; the scroll view zooms it.
// While cropping, it shows the whole shot with the box to keep.

import AppKit
import SamoKit
import ShotsCore

/// Centres the canvas when it is smaller than the window.
final class CenteringClipView: NSClipView {
    override func constrainBoundsRect(_ proposedBounds: NSRect) -> NSRect {
        var rect = super.constrainBoundsRect(proposedBounds)
        guard let document = documentView else { return rect }
        if rect.width > document.frame.width { rect.origin.x = (document.frame.width - rect.width) / 2 }
        if rect.height > document.frame.height { rect.origin.y = (document.frame.height - rect.height) / 2 }
        return rect
    }
}

final class CanvasView: NSView {
    weak var editor: EditorWindow?
    let ground = GroundView()
    let shotView = ShotView()
    let marks = MarksView()
    /// Where things are, in export pixels.
    private(set) var layoutInfo = Layout.empty
    /// The box to keep while cropping, in the shot's pixels; `nil` when not cropping.
    var cropBox: CGRect?

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        // Since macOS 14 a view may draw outside its bounds; these must not.
        ground.clipsToBounds = true
        marks.clipsToBounds = true
        addSubview(ground)
        addSubview(shotView)
        addSubview(marks)
        marks.canvas = self
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    var scale: CGFloat { editor?.document.scale ?? 1 }

    /// After a crop or backdrop change: the canvas's size and its layers.
    func relayout() {
        guard let editor else { return }
        let doc = editor.document
        layoutInfo = cropBox == nil
            ? Renderer.layout(doc)
            : Renderer.layout(visible: doc.fullRect, backdrop: nil, scale: doc.scale)
        let size = CGSize(width: layoutInfo.size.width / scale, height: layoutInfo.size.height / scale)
        setFrameSize(size)
        let bounds = CGRect(origin: .zero, size: size)
        ground.frame = bounds
        marks.frame = bounds
        let shotRect = toView(layoutInfo.shotRect)
        shotView.frame = shotRect
        ground.backdrop = cropBox == nil ? doc.canvas.backdrop : nil
        ground.layoutInfo = layoutInfo
        ground.scale = scale
        ground.needsDisplay = true
        let visible = cropBox == nil ? doc.visibleRect : doc.fullRect
        // The image's rows run from the top.
        let rows = CGRect(x: visible.minX, y: CGFloat(editor.shot.image.height) - visible.maxY, width: visible.width,
                          height: visible.height)
        shotView.image = editor.shot.image.cropping(to: rows)
        shotView.radius = layoutInfo.cornerRadius / scale
        // A soft shadow lifts a plain shot off the window's grey; a backdrop draws its own.
        layer?.shadowColor = NSColor.black.cgColor
        layer?.shadowOpacity = ground.backdrop == nil && cropBox == nil ? 0.22 : 0
        layer?.shadowRadius = 10
        layer?.shadowOffset = CGSize(width: 0, height: -2)
        layer?.shadowPath = CGPath(rect: bounds, transform: nil)
        marks.needsDisplay = true
    }

    /// Export pixels to canvas points.
    func toView(_ rect: CGRect) -> CGRect {
        CGRect(x: rect.minX / scale, y: rect.minY / scale, width: rect.width / scale, height: rect.height / scale)
    }

    func toView(_ p: CGPoint) -> CGPoint {
        CGPoint(x: p.x / scale, y: p.y / scale)
    }

    /// A point in the shot's pixels, in canvas points.
    func viewPoint(forShot p: CGPoint) -> CGPoint {
        toView(layoutInfo.toExport(p))
    }

    /// A canvas point, in the shot's pixels.
    func shotPoint(forView p: CGPoint) -> CGPoint {
        layoutInfo.toShot(CGPoint(x: p.x * scale, y: p.y * scale))
    }

    /// A shot rectangle, in canvas points.
    func viewRect(forShot r: CGRect) -> CGRect {
        let a = viewPoint(forShot: CGPoint(x: r.minX, y: r.minY))
        let b = viewPoint(forShot: CGPoint(x: r.maxX, y: r.maxY))
        return CGRect(x: a.x, y: a.y, width: b.x - a.x, height: b.y - a.y)
    }

    /// Where a point in the shot is in the window (for tests that click).
    public func windowPoint(forShot p: CGPoint) -> CGPoint {
        marks.convert(viewPoint(forShot: p), to: nil)
    }

    // MARK: Crop

    func enterCrop() {
        guard let editor else { return }
        cropBox = editor.document.visibleRect
        relayout()
        if editor.fitting { editor.fit() }
    }

    /// Leaves cropping; `apply` keeps the box.
    func leaveCrop(apply: Bool) {
        guard let editor, let box = cropBox else { return }
        cropBox = nil
        if apply, box != editor.document.visibleRect {
            editor.document.setCrop(box)
        }
        relayout()
        if editor.fitting { editor.fit() }
    }
}

/// The backdrop's ground and the shot's shadow; drawn only when the layout changes.
final class GroundView: NSView {
    var backdrop: Backdrop?
    var layoutInfo: Layout?
    var scale: CGFloat = 1

    override func draw(_ dirtyRect: NSRect) {
        guard let backdrop, let layoutInfo, let ctx = NSGraphicsContext.current?.cgContext else { return }
        ctx.saveGState()
        ctx.scaleBy(x: 1 / scale, y: 1 / scale)
        Backdrop.drawGround(backdrop, size: layoutInfo.size, in: ctx)
        Backdrop.drawShadow(layoutInfo, in: ctx)
        ctx.restoreGState()
    }
}

/// The shot itself, as a layer's contents: scaled by the GPU, pixels kept sharp when zoomed in.
final class ShotView: NSView {
    var image: CGImage? {
        didSet { needsDisplay = true }
    }
    var radius: CGFloat = 0 {
        didSet { needsDisplay = true }
    }

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        layerContentsRedrawPolicy = .onSetNeedsDisplay
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        guard let layer else { return }
        layer.contents = image
        layer.contentsGravity = .resize
        layer.magnificationFilter = .nearest
        layer.minificationFilter = .linear
        layer.cornerRadius = radius
        layer.masksToBounds = radius > 0
    }
}
