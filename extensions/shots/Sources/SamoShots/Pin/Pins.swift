// Pinned shots (ADR-004 S2): a shot floating above every window and Space, where it was taken. Drag to move, pinch
// to resize, scroll for opacity, ⌘C to copy, Esc or × to close. Its image is freed when it closes (R17).

import AppKit
import ShotsCore
import ShotsUI

final class PinPanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }
}

final class PinView: NSView {
    let shot: Shot
    private let palette: Palette
    private let onClose: () -> Void
    private var dragFrom: CGPoint?
    private var hovering = false {
        didSet { needsDisplay = true }
    }

    init(shot: Shot, palette: Palette, onClose: @escaping () -> Void) {
        self.shot = shot
        self.palette = palette
        self.onClose = onClose
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = Metrics.radiusChip
        layer?.masksToBounds = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var acceptsFirstResponder: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func updateTrackingAreas() {
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect],
                                       owner: self))
        super.updateTrackingAreas()
    }

    private var closeBox: CGRect {
        CGRect(x: bounds.maxX - 26, y: bounds.maxY - 26, width: 20, height: 20)
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        ctx.interpolationQuality = .high
        ctx.draw(shot.image, in: bounds)
        palette.hairlineStrong.color.setStroke()
        NSBezierPath(roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), xRadius: Metrics.radiusChip,
                     yRadius: Metrics.radiusChip).stroke()
        guard hovering else { return }
        palette.glass.color.withAlphaComponent(0.9).setFill()
        NSBezierPath(ovalIn: closeBox).fill()
        let attrs: [NSAttributedString.Key: Any] = [.font: Fonts.body(13, weight: .semibold),
                                                    .foregroundColor: palette.glassText.color]
        ("×" as NSString).draw(at: CGPoint(x: closeBox.minX + 6, y: closeBox.minY + 2), withAttributes: attrs)
    }

    override func mouseEntered(with event: NSEvent) { hovering = true }
    override func mouseExited(with event: NSEvent) { hovering = false }

    override func mouseDown(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        if hovering, closeBox.contains(p) {
            onClose()
            return
        }
        if event.clickCount == 2 {
            Note.show("Editing comes in a later version of Shots", problem: false)
            return
        }
        dragFrom = NSEvent.mouseLocation
        window?.makeKey()
    }

    override func mouseDragged(with event: NSEvent) {
        guard let window, let from = dragFrom else { return }
        let now = NSEvent.mouseLocation
        window.setFrameOrigin(CGPoint(x: window.frame.minX + now.x - from.x, y: window.frame.minY + now.y - from.y))
        dragFrom = now
    }

    override func mouseUp(with event: NSEvent) {
        dragFrom = nil
    }

    override func scrollWheel(with event: NSEvent) {
        guard let window else { return }
        window.alphaValue = CGFloat(PinMath.opacity(Double(window.alphaValue), scrolledBy: Double(event.scrollingDeltaY)))
    }

    override func magnify(with event: NSEvent) {
        guard let window else { return }
        let factor = 1 + event.magnification
        let size = CGSize(width: max(60, window.frame.width * factor), height: max(60, window.frame.height * factor))
        let center = CGPoint(x: window.frame.midX, y: window.frame.midY)
        window.setFrame(CGRect(x: center.x - size.width / 2, y: center.y - size.height / 2, width: size.width,
                               height: size.height), display: true)
    }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 {
            onClose()
        } else {
            super.keyDown(with: event)
        }
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if event.modifierFlags.contains(.command), event.charactersIgnoringModifiers == "c" {
            if let png = Encoding.data(shot.image, format: .png, scale: shot.scale), SystemClipboard().put(png: png) {
                Note.show("Copied", problem: false)
            } else {
                Note.show("Couldn't copy the shot", problem: true)
            }
            return true
        }
        return super.performKeyEquivalent(with: event)
    }
}

final class Pins {
    private let store: ShotStore
    private var panels: [PinPanel] = []

    init(store: ShotStore) {
        self.store = store
    }

    func open(_ shot: Shot) {
        let pointer = NSEvent.mouseLocation
        let at = shot.origin ?? pointer
        let screen = NSScreen.screens.first { $0.frame.contains(at) } ?? NSScreen.main
        let screenSize = screen?.visibleFrame.size ?? CGSize(width: 1440, height: 900)
        let size = PinMath.size(pixels: CGSize(width: shot.image.width, height: shot.image.height), scale: shot.scale,
                                screen: screenSize)
        let origin = shot.origin ?? CGPoint(x: pointer.x - size.width / 2, y: pointer.y - size.height / 2)
        let panel = PinPanel(contentRect: CGRect(origin: origin, size: size),
                             styleMask: [.borderless, .nonactivatingPanel, .resizable], backing: .buffered, defer: false)
        panel.level = .floating
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.hasShadow = true
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.isReleasedWhenClosed = false
        panel.contentAspectRatio = size
        let view = PinView(shot: shot, palette: Palette.current(NSApp.effectiveAppearance)) { [weak self, weak panel] in
            if let panel { self?.close(panel) }
        }
        panel.contentView = view
        store.retain(shot)
        panels.append(panel)
        panel.makeKeyAndOrderFront(nil)
    }

    private func close(_ panel: PinPanel) {
        panel.orderOut(nil)
        if let view = panel.contentView as? PinView {
            store.release(view.shot)
        }
        panel.contentView = nil
        panels.removeAll { $0 === panel }
    }

    func closeAll() {
        for panel in panels {
            close(panel)
        }
    }

    var count: Int { panels.count }
}
