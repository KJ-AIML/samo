// The list at ⇧⌘C (ADR-005 C2): the history at the pointer, on the readout's glass (DESIGN.md rule 10), with the
// search field taking the keyboard. Type to search; ↑↓ move; Return or ⌘1–⌘9 paste; ⌥Return pastes without
// formatting; ⌥P pins; ⌥⌫ deletes; Esc, or a click anywhere else, closes it. Pins come first.

import AppKit
import ClipsCore
import SamoKit

/// What the list needs from the rest of Clips.
public struct ListHooks {
    /// The history, pins first.
    public var items: () -> [Item]
    /// A small picture of an image item.
    public var thumbnail: (Item) -> CGImage?
    /// An item picked: paste it (`true`: without formatting).
    public var pick: (Item, Bool) -> Void
    public var pin: (Item) -> Void
    public var delete: (Item) -> Void
    /// The list closed; `true` if by a pick.
    public var closed: (Bool) -> Void

    public init(items: @escaping () -> [Item], thumbnail: @escaping (Item) -> CGImage?,
                pick: @escaping (Item, Bool) -> Void, pin: @escaping (Item) -> Void,
                delete: @escaping (Item) -> Void, closed: @escaping (Bool) -> Void) {
        self.items = items
        self.thumbnail = thumbnail
        self.pick = pick
        self.pin = pin
        self.delete = delete
        self.closed = closed
    }
}

final class ListPanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }
}

public final class ListWindow: NSObject, NSTextFieldDelegate, NSWindowDelegate {
    public static let width: CGFloat = 440
    static let rowHeight: CGFloat = 34
    static let header: CGFloat = 50
    static let maxRows = 12

    let panel: ListPanel
    public let field: NSTextField
    let rows: RowsView
    let hooks: ListHooks
    let palette: Palette
    public private(set) var shown: [Item] = []
    public private(set) var selected = 0
    public private(set) var isOpen = false
    private var monitor: Any?

    public init(hooks: ListHooks) {
        self.hooks = hooks
        palette = Palette.current(NSApplication.shared.effectiveAppearance)
        panel = ListPanel(contentRect: CGRect(x: 0, y: 0, width: ListWindow.width, height: 200),
                          styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        field = NSTextField(frame: .zero)
        rows = RowsView(frame: .zero)
        super.init()
        panel.level = .popUpMenu
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.isReleasedWhenClosed = false
        panel.hidesOnDeactivate = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient]
        panel.delegate = self
        let box = NSView(frame: panel.contentRect(forFrameRect: panel.frame))
        box.wantsLayer = true
        box.layer?.backgroundColor = palette.glass.color.withAlphaComponent(0.97).cgColor
        box.layer?.cornerRadius = Metrics.radiusCard
        box.layer?.borderColor = palette.glassLine.color.cgColor
        box.layer?.borderWidth = 1
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = Fonts.body(16)
        field.textColor = palette.glassText.color
        field.placeholderAttributedString = NSAttributedString(string: "Type to search", attributes: [
            .font: Fonts.body(16), .foregroundColor: palette.glassText2.color,
        ])
        field.delegate = self
        rows.list = self
        box.addSubview(field)
        box.addSubview(rows)
        panel.contentView = box
    }

    public var query: String { field.stringValue }

    /// Opens the list, fresh, without showing it yet (tests drive it like this).
    public func prepare() {
        field.stringValue = ""
        selected = 0
        isOpen = true
        reload()
    }

