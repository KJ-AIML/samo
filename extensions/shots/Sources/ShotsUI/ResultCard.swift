// What was read, on a small card near the pointer (ADR-004 S4): a QR code's text, or text too long for a note, with
// Copy, and Open for a web link (DESIGN.md rule 10: the readout's glass, over the screen). It takes clicks only on
// its buttons, and leaves on Esc, a click anywhere else, or after 8 s (held while the pointer is on it).

import AppKit
import ShotsCore

public final class ResultCard {
    public struct Content: Equatable {
        public var title: String
        public var text: String
        /// Set only for an `http` or `https` link.
        public var link: URL?

        public init(title: String, text: String, link: URL?) {
            self.title = title
            self.text = text
            self.link = link
        }

        /// A QR code's text: openable only if it is a web link (R16).
        public static func code(_ text: String) -> Content {
            Content(title: "QR CODE", text: text, link: Links.web(text))
        }
    }

    public static let lifetime: TimeInterval = 8
    /// Cards on screen, kept until they close.
    private static var showing: [ResultCard] = []

    public let content: Content
    let panel: CardPanel
    let copyButton: GlassButton
    let openButton: GlassButton?
    private let clipboard: Clipboard
    private let note: (String, Bool) -> Void
    private let onClose: () -> Void
    private var monitors: [Any] = []
    public private(set) var isClosed = false

    public init(
        _ content: Content, clipboard: Clipboard, note: @escaping (String, Bool) -> Void = { Note.show($0, problem: $1) },
        onClose: @escaping () -> Void = {}
    ) {
        self.content = content
        self.clipboard = clipboard
        self.note = note
        self.onClose = onClose
        let palette = Palette.current(NSApplication.shared.effectiveAppearance)
        let width: CGFloat = 380
        let textFont = Fonts.body(Metrics.textBody)
        let wrapping = NSMutableParagraphStyle()
        wrapping.lineBreakMode = .byCharWrapping
        let shown = NSString(string: content.text).boundingRect(
            with: CGSize(width: width - 32, height: 400), options: [.usesLineFragmentOrigin],
            attributes: [.font: textFont, .paragraphStyle: wrapping]
        )
        let textHeight = min(ceil(shown.height), 4 * ceil(textFont.ascender - textFont.descender + 4))
        let height = 16 + 16 + 6 + textHeight + 14 + 30 + 16
        panel = CardPanel(contentRect: CGRect(x: 0, y: 0, width: width, height: height),
                          styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: true)
        copyButton = GlassButton("Copy", palette: palette, strong: content.link == nil)
        openButton = content.link == nil ? nil : GlassButton("Open", palette: palette, strong: true)
        panel.card = self
        panel.level = .floating
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.isReleasedWhenClosed = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]

