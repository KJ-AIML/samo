// The editor's bars, in Samo's tokens (DESIGN.md rule 10): the tools, colours, widths and backdrop on top, Cobalt
// for what is chosen; at the foot, the size and zoom in mono, the colour under the pointer, a chip for a QR code,
// then Copy text, the drag handle, Pin, Save and Copy, the window's one Ink action.

import AppKit
import SamoKit
import ShotsCore

final class ToolbarView: NSView {
    static let height: CGFloat = 46
    /// Every tool, colour, width, the backdrop, undo and redo, without overlapping.
    static let minimumWidth: CGFloat = 900

    weak var editor: EditorWindow?
    private var tools: [Tool: ToolButton] = [:]
    private var swatches: [SwatchButton] = []
    private let more = SwatchButton(colour: nil)
    private var widths: [Width: ToolButton] = [:]
    private let backdrop = ToolButton(symbol: "photo.artframe", fallback: "B")
    private let undo = ToolButton(symbol: "arrow.uturn.backward", fallback: "↶")
    private let redo = ToolButton(symbol: "arrow.uturn.forward", fallback: "↷")
    private var separators: [NSView] = []

    static let groups: [[Tool]] = [
        [.select], [.arrow, .line, .rectangle, .ellipse], [.text, .highlighter, .pen, .step],
        [.blur, .pixelate, .spotlight], [.crop, .measure],
    ]

