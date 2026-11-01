// One capture's overlays: a panel on every display, above everything (the shielding level), on every Space. The
// session switches them between area and window picking together, and hands back what was chosen.

import AppKit
import ScreenCaptureKit
import ShotsCore
import ShotsUI

final class OverlaySession {
    private var panels: [OverlayPanel] = []
    private var views: [OverlayView] = []
    private var mode: OverlayView.Mode
    private let onArea: (Frozen, CGRect, OverlayView) -> Void
    private let onWindow: (SCWindow, CGFloat) -> Void
    private let onCancel: () -> Void

    init(
        startIn mode: OverlayView.Mode,
        onArea: @escaping (Frozen, CGRect, OverlayView) -> Void,
        onWindow: @escaping (SCWindow, CGFloat) -> Void,
        onCancel: @escaping () -> Void
    ) {
        self.mode = mode
        self.onArea = onArea
        self.onWindow = onWindow
        self.onCancel = onCancel
    }

    func show(_ frozen: [Frozen]) {
        let pointer = NSEvent.mouseLocation
        for f in frozen {
            let panel = OverlayPanel(
                contentRect: f.screen.frame, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered,
                defer: false
            )
            panel.level = NSWindow.Level(rawValue: Int(CGShieldingWindowLevel()))
            panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
            panel.isOpaque = true
            panel.hasShadow = false
            panel.acceptsMouseMovedEvents = true
            panel.isReleasedWhenClosed = false
            let view = OverlayView(frozen: f, palette: Palette.current(NSApp.effectiveAppearance), session: self)
            view.mode = mode
            panel.contentView = view
            panel.setFrame(f.screen.frame, display: false)
            panels.append(panel)
            views.append(view)
        }
        NSApp.activate(ignoringOtherApps: true)
        for (panel, view) in zip(panels, views) {
            if panel.frame.contains(pointer) {
                panel.makeKeyAndOrderFront(nil)
                panel.makeFirstResponder(view)
            } else {
                panel.orderFrontRegardless()
            }
        }
        if mode == .window {
            loadWindows()
        }
    }

    func toggleMode() {
        mode = mode == .area ? .window : .area
        views.forEach { $0.mode = mode }
        if mode == .window {
            loadWindows()
        }
    }

    /// The windows on each display, in its local points (ScreenCaptureKit gives global points, origin top-left).
    private func loadWindows() {
        Task { @MainActor [weak self] in
            let windows = await Screenshotter.windows()
            guard let self else { return }
            let primaryHeight = NSScreen.screens.first?.frame.height ?? 0
            for view in self.views {
                let screen = view.frozen.screen.frame
                view.pickable = windows.compactMap { w in
                    let cocoa = CGRect(x: w.frame.minX, y: primaryHeight - w.frame.maxY, width: w.frame.width,
                                       height: w.frame.height)
                    let local = cocoa.offsetBy(dx: -screen.minX, dy: -screen.minY)
                    return local.intersects(CGRect(origin: .zero, size: screen.size))
                        ? Pickable(rect: local, window: w) : nil
                }
            }
        }
    }

    func finish(_ rect: CGRect, on view: OverlayView) {
        onArea(view.frozen, rect, view)
    }

    func pick(_ pickable: Pickable, on view: OverlayView) {
        onWindow(pickable.window, view.frozen.scale)
    }

    func cancel() {
        onCancel()
    }

    func note(_ words: String) {
        Note.show(words, problem: false)
    }

    func close() {
        panels.forEach { $0.orderOut(nil) }
        panels.removeAll()
        views.removeAll()
    }
}