    /// Opens the list beside `point` (global, AppKit's), on its screen, with the keyboard in the search field.
    public func show(near point: CGPoint) {
        prepare()
        let size = panel.frame.size
        let screen = NSScreen.screens.first { $0.frame.contains(point) }?.visibleFrame ?? NSScreen.main?.visibleFrame
            ?? CGRect(x: 0, y: 0, width: 1440, height: 900)
        var origin = CGPoint(x: point.x - 20, y: point.y - size.height + 20)
        origin.x = min(max(origin.x, screen.minX + 8), screen.maxX - size.width - 8)
        origin.y = min(max(origin.y, screen.minY + 8), screen.maxY - size.height - 8)
        panel.setFrameOrigin(origin)
        Activation.bringForward()
        panel.makeKeyAndOrderFront(nil)
        panel.makeFirstResponder(field)
        if monitor == nil {
            monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
                guard let self, self.isOpen, event.window === self.panel else { return event }
                return self.handle(event) ? nil : event
            }
        }
    }

    /// Filters again and resizes to fit.
    public func reload() {
        shown = Search.filter(hooks.items(), query)
        selected = min(selected, max(shown.count - 1, 0))
        let count = max(1, min(shown.count, ListWindow.maxRows))
        let height = ListWindow.header + CGFloat(count) * ListWindow.rowHeight + 10
        let top = panel.frame.maxY
        panel.setFrame(CGRect(x: panel.frame.minX, y: top - height, width: ListWindow.width, height: height),
                       display: false)
        let content = panel.contentView?.bounds ?? .zero
        field.frame = CGRect(x: 16, y: content.maxY - ListWindow.header + 14, width: content.width - 32, height: 24)
        rows.frame = CGRect(x: 6, y: 6, width: content.width - 12, height: content.height - ListWindow.header - 4)
        rows.needsDisplay = true
    }

    /// Typing in the search field.
    public func search(_ text: String) {
        field.stringValue = text
        selected = 0
        reload()
    }

    public func controlTextDidChange(_ obj: Notification) {
        selected = 0
        reload()
    }

    /// The list's own keys; `false` for a key the search field should type.
    public func handle(_ event: NSEvent) -> Bool {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        switch event.keyCode {
        case 125:  // ↓
            move(1)
        case 126:  // ↑
            move(-1)
        case 36, 76:  // Return: paste; ⌥Return without formatting.
            pick(at: selected, plain: flags.contains(.option))
        case 53:  // Esc
            close(picked: false)
        case 35 where flags == .option:  // ⌥P
            if let item = shown[safe: selected] {
                hooks.pin(item)
                reload()
            }
        case 51 where flags == .option, 117 where flags == .option:  // ⌥⌫
            if let item = shown[safe: selected] {
                hooks.delete(item)
                reload()
            }
        default:
            // ⌘1–⌘9: the row with that number.
            guard flags == .command, let digit = event.charactersIgnoringModifiers.flatMap(Int.init), digit >= 1,
                  digit <= 9
            else { return false }
            pick(at: digit - 1, plain: false)
        }
        return true
    }

    private func move(_ by: Int) {
        guard !shown.isEmpty else { return }
        selected = min(max(selected + by, 0), shown.count - 1)
        rows.needsDisplay = true
    }

    func pick(at index: Int, plain: Bool) {
        guard let item = shown[safe: index] else { return }
        close(picked: true)
        hooks.pick(item, plain)
    }

    func hover(_ index: Int) {
        guard index != selected, shown.indices.contains(index) else { return }
        selected = index
        rows.needsDisplay = true
    }

    public func close(picked: Bool) {
        guard isOpen else { return }
        isOpen = false
        panel.orderOut(nil)
        hooks.closed(picked)
    }

    public func windowDidResignKey(_ notification: Notification) {
        close(picked: false)
    }

    deinit {
        if let monitor { NSEvent.removeMonitor(monitor) }
    }
}

/// The rows: a kind's mark (or a thumbnail), the first line, a pin, and ⌘1–⌘9.
final class RowsView: NSView {
    weak var list: ListWindow?