    static func symbol(_ tool: Tool) -> String {
        switch tool {
        case .select: return "cursorarrow"
        case .arrow: return "arrow.up.right"
        case .line: return "line.diagonal"
        case .rectangle: return "rectangle"
        case .ellipse: return "circle"
        case .text: return "textformat"
        case .highlighter: return "highlighter"
        case .pen: return "scribble"
        case .step: return "1.circle"
        case .blur: return "drop.halffull"
        case .pixelate: return "squareshape.split.3x3"
        case .spotlight: return "flashlight.on.fill"
        case .crop: return "crop"
        case .measure: return "ruler"
        }
    }

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        for tool in Tool.allCases {
            let button = ToolButton(symbol: ToolbarView.symbol(tool), fallback: String(tool.key).uppercased())
            button.toolTip = "\(tool.label)  \(String(tool.key).uppercased())"
            button.setAccessibilityLabel(tool.label)
            button.onPress = { [weak self] in self?.editor?.select(tool) }
            tools[tool] = button
            addSubview(button)
        }
        for (name, colour) in Swatch.all {
            let swatch = SwatchButton(colour: colour)
            swatch.toolTip = name
            swatch.setAccessibilityLabel(name)
            swatch.onPress = { [weak self] in
                guard let editor = self?.editor else { return }
                editor.setStyle(Style(colour: colour, width: editor.style.width))
            }
            swatches.append(swatch)
            addSubview(swatch)
        }
        more.toolTip = "More colours…"
        more.setAccessibilityLabel("More colours")
        more.onPress = { [weak self] in self?.showColourPanel() }
        addSubview(more)
        for width in Width.allCases {
            let button = ToolButton(symbol: nil, fallback: width.label)
            button.toolTip = "Width \(width.label)  [ ]"
            button.onPress = { [weak self] in
                guard let editor = self?.editor else { return }
                editor.setStyle(Style(colour: editor.style.colour, width: width))
            }
            widths[width] = button
            addSubview(button)
        }
        backdrop.toolTip = "Backdrop"
        backdrop.onPress = { [weak self] in self?.showBackdropMenu() }
        undo.toolTip = "Undo  ⌘Z"
        undo.onPress = { [weak self] in self?.editor?.undo() }
        redo.toolTip = "Redo  ⇧⌘Z"
        redo.onPress = { [weak self] in self?.editor?.redo() }
        for button in [backdrop, undo, redo] { addSubview(button) }
        for _ in 0..<(ToolbarView.groups.count + 2) {
            let line = NSView()
            line.wantsLayer = true
            separators.append(line)
            addSubview(line)
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override func layout() {
        super.layout()
        let side: CGFloat = 28
        let y = (bounds.height - side) / 2
        var x: CGFloat = 12
        var separator = 0
        func rule() {
            guard separator < separators.count else { return }
            separators[separator].frame = CGRect(x: x + 3, y: y + 5, width: 1, height: side - 10)
            separator += 1
            x += 10
        }
        for (i, group) in ToolbarView.groups.enumerated() {
            if i > 0 { rule() }
            for tool in group {
                tools[tool]?.frame = CGRect(x: x, y: y, width: side, height: side)
                x += side + 2
            }
        }
        rule()
        for swatch in swatches + [more] {
            swatch.frame = CGRect(x: x + 2, y: y + 4, width: 20, height: 20)
            x += 24
        }
        rule()
        for width in Width.allCases {
            widths[width]?.frame = CGRect(x: x, y: y, width: 26, height: side)
            x += 28
        }
        x += 6
        backdrop.frame = CGRect(x: x, y: y, width: side, height: side)
        redo.frame = CGRect(x: bounds.maxX - 12 - side, y: y, width: side, height: side)
        undo.frame = CGRect(x: redo.frame.minX - 2 - side, y: y, width: side, height: side)
    }

    func refresh() {
        guard let editor else { return }
        let palette = editor.palette
        layer?.backgroundColor = palette.card.color.cgColor
        layer?.borderColor = palette.hairline.color.cgColor
        for (tool, button) in tools {
            button.paint(selected: tool == editor.tool, palette: palette)
        }
        for swatch in swatches {
            swatch.paint(selected: swatch.colour == editor.style.colour, palette: palette)
        }
        more.paint(selected: !Swatch.all.contains { $0.colour == editor.style.colour }, palette: palette)
        for (width, button) in widths {
            button.paint(selected: width == editor.style.width, palette: palette)
        }
        backdrop.paint(selected: editor.document.canvas.backdrop != nil, palette: palette)
        undo.paint(selected: false, palette: palette)
        redo.paint(selected: false, palette: palette)
        undo.isEnabled = editor.document.canUndo
        redo.isEnabled = editor.document.canRedo
        for line in separators {
            line.layer?.backgroundColor = palette.hairlineStrong.color.cgColor
        }
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let editor else { return }
        editor.palette.hairline.color.setFill()
        CGRect(x: 0, y: 0, width: bounds.width, height: 1).fill()
    }

    private func showBackdropMenu() {
        guard let editor else { return }
        let menu = NSMenu()
        let none = NSMenuItem(title: "No backdrop", action: #selector(pickBackdrop(_:)), keyEquivalent: "")
        none.target = self
        none.state = editor.document.canvas.backdrop == nil ? .on : .off
        menu.addItem(none)
        menu.addItem(.separator())
        for choice in Backdrop.allCases {
            let item = NSMenuItem(title: choice.label, action: #selector(pickBackdrop(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = choice.rawValue
            item.state = editor.document.canvas.backdrop == choice ? .on : .off
            menu.addItem(item)
        }
        menu.popUp(positioning: nil, at: CGPoint(x: backdrop.frame.minX, y: backdrop.frame.minY - 4), in: self)
    }

    @objc private func pickBackdrop(_ item: NSMenuItem) {
        editor?.setBackdrop((item.representedObject as? String).flatMap(Backdrop.init(rawValue:)))
    }

    private func showColourPanel() {
        let panel = NSColorPanel.shared
        panel.setTarget(self)
        panel.setAction(#selector(colourChosen(_:)))
        panel.showsAlpha = false
        if let colour = editor?.style.colour { panel.color = colour.color }
        panel.orderFront(nil)
    }

    @objc private func colourChosen(_ panel: NSColorPanel) {
        guard let editor, let colour = RGBA(panel.color) else { return }
        editor.setStyle(Style(colour: RGBA(r: colour.r, g: colour.g, b: colour.b, a: 1), width: editor.style.width))
    }
}

/// A square button with a symbol (or a letter), Cobalt on its wash when chosen.
final class ToolButton: NSButton {
    var onPress: (() -> Void)?

    init(symbol: String?, fallback: String) {
        super.init(frame: .zero)
        isBordered = false
        wantsLayer = true
        layer?.cornerRadius = Metrics.radiusChip
        if let symbol, let image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil) {
            self.image = image.withSymbolConfiguration(.init(pointSize: 14, weight: .regular))
            imagePosition = .imageOnly
        } else {
            title = fallback
            font = Fonts.mono(12, weight: .medium)
        }
        target = self
        action = #selector(pressed)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    @objc private func pressed() {
        onPress?()
    }

    func paint(selected: Bool, palette: Palette) {
        layer?.backgroundColor = selected ? palette.accentWash.color.cgColor : NSColor.clear.cgColor
        contentTintColor = selected ? palette.accentText.color : palette.text2.color
        if image == nil {
            attributedTitle = NSAttributedString(string: title, attributes: [
                .font: Fonts.mono(12, weight: .medium),
                .foregroundColor: selected ? palette.accentText.color : palette.text2.color,
            ])
        }
    }
}

/// A colour to pick: a disc, ringed in Cobalt when chosen. Without a colour, *More…*: a ring with a plus.
final class SwatchButton: NSButton {
    let colour: RGBA?
    var onPress: (() -> Void)?
    private var selected = false
    private var palette = Palette.light

    init(colour: RGBA?) {
        self.colour = colour
        super.init(frame: .zero)
        isBordered = false
        title = ""
        target = self
        action = #selector(pressed)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    @objc private func pressed() {
        onPress?()
    }

    func paint(selected: Bool, palette: Palette) {
        self.selected = selected
        self.palette = palette
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        let disc = bounds.insetBy(dx: 3, dy: 3)
        if let colour {
            colour.color.setFill()
            NSBezierPath(ovalIn: disc).fill()
            palette.hairlineStrong.color.setStroke()
            NSBezierPath(ovalIn: disc.insetBy(dx: 0.5, dy: 0.5)).stroke()
        } else {
            palette.text2.color.setStroke()
            let ring = NSBezierPath(ovalIn: disc.insetBy(dx: 0.5, dy: 0.5))
            ring.lineWidth = 1
            ring.stroke()
            let plus = NSBezierPath()
            plus.move(to: CGPoint(x: disc.midX - 3.5, y: disc.midY))
            plus.line(to: CGPoint(x: disc.midX + 3.5, y: disc.midY))
            plus.move(to: CGPoint(x: disc.midX, y: disc.midY - 3.5))
            plus.line(to: CGPoint(x: disc.midX, y: disc.midY + 3.5))
            plus.lineWidth = 1.5
            plus.stroke()
        }
        if selected {
            palette.accent.color.setStroke()
            let ring = NSBezierPath(ovalIn: bounds.insetBy(dx: 1, dy: 1))
            ring.lineWidth = 2
            ring.stroke()
        }
    }
}

/// A text button: Ink (the window's one primary action) or plain.
final class ActionButton: NSButton {
    enum Kind {
        case ink, plain, danger
    }

    let kind: Kind
    var onPress: (() -> Void)?

    init(_ label: String, kind: Kind) {
        self.kind = kind
        super.init(frame: .zero)
        title = label
        isBordered = false
        wantsLayer = true
        layer?.cornerRadius = Metrics.radiusControl
        target = self
        action = #selector(pressed)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    @objc private func pressed() {
        onPress?()
    }

    var width: CGFloat {
        (title as NSString).size(withAttributes: [.font: Fonts.body(13, weight: .medium)]).width + 26
    }

    func paint(palette: Palette) {
        let fg: RGBA
        switch kind {
        case .ink:
            layer?.backgroundColor = palette.primaryBg.color.cgColor
            layer?.borderWidth = 0
            fg = palette.primaryFg
        case .plain, .danger:
            layer?.backgroundColor = palette.card.color.cgColor
            layer?.borderColor = palette.hairlineStrong.color.cgColor
            layer?.borderWidth = 1
            fg = kind == .danger ? palette.signalText : palette.text
        }
        attributedTitle = NSAttributedString(string: title, attributes: [
            .font: Fonts.body(13, weight: .medium), .foregroundColor: fg.color,
        ])
    }
}

final class FooterView: NSView {
    static let height: CGFloat = 40

    weak var editor: EditorWindow? {
        didSet { handle.editor = editor }
    }
    let status = NSTextField(labelWithString: "")
    let colour = ColourReadout()
    let codeChip = ActionButton("QR", kind: .plain)
    let textButton = ActionButton("Copy text", kind: .plain)
    let handle = DragHandle()
    let pinButton = ActionButton("Pin", kind: .plain)
    let saveButton = ActionButton("Save", kind: .plain)
    let copyButton = ActionButton("Copy", kind: .ink)

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        status.lineBreakMode = .byTruncatingTail
        addSubview(status)
        addSubview(colour)
        codeChip.isHidden = true
        codeChip.toolTip = "A QR code in the shot"
        codeChip.onPress = { [weak self] in self?.editor?.showCode() }
        addSubview(codeChip)
        textButton.toolTip = "Copy the shot's text  ⇧⌘C"
        textButton.onPress = { [weak self] in self?.editor?.copyText() }
        addSubview(textButton)
        addSubview(handle)
        pinButton.toolTip = "Pin  ⌘P"
        saveButton.toolTip = "Save  ⌘S"
        copyButton.toolTip = "Copy  ⌘C"
        pinButton.onPress = { [weak self] in self?.editor?.pin() }
        saveButton.onPress = { [weak self] in self?.editor?.save() }
        copyButton.onPress = { [weak self] in self?.editor?.copyShot() }
        for button in [pinButton, saveButton, copyButton] { addSubview(button) }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override func layout() {
        super.layout()
        let h: CGFloat = 28
        let y = (bounds.height - h) / 2
        var x = bounds.maxX - 12
        for button in [copyButton, saveButton, pinButton] {
            x -= button.width
            button.frame = CGRect(x: x, y: y, width: button.width, height: h)
            x -= 8
        }
        handle.frame = CGRect(x: x - 40, y: y, width: 40, height: h)
        x -= 48 + textButton.width
        textButton.frame = CGRect(x: x, y: y, width: textButton.width, height: h)
        let right = x - 12
        // Left: the status as wide as it needs, then the colour, then the QR chip, all within what is left.
        let statusWidth = min(ceil(status.attributedStringValue.size().width) + 4, max(0, right - 14))
        status.frame = CGRect(x: 14, y: y + 6, width: statusWidth, height: 18)
        var left = status.frame.maxX + 14
        colour.frame = CGRect(x: left, y: y + 4, width: min(110, max(0, right - left)), height: 20)
        left = colour.frame.maxX + 8
        codeChip.frame = CGRect(x: left, y: y, width: min(codeChip.width, max(0, right - left)), height: h)
    }

    /// The colour under the pointer changed.
    func refreshColour() {
        colour.colour = editor?.colourUnderPointer
        colour.palette = editor?.palette ?? .light
        colour.needsDisplay = true
    }

    func refresh() {
        guard let editor else { return }
        let palette = editor.palette
        layer?.backgroundColor = palette.card.color.cgColor
        for button in [pinButton, saveButton, copyButton, textButton, codeChip] { button.paint(palette: palette) }
        if let code = editor.codes.first {
            let shown = Links.web(code)?.host ?? code
            codeChip.title = "QR · " + (shown.count > 26 ? String(shown.prefix(25)) + "…" : shown)
            codeChip.paint(palette: palette)
            codeChip.isHidden = false
        } else {
            codeChip.isHidden = true
        }
        needsLayout = true
        handle.palette = palette
        handle.needsDisplay = true
        status.attributedStringValue = NSAttributedString(string: FooterView.status(editor), attributes: [
            .font: Fonts.mono(12), .foregroundColor: palette.text2.color,
        ])
        needsDisplay = true
    }

    /// `3024 × 1964 px · 62 % · Edited`.
    static func status(_ editor: EditorWindow) -> String {
        let doc = editor.document
        let size: CGSize
        if let box = editor.canvas.cropBox {
            size = box.size
        } else {
            size = Renderer.layout(doc).size
        }
        let retina = editor.hooks.settings().retina
        let factor = retina ? 1 : 1 / doc.scale
        let pixels = "\(Int((size.width * factor).rounded())) × \(Int((size.height * factor).rounded())) px"
        let zoom = "\(Int((editor.scroll.magnification * 100).rounded())) %"
        let state: String
        if editor.canvas.cropBox != nil {
            state = "Cropping: Return keeps it, Esc doesn't"
        } else if doc.dirty {
            state = "Edited"
        } else if editor.shot.savedTo != nil {
            state = "Saved"
        } else {
            state = "Copied"
        }
        return [pixels, zoom, state].joined(separator: "  ·  ")
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let editor else { return }
        editor.palette.hairline.color.setFill()
        CGRect(x: 0, y: bounds.maxY - 1, width: bounds.width, height: 1).fill()
    }
}

/// A thumbnail of the shot that drags it out as a file (decision 5): a copy, so Finder never moves the saved shot.
final class DragHandle: NSView, NSDraggingSource {
    weak var editor: EditorWindow?
    var palette = Palette.light
    private var pressed = false

    override init(frame: NSRect) {
        super.init(frame: frame)
        toolTip = "Drag the shot into another app"
        setAccessibilityLabel("Drag the shot")
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let editor, let ctx = NSGraphicsContext.current?.cgContext else { return }
        let box = bounds.insetBy(dx: 1, dy: 1)
        let image = editor.shot.image
        let fit = min(box.width / CGFloat(image.width), box.height / CGFloat(image.height))
        let size = CGSize(width: CGFloat(image.width) * fit, height: CGFloat(image.height) * fit)
        let rect = CGRect(x: box.midX - size.width / 2, y: box.midY - size.height / 2, width: size.width,
                          height: size.height)
        ctx.saveGState()
        ctx.interpolationQuality = .medium
        ctx.addPath(CGPath(roundedRect: rect, cornerWidth: 3, cornerHeight: 3, transform: nil))
        ctx.clip()
        ctx.draw(image, in: rect)
        ctx.restoreGState()
        palette.hairlineStrong.color.setStroke()
        NSBezierPath(roundedRect: rect.insetBy(dx: -0.5, dy: -0.5), xRadius: 3.5, yRadius: 3.5).stroke()
    }

    override func resetCursorRects() {
        addCursorRect(bounds, cursor: .openHand)
    }

    override func mouseDown(with event: NSEvent) {
        pressed = true
    }

    override func mouseDragged(with event: NSEvent) {
        guard pressed, let editor else { return }
        pressed = false
        guard let url = editor.dragFile() else { return }
        let item = NSPasteboardItem()
        item.setString(url.absoluteString, forType: .fileURL)
        if url.pathExtension.lowercased() == "png", let png = try? Data(contentsOf: url) {
            item.setData(png, forType: .png)
        }
        let dragging = NSDraggingItem(pasteboardWriter: item)
        let image = NSImage(contentsOf: url) ?? NSImage(size: CGSize(width: 1, height: 1))
        let side: CGFloat = 140
        let fit = min(side / max(image.size.width, 1), side / max(image.size.height, 1), 1)
        let size = CGSize(width: image.size.width * fit, height: image.size.height * fit)
        let at = convert(event.locationInWindow, from: nil)
        dragging.setDraggingFrame(CGRect(x: at.x - size.width / 2, y: at.y - size.height / 2, width: size.width,
                                         height: size.height), contents: image)
        beginDraggingSession(with: [dragging], event: event, source: self)
    }

    override func mouseUp(with event: NSEvent) {
        pressed = false
    }

    func draggingSession(_ session: NSDraggingSession,
                         sourceOperationMaskFor context: NSDraggingContext) -> NSDragOperation {
        .copy
    }

    func draggingSession(_ session: NSDraggingSession, endedAt screenPoint: NSPoint, operation: NSDragOperation) {
        if !operation.isEmpty { editor?.dragLanded() }
    }
}

/// The colour under the pointer: a swatch and its hex, in mono. Tab copies it.
final class ColourReadout: NSView {
    var colour: Pixel?
    var palette = Palette.light

    override init(frame: NSRect) {
        super.init(frame: frame)
        toolTip = "The colour under the pointer: Tab copies the hex, ⇧Tab rgb()"
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let colour else { return }
        let side = bounds.height - 6
        let swatch = CGRect(x: 0, y: 3, width: side, height: side)
        colour.rgba.color.setFill()
        NSBezierPath(roundedRect: swatch, xRadius: 3, yRadius: 3).fill()
        palette.hairlineStrong.color.setStroke()
        NSBezierPath(roundedRect: swatch.insetBy(dx: 0.5, dy: 0.5), xRadius: 3, yRadius: 3).stroke()
        (colour.hex as NSString).draw(at: CGPoint(x: side + 6, y: 2), withAttributes: [
            .font: Fonts.mono(12), .foregroundColor: palette.text2.color,
        ])
    }
}
