// Scrolling capture (ADR-004 S5): after the area is chosen, Shots watches it while you scroll the app underneath
// yourself, and stitches each new strip below. A frame around the area and a growing preview at its side never take a
// click, and Shots' own windows are left out of what it watches. Esc (held only while this runs), a click outside the
// area, a page that jumped, or the cap ends it.

import AppKit
import CoreMedia
import ScreenCaptureKit
import ShotsCore
import ShotsUI

final class ScrollSession: NSObject, SCStreamOutput, SCStreamDelegate {
    /// The Carbon id that holds Esc while scrolling.
    private static let escapeKey: UInt32 = 90

    private let screen: NSScreen
    /// The area, in global points (AppKit's, origin bottom-left).
    let area: CGRect
    let scale: CGFloat
    private let onEnd: (CGImage?, ScrollEnd) -> Void
    private let queue = DispatchQueue(label: "dev.talvyn.samo.shots.scroll")
    private var stream: SCStream?
    /// Only touched on `queue`.
    private var stitcher: Stitcher?
    /// Set on `queue` once the capture has ended, so a late frame is dropped.
    private var stopped = false
    private var lastPreview = Date.distantPast
    private var frame: NSPanel?
    private var preview: ScrollPreview?
    private var monitors: [Any] = []
    private var ended = false

    init(screen: NSScreen, area: CGRect, scale: CGFloat, onEnd: @escaping (CGImage?, ScrollEnd) -> Void) {
        self.screen = screen
        self.area = area
        self.scale = scale
        self.onEnd = onEnd
    }

    /// Starts watching the area. `false` if macOS wouldn't give it.
    @MainActor
    func start() async -> Bool {
        guard let content = try? await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true),
              let id = Screenshotter.displayID(screen),
              let display = content.displays.first(where: { $0.displayID == id })
        else { return false }
        let me = content.applications.filter { $0.processID == ProcessInfo.processInfo.processIdentifier }
        let filter = SCContentFilter(display: display, excludingApplications: me, exceptingWindows: [])
        let config = SCStreamConfiguration()
        // The display's own points, origin top-left.
        config.sourceRect = CGRect(x: area.minX - screen.frame.minX, y: screen.frame.maxY - area.maxY,
                                   width: area.width, height: area.height)
        config.width = Int((area.width * scale).rounded())
        config.height = Int((area.height * scale).rounded())
        config.minimumFrameInterval = CMTime(value: 1, timescale: 20)
        config.pixelFormat = kCVPixelFormatType_32BGRA
        config.colorSpaceName = CGColorSpace.sRGB
        config.showsCursor = false
        config.queueDepth = 4
        let stream = SCStream(filter: filter, configuration: config, delegate: self)
        do {
            try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: queue)
            try await stream.startCapture()
        } catch {
            return false
        }
        self.stream = stream
        showFrame()
        listen()
        return true
    }

    // MARK: Frames

    func stream(_ stream: SCStream, didOutputSampleBuffer buffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard !stopped, type == .screen, ScrollSession.complete(buffer), let image = ScrollSession.image(buffer),
              let frame = ScrollFrame(image)
        else { return }
        guard let stitcher else {
            stitcher = Stitcher(first: frame)
            showPreview(force: true)
            return
        }
        guard !stitcher.finished else { return }
        switch stitcher.add(frame) {
        case .added:
            showPreview(force: false)
        case .lost:
            DispatchQueue.main.async { self.end(.lost) }
        case .full:
            showPreview(force: true)
            DispatchQueue.main.async { self.end(.full) }
        case .unchanged, .scrolledBack:
            break
        }
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        DispatchQueue.main.async { self.end(.failed) }
    }

    /// Whether a frame has new pixels (ScreenCaptureKit also sends idle and blank ones).
    private static func complete(_ buffer: CMSampleBuffer) -> Bool {
        guard let attachments = CMSampleBufferGetSampleAttachmentsArray(buffer, createIfNecessary: false)
            as? [[SCStreamFrameInfo: Any]],
              let raw = attachments.first?[.status] as? Int,
              let status = SCFrameStatus(rawValue: raw)
        else { return false }
        return status == .complete
    }

    /// A frame's pixels, copied, as an sRGB image.
    private static func image(_ buffer: CMSampleBuffer) -> CGImage? {
        guard let pixels = CMSampleBufferGetImageBuffer(buffer) else { return nil }
        CVPixelBufferLockBaseAddress(pixels, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(pixels, .readOnly) }
        guard let base = CVPixelBufferGetBaseAddress(pixels), let space = CGColorSpace(name: CGColorSpace.sRGB)
        else { return nil }
        let width = CVPixelBufferGetWidth(pixels)
        let height = CVPixelBufferGetHeight(pixels)
        let bytesPerRow = CVPixelBufferGetBytesPerRow(pixels)
        let data = Data(bytes: base, count: bytesPerRow * height)
        guard let provider = CGDataProvider(data: data as CFData) else { return nil }
        let info = CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue)
        return CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: bytesPerRow,
                       space: space, bitmapInfo: info, provider: provider, decode: nil, shouldInterpolate: false,
                       intent: .defaultIntent)
    }

    /// Hands the picture so far to the preview, at most ten times a second (on `queue`).
    private func showPreview(force: Bool) {
        guard let stitcher, force || Date().timeIntervalSince(lastPreview) > 0.1 else { return }
        lastPreview = Date()
        let parts = stitcher.parts
        let height = stitcher.height
        DispatchQueue.main.async { self.preview?.show(parts, height: height) }
    }

    // MARK: What is on screen

    @MainActor
    private func showFrame() {
        let palette = Palette.current(NSApp.effectiveAppearance)
        let border = area.insetBy(dx: -3, dy: -3)
        let panel = NSPanel(contentRect: border, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered,
                            defer: false)
        panel.level = .statusBar
        panel.ignoresMouseEvents = true
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        let view = NSView(frame: CGRect(origin: .zero, size: border.size))
        view.wantsLayer = true
        view.layer?.borderColor = palette.phosphor.color.cgColor
        view.layer?.borderWidth = 2
        view.layer?.cornerRadius = 4
        panel.contentView = view
        panel.orderFrontRegardless()
        frame = panel
        let preview = ScrollPreview(beside: area, on: screen.visibleFrame, palette: palette)
        preview.panel.orderFrontRegardless()
        self.preview = preview
    }

    @MainActor
    private func listen() {
        _ = HotKeys.shared.register(id: ScrollSession.escapeKey, Shortcut(keyCode: 53, modifiers: 0)) { [weak self] in
            self?.end(.done)
        }
        if let monitor = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown], handler: {
            [weak self] _ in
            guard let self, !self.area.contains(NSEvent.mouseLocation) else { return }
            self.end(.done)
        }) {
            monitors.append(monitor)
        }
    }

    /// Ends the capture: stops watching, takes its windows away, and hands back the picture.
    func end(_ reason: ScrollEnd) {
        finish(reason, keep: true)
    }

    /// Ends it and keeps nothing (Shots is stopping, or the start came too late).
    func cancel() {
        finish(.failed, keep: false)
    }

    private func finish(_ reason: ScrollEnd, keep: Bool) {
        guard !ended else { return }
        ended = true
        HotKeys.shared.unregister(ScrollSession.escapeKey)
        monitors.forEach(NSEvent.removeMonitor)
        monitors.removeAll()
        frame?.orderOut(nil)
        frame = nil
        preview?.panel.orderOut(nil)
        preview = nil
        if let stream {
            Task { try? await stream.stopCapture() }
        }
        stream = nil
        queue.async {
            self.stopped = true
            let image = keep ? self.stitcher?.image() : nil
            self.stitcher = nil
            if keep {
                DispatchQueue.main.async { self.onEnd(image, reason) }
            }
        }
    }
}