    override var isFlipped: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func updateTrackingAreas() {
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseMoved, .activeAlways, .inVisibleRect],
                                       owner: self))
        super.updateTrackingAreas()
    }

    private func row(at point: CGPoint) -> Int {
        Int(point.y / ListWindow.rowHeight)
    }

    override func mouseMoved(with event: NSEvent) {
        list?.hover(row(at: convert(event.locationInWindow, from: nil)))
    }

    override func mouseDown(with event: NSEvent) {
        let index = row(at: convert(event.locationInWindow, from: nil))
        list?.pick(at: index, plain: event.modifierFlags.contains(.option))
    }

    static func symbol(_ item: Item) -> String {
        switch item.kind {
        case .text: return "text.alignleft"
        case .rich: return "textformat"
        case .image: return "photo"
        case .files: return "doc"
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let list, let ctx = NSGraphicsContext.current?.cgContext else { return }
        let palette = list.palette
        let h = ListWindow.rowHeight
        if list.shown.isEmpty {
            let words = list.query.isEmpty ? "Nothing copied yet" : "Nothing matches “\(list.query)”"
            (words as NSString).draw(at: CGPoint(x: 12, y: 9), withAttributes: [
                .font: Fonts.body(Metrics.textBody), .foregroundColor: palette.glassText2.color,
            ])
            return
        }
        for (i, item) in list.shown.prefix(ListWindow.maxRows).enumerated() {
            let rect = CGRect(x: 0, y: CGFloat(i) * h, width: bounds.width, height: h)
            if i == list.selected {
                palette.phosphor.color.withAlphaComponent(0.16).setFill()
                NSBezierPath(roundedRect: rect.insetBy(dx: 2, dy: 1), xRadius: Metrics.radiusControl,
                             yRadius: Metrics.radiusControl).fill()
            }
            let mark = CGRect(x: rect.minX + 10, y: rect.midY - 11, width: 22, height: 22)
            if item.kind == .image, let thumb = list.hooks.thumbnail(item) {
                ctx.saveGState()
                ctx.addPath(CGPath(roundedRect: mark, cornerWidth: 4, cornerHeight: 4, transform: nil))
                ctx.clip()
                // Flipped view: draw the picture upright.
                ctx.translateBy(x: 0, y: mark.maxY + mark.minY)
                ctx.scaleBy(x: 1, y: -1)
                ctx.draw(thumb, in: mark)
                ctx.restoreGState()
            } else if let image = NSImage(systemSymbolName: RowsView.symbol(item), accessibilityDescription: nil)?
                .withSymbolConfiguration(.init(pointSize: 13, weight: .regular)) {
                let tinted = image.tinted(palette.glassText2.color)
                tinted.draw(in: CGRect(x: mark.midX - tinted.size.width / 2, y: mark.midY - tinted.size.height / 2,
                                       width: tinted.size.width, height: tinted.size.height))
            }
            var right = rect.maxX - 12
            if i < 9 {
                let key = "⌘\(i + 1)" as NSString
                let attrs: [NSAttributedString.Key: Any] = [
                    .font: Fonts.mono(Metrics.textLabel, weight: .medium), .foregroundColor: palette.glassText2.color,
                ]
                let size = key.size(withAttributes: attrs)
                right -= size.width
                key.draw(at: CGPoint(x: right, y: rect.midY - size.height / 2), withAttributes: attrs)
                right -= 10
            }
            if item.pinned, let pin = NSImage(systemSymbolName: "pin.fill", accessibilityDescription: "Pinned")?
                .withSymbolConfiguration(.init(pointSize: 10, weight: .semibold)) {
                let tinted = pin.tinted(palette.phosphor.color)
                right -= tinted.size.width
                tinted.draw(in: CGRect(x: right, y: rect.midY - tinted.size.height / 2, width: tinted.size.width,
                                       height: tinted.size.height))
                right -= 10
            }
            let title = NSAttributedString(string: item.title.isEmpty ? " " : item.title, attributes: [
                .font: Fonts.body(Metrics.textBody), .foregroundColor: palette.glassText.color,
            ])
            let titleRect = CGRect(x: mark.maxX + 10, y: rect.midY - 9, width: max(0, right - mark.maxX - 16),
                                   height: 18)
            title.draw(with: titleRect, options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        }
    }
}

extension NSImage {
    /// The symbol in one colour.
    func tinted(_ color: NSColor) -> NSImage {
        let image = NSImage(size: size, flipped: false) { rect in
            self.draw(in: rect)
            color.set()
            rect.fill(using: .sourceAtop)
            return true
        }
        image.isTemplate = false
        return image
    }
}

extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