        let box = NSView(frame: CGRect(x: 0, y: 0, width: width, height: height))
        box.wantsLayer = true
        box.layer?.backgroundColor = palette.glass.color.withAlphaComponent(0.96).cgColor
        box.layer?.cornerRadius = Metrics.radiusCard
        box.layer?.borderColor = palette.glassLine.color.cgColor
        box.layer?.borderWidth = 1
        let title = NSTextField(labelWithAttributedString: NSAttributedString(string: content.title, attributes: [
            .font: Fonts.mono(Metrics.textLabel, weight: .medium), .foregroundColor: palette.glassText2.color,
            .kern: 0.6,
        ]))
        title.frame = CGRect(x: 16, y: height - 16 - 16, width: width - 32, height: 16)
        let text = NSTextField(wrappingLabelWithString: content.text)
        text.font = textFont
        text.textColor = palette.glassText.color
        // Codes are long and have few spaces: wrap anywhere, up to four lines, and cut only the last.
        text.maximumNumberOfLines = 4
        text.lineBreakMode = .byCharWrapping
        text.cell?.truncatesLastVisibleLine = true
        text.isSelectable = true
        text.frame = CGRect(x: 16, y: 16 + 30 + 14, width: width - 32, height: textHeight)
        var x = width - 16
        for button in [openButton, copyButton].compactMap({ $0 }) {
            x -= button.width
            button.frame = CGRect(x: x, y: 16, width: button.width, height: 30)
            x -= 8
        }
        for view in [title, text, copyButton] { box.addSubview(view) }
        if let openButton { box.addSubview(openButton) }
        panel.contentView = box
        copyButton.onPress = { [weak self] in self?.copy() }
        openButton?.onPress = { [weak self] in self?.open() }
    }

    /// Shows the card beside `point` (global, AppKit's), kept on its screen, and listens for clicks elsewhere.
    public func show(near point: CGPoint) {
        let size = panel.frame.size
        let screen = NSScreen.screens.first { $0.frame.contains(point) }?.visibleFrame ?? NSScreen.main?.visibleFrame
            ?? CGRect(x: 0, y: 0, width: 1440, height: 900)
        var origin = CGPoint(x: point.x + 16, y: point.y - size.height - 16)
        origin.x = min(max(origin.x, screen.minX + 8), screen.maxX - size.width - 8)
        origin.y = min(max(origin.y, screen.minY + 8), screen.maxY - size.height - 8)
        panel.setFrameOrigin(origin)
        ResultCard.showing.append(self)
        panel.makeKeyAndOrderFront(nil)
        if let global = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown], handler: {
            [weak self] _ in self?.close()
        }) {
            monitors.append(global)
        }
        if let local = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown], handler: {
            [weak self] event in
            if let self, event.window !== self.panel { self.close() }
            return event
        }) {
            monitors.append(local)
        }
        expire(after: ResultCard.lifetime)
    }

    private func expire(after seconds: TimeInterval) {
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { [weak self] in
            guard let self, !self.isClosed else { return }
            // Held while the pointer is on it.
            if self.panel.frame.contains(NSEvent.mouseLocation) {
                self.expire(after: 2)
            } else {
                self.close()
            }
        }
    }

    public func copy() {
        if clipboard.put(text: content.text) {
            note("Copied", false)
        } else {
            note("Couldn't copy it", true)
        }
        close()
    }

    public func open() {
        if !Links.open(content.text) {
            note("Couldn't open that link", true)
        }
        close()
    }

    public func close() {
        guard !isClosed else { return }
        isClosed = true
        monitors.forEach(NSEvent.removeMonitor)
        monitors.removeAll()
        panel.orderOut(nil)
        panel.card = nil
        ResultCard.showing.removeAll { $0 === self }
        onClose()
    }
}

/// The card's window: it can take the keyboard (for Esc) without making Shots the active app.
final class CardPanel: NSPanel {
    var card: ResultCard?

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 {
            card?.close()
        } else {
            super.keyDown(with: event)
        }
    }

    override func cancelOperation(_ sender: Any?) {
        card?.close()
    }
}

/// A button on the readout's glass: a line around plain ones, Pale cobalt for the one to press.
final class GlassButton: NSButton {
    var onPress: (() -> Void)?

    init(_ label: String, palette: Palette, strong: Bool) {
        super.init(frame: .zero)
        isBordered = false
        wantsLayer = true
        layer?.cornerRadius = Metrics.radiusControl
        layer?.borderWidth = 1
        layer?.borderColor = (strong ? palette.phosphor : palette.glassLine).color.cgColor
        title = label
        attributedTitle = NSAttributedString(string: label, attributes: [
            .font: Fonts.body(13, weight: .medium),
            .foregroundColor: (strong ? palette.phosphor : palette.glassText).color,
        ])
        target = self
        action = #selector(pressed)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    var width: CGFloat {
        (title as NSString).size(withAttributes: [.font: Fonts.body(13, weight: .medium)]).width + 26
    }

    @objc private func pressed() {
        onPress?()
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}