/// The picture growing at the side of the area, with how tall it is and how to finish. It never takes a click.
final class ScrollPreview {
    let panel: NSPanel
    private let view: PreviewView

    init(beside area: CGRect, on visible: CGRect, palette: Palette) {
        let width: CGFloat = 180
        let height = min(max(area.height, 220), visible.height - 16)
        var x = area.maxX + 14
        if x + width > visible.maxX - 8 { x = area.minX - 14 - width }
        if x < visible.minX + 8 { x = visible.maxX - width - 8 }
        let y = min(max(area.maxY - height, visible.minY + 8), visible.maxY - height - 8)
        panel = NSPanel(contentRect: CGRect(x: x, y: y, width: width, height: height),
                        styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.level = .statusBar
        panel.ignoresMouseEvents = true
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        view = PreviewView(frame: CGRect(x: 0, y: 0, width: width, height: height), palette: palette)
        panel.contentView = view
    }

    func show(_ parts: [CGImage], height: Int) {
        view.parts = parts
        view.total = height
        view.needsDisplay = true
    }
}

final class PreviewView: NSView {
    var parts: [CGImage] = []
    var total = 0
    private let palette: Palette

    init(frame: NSRect, palette: Palette) {
        self.palette = palette
        super.init(frame: frame)
        wantsLayer = true
        layer?.cornerRadius = Metrics.radiusCard
        layer?.masksToBounds = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let ctx = NSGraphicsContext.current?.cgContext else { return }
        palette.glass.color.withAlphaComponent(0.96).setFill()
        bounds.fill()
        let attrs: [NSAttributedString.Key: Any] = [
            .font: Fonts.mono(Metrics.textLabel, weight: .medium), .foregroundColor: palette.glassText2.color,
        ]
        let figure = NumberFormatter.localizedString(from: NSNumber(value: total), number: .decimal)
        ("SCROLLING · \(figure) px" as NSString).draw(at: CGPoint(x: 12, y: bounds.maxY - 22), withAttributes: attrs)
        ("Esc or a click outside ends it" as NSString).draw(
            at: CGPoint(x: 12, y: bounds.maxY - 38),
            withAttributes: [.font: Fonts.body(Metrics.textLabel), .foregroundColor: palette.glassText2.color]
        )
        // The picture, as wide as the panel, its newest rows at the bottom.
        let area = CGRect(x: 10, y: 10, width: bounds.width - 20, height: bounds.height - 58)
        guard let width = parts.first?.width, width > 0 else { return }
        let fit = area.width / CGFloat(width)
        let tall = parts.reduce(CGFloat(0)) { $0 + CGFloat($1.height) * fit }
        ctx.saveGState()
        ctx.clip(to: area)
        ctx.interpolationQuality = .medium
        // From the top of the picture down; when it's taller than the panel, its bottom sits at the panel's foot.
        var y = tall > area.height ? area.minY + tall : area.maxY
        for part in parts {
            let h = CGFloat(part.height) * fit
            y -= h
            ctx.draw(part, in: CGRect(x: area.minX, y: y, width: area.width, height: h))
        }
        ctx.restoreGState()
    }
}
