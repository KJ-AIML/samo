// The editor (ADR-004 S3): one window per shot, with the tools on top, the shot in the middle and Copy, Save, Pin
// and a drag handle at the foot. It asks before discarding changes that weren't copied or saved (R19), and lets the
// shot go when it closes (R17).
//
// Keys: a tool's letter picks it (M measures); ⌘Z, ⇧⌘Z; ⌫ deletes; [ ] change the width; ⌘C copy, ⇧⌘C copy the
// shot's text, ⌘S save, ⇧⌘S save as, ⌘P pin, ⌘↩ copy and close, ⌘W close; ⌘+ ⌘− zoom, ⌘0 fit, ⌘1 real size; Tab
// copies the colour under the pointer (⇧Tab as rgb()); Esc deselects, then closes. While a text mark is typed, keys
// are text.

import AppKit
import ShotsCore

/// What the editor needs from the rest of Shots.
public struct EditorHooks {
    public var settings: () -> Settings
    public var clipboard: Clipboard
    public var disk: Disk
    /// `data/drag/`, for files a drag carries.
    public var dragFolder: URL
    /// A save landed: for the recent list.
    public var saved: (Shot, ExportResult) -> Void
    public var pin: (Shot) -> Void
    /// The editor opened: keep the shot, and how to get its unsaved edits on shutdown.
    public var opened: (Shot, @escaping () -> CGImage?) -> Void
    /// The editor closed: let the shot go.
    public var closed: (EditorWindow) -> Void
    public var note: (String, Bool) -> Void
    /// Reading text and QR codes (Vision, on the Mac); tests stand in for them.
    public var readText: (CGImage) async -> [String]
    public var readCodes: (CGImage) async -> [String]

    public init(
        settings: @escaping () -> Settings, clipboard: Clipboard, disk: Disk, dragFolder: URL,
        saved: @escaping (Shot, ExportResult) -> Void, pin: @escaping (Shot) -> Void,
        opened: @escaping (Shot, @escaping () -> CGImage?) -> Void, closed: @escaping (EditorWindow) -> Void,
        note: @escaping (String, Bool) -> Void = { Note.show($0, problem: $1) },
        readText: @escaping (CGImage) async -> [String] = { await TextReader.read($0) },
        readCodes: @escaping (CGImage) async -> [String] = { await QRReader.read($0) }
    ) {
        self.settings = settings
        self.clipboard = clipboard
        self.disk = disk
        self.dragFolder = dragFolder
        self.saved = saved
        self.pin = pin
        self.opened = opened
        self.closed = closed
        self.note = note
        self.readText = readText
        self.readCodes = readCodes
    }
}

/// The editor's window: it sends the editor's commands to the editor before AppKit's menus see them, unless a text
/// mark is being typed.
public final class EditorNSWindow: NSWindow {
    weak var editor: EditorWindow?

    override public func performKeyEquivalent(with event: NSEvent) -> Bool {
        if !(firstResponder is NSTextView), let editor, editor.command(event) { return true }
        return super.performKeyEquivalent(with: event)
    }
}

public final class EditorWindow: NSObject, NSWindowDelegate {
    /// The last style used, so the next editor starts with it.
    static var lastStyle = Style.standard

    public let shot: Shot
    public let document: Document
    let redaction = Redaction()
    let hooks: EditorHooks
    public let window: EditorNSWindow
    let toolbar: ToolbarView
    let footer: FooterView
    let scroll: NSScrollView
    let canvas: CanvasView
    private(set) var palette: Palette

    public private(set) var tool: Tool = .arrow
    public var style: Style {
        didSet { EditorWindow.lastStyle = style }
    }
    /// The discard question is up.
    public private(set) var asking = false
    public private(set) var isClosed = false
    private var sheet: DiscardSheet?
    private var closing = false
    /// The zoom follows the window until the person zooms.
    var fitting = true
    /// The shot's pixels in sRGB, made when the pointer first needs a colour or a measurement.
    private(set) lazy var grid: PixelGrid? = PixelGrid(shot.image)
    /// The colour under the pointer, for the footer and Tab.
    public private(set) var colourUnderPointer: Pixel?
    /// QR codes in the shot, found in the background when the editor opens.
    public private(set) var codes: [String] = []
    /// Text being read for ⇧⌘C.
    public private(set) var readingText = false

    public init(shot: Shot, hooks: EditorHooks) {
        self.shot = shot
        self.hooks = hooks
        document = Document(size: CGSize(width: shot.image.width, height: shot.image.height), scale: shot.scale)
        style = EditorWindow.lastStyle
        palette = Palette.current(NSApplication.shared.effectiveAppearance)
        window = EditorNSWindow(
            contentRect: CGRect(x: 0, y: 0, width: 900, height: 600),
            styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false
        )
        toolbar = ToolbarView()
        footer = FooterView()
        scroll = NSScrollView()
        canvas = CanvasView()
        super.init()
        window.editor = self
        window.isReleasedWhenClosed = false
        window.delegate = self
        window.title = EditorWindow.title(for: shot)
        // Wide enough for the whole toolbar.
        window.minSize = CGSize(width: ToolbarView.minimumWidth, height: 420)
        window.collectionBehavior = [.fullScreenPrimary]
        window.tabbingMode = .disallowed
        build()
        hooks.opened(shot) { [weak self] in
            guard let self, self.document.dirty else { return nil }
            return Renderer.export(self.document, base: self.shot.image, redaction: self.redaction, retina: true)
        }
        changed()
        findCodes()
    }

    /// Looks for QR codes in the background; a chip appears in the footer if there are any.
    private func findCodes() {
        let image = shot.image
        let read = hooks.readCodes
        Task { @MainActor [weak self] in
            let codes = await read(image)
            guard let self, !self.isClosed, !codes.isEmpty else { return }
            self.codes = codes
            self.footer.refresh()
        }
    }

    static func title(for shot: Shot) -> String {
        if let saved = shot.savedTo { return saved.deletingPathExtension().lastPathComponent }
        return (FileNaming.name(at: shot.takenAt, format: .png) as NSString).deletingPathExtension
    }

    private func build() {
        let root = RootView(editor: self)
        window.contentView = root
        scroll.contentView = CenteringClipView()
        scroll.documentView = canvas
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        scroll.allowsMagnification = true
        scroll.minMagnification = 0.05
        scroll.maxMagnification = 16
        scroll.drawsBackground = true
        canvas.editor = self
        toolbar.editor = self
        footer.editor = self
        root.addSubview(scroll)
        root.addSubview(toolbar)
        root.addSubview(footer)
        canvas.relayout()
        applyPalette()
        NotificationCenter.default.addObserver(self, selector: #selector(magnifyStarted),
                                               name: NSScrollView.willStartLiveMagnifyNotification, object: scroll)
        NotificationCenter.default.addObserver(self, selector: #selector(magnified),
                                               name: NSScrollView.didEndLiveMagnifyNotification, object: scroll)
        window.initialFirstResponder = canvas.marks
    }

    func applyPalette() {
        palette = Palette.current(window.effectiveAppearance)
        scroll.backgroundColor = palette.shell.color
        toolbar.refresh()
        footer.refresh()
        canvas.needsDisplay = true
        canvas.marks.needsDisplay = true
    }

    /// Lays the window out at its first size, fitted, without showing it.
    public func prepare() {
        let points = CGSize(width: canvas.frame.width, height: canvas.frame.height)
        let screen = screenForShot()?.visibleFrame ?? CGRect(x: 0, y: 0, width: 1440, height: 900)
        let chrome = CGSize(width: 64, height: ToolbarView.height + FooterView.height + 64)
        let width = min(max(points.width + chrome.width, window.minSize.width), screen.width * 0.9)
        let height = min(max(points.height + chrome.height, window.minSize.height), screen.height * 0.9)
        window.setContentSize(CGSize(width: width, height: height))
        window.contentView?.layoutSubtreeIfNeeded()
        fit()
    }

    /// Shows the editor and gives it the keyboard.
    public func show() {
        prepare()
        if let screen = screenForShot()?.visibleFrame {
            window.setFrameOrigin(CGPoint(x: screen.midX - window.frame.width / 2,
                                          y: screen.midY - window.frame.height / 2))
        }
        Activation.bringForward()
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(canvas.marks)
    }

    private func screenForShot() -> NSScreen? {
        let at = shot.origin ?? NSEvent.mouseLocation
        return NSScreen.screens.first { $0.frame.contains(at) } ?? NSScreen.main
    }

    // MARK: State

    /// After any change: redraw, and bring the toolbar, the footer and the edited dot up to date.
    func changed() {
        canvas.marks.needsDisplay = true
        toolbar.refresh()
        footer.refresh()
        window.isDocumentEdited = document.dirty
    }

    public func select(_ tool: Tool) {
        guard tool != self.tool else { return }
        canvas.marks.endTextEditing()
        if self.tool == .crop { canvas.leaveCrop(apply: true) }
        if self.tool == .measure { canvas.marks.clearMeasure() }
        self.tool = tool
        if tool == .crop { canvas.enterCrop() }
        if tool != .select, tool != .crop { document.selected = nil }
        changed()
        window.invalidateCursorRects(for: canvas.marks)
    }

    /// A colour or width: for the next mark and the selected one.
    public func setStyle(_ new: Style) {
        style = new
        if let id = document.selected {
            document.update(id) { $0.style = new }
        }
        changed()
    }

    public func setBackdrop(_ backdrop: Backdrop?) {
        document.setBackdrop(backdrop)
        canvas.relayout()
        changed()
        if fitting { fit() }
    }

    // MARK: Commands

    /// The editor's ⌘ keys. `true` if it was one.
    func command(_ event: NSEvent) -> Bool {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        guard flags.contains(.command) else { return false }
        let key = event.charactersIgnoringModifiers?.lowercased() ?? ""
        let shift = flags.contains(.shift)
        switch (key, shift) {
        case ("z", false): undo()
        case ("z", true): redo()
        case ("c", false): copyShot()
        case ("c", true): copyText()
        case ("s", false): save()
        case ("s", true): saveAs()
        case ("p", false): pin()
        case ("w", false): requestClose()
        case ("=", _), ("+", _): zoom(by: 1.25)
        case ("-", false): zoom(by: 0.8)
        case ("0", false): fit()
        case ("1", false): realSize()
        default:
            if event.keyCode == 36 || event.keyCode == 76 {
                copyAndClose()
            } else {
                return false
            }
        }
        return true
    }

    public func undo() {
        canvas.marks.endTextEditing()
        document.undo()
        afterHistory()
    }

    public func redo() {
        canvas.marks.endTextEditing()
        document.redo()
        afterHistory()
    }

    private func afterHistory() {
        canvas.relayout()
        changed()
        if fitting { fit() }
    }

    public func copyShot() {
        canvas.marks.endTextEditing()
        if let problem = Export.copy(document, shot: shot, redaction: redaction, retina: hooks.settings().retina,
                                     clipboard: hooks.clipboard) {
            hooks.note(problem, true)
        } else {
            hooks.note("Copied", false)
        }
        changed()
    }

    @discardableResult
    public func save() -> Bool {
        canvas.marks.endTextEditing()
        let result = Export.save(document, shot: shot, redaction: redaction, settings: hooks.settings(),
                                 disk: hooks.disk)
        changed()
        if let problem = result.problem {
            hooks.note(problem, true)
            return false
        }
        window.title = EditorWindow.title(for: shot)
        hooks.saved(shot, result)
        hooks.note("Saved", false)
        return true
    }

    public func saveAs() {
        canvas.marks.endTextEditing()
        let panel = NSSavePanel()
        panel.nameFieldStringValue = EditorWindow.title(for: shot) + ".png"
        panel.allowedContentTypes = [.png, .jpeg]
        panel.canCreateDirectories = true
        panel.beginSheetModal(for: window) { [weak self] response in
            guard let self, response == .OK, let url = panel.url else { return }
            let result = Export.saveAs(self.document, shot: self.shot, redaction: self.redaction,
                                       retina: self.hooks.settings().retina, to: url, disk: self.hooks.disk)
            self.changed()
            if let problem = result.problem {
                self.hooks.note(problem, true)
            } else {
                self.hooks.note("Saved", false)
            }
        }
    }

    public func pin() {
        canvas.marks.endTextEditing()
        guard let pinned = Export.pinned(document, shot: shot, redaction: redaction) else {
            hooks.note("Couldn't pin the shot", true)
            return
        }
        hooks.pin(pinned)
    }

    public func copyAndClose() {
        copyShot()
        if !document.dirty { close() }
    }

    // MARK: Reading and measuring

    /// The pointer is over the shot at `p` (its pixels, origin bottom-left), or left it.
    func pointerMoved(shot p: CGPoint?) {
        guard let p, let grid else {
            colourUnderPointer = nil
            footer.refreshColour()
            return
        }
        colourUnderPointer = grid.pixel(Int(p.x.rounded(.down)), grid.height - 1 - Int(p.y.rounded(.down)))
        footer.refreshColour()
    }

    /// Tab: the colour under the pointer as hex, or `rgb()` with ⇧.
    public func copyColour(css: Bool) {
        guard let colour = colourUnderPointer else {
            hooks.note("Point at a colour first", false)
            return
        }
        let text = css ? colour.css : colour.hex
        if hooks.clipboard.put(text: text) {
            hooks.note("Copied \(text)", false)
        } else {
            hooks.note("Couldn't copy the colour", true)
        }
    }

    /// ⇧⌘C: the text in the shot (the kept part, without marks), copied.
    public func copyText() {
        canvas.marks.endTextEditing()
        guard !readingText else { return }
        let visible = document.visibleRect
        let rows = CGRect(x: visible.minX, y: CGFloat(shot.image.height) - visible.maxY, width: visible.width,
                          height: visible.height)
        guard let image = shot.image.cropping(to: rows) else { return }
        readingText = true
        let read = hooks.readText
        let clipboard = hooks.clipboard
        Task { @MainActor [weak self] in
            let lines = await read(image)
            guard let self else { return }
            self.readingText = false
            let outcome = Reading.outcome(lines: lines, codes: [], clipboard: clipboard)
            self.hooks.note(outcome.note, outcome.problem)
        }
    }

    /// The footer's QR chip: the first code on the card.
    func showCode() {
        guard let code = codes.first else { return }
        ResultCard(.code(code), clipboard: hooks.clipboard, note: hooks.note).show(near: NSEvent.mouseLocation)
    }

    /// The file a drag carries, or `nil` with the problem said.
    func dragFile() -> URL? {
        canvas.marks.endTextEditing()
        let url = Export.dragFile(document, shot: shot, redaction: redaction, retina: hooks.settings().retina,
                                  folder: hooks.dragFolder, disk: hooks.disk)
        if url == nil { hooks.note("Couldn't make a file to drag", true) }
        return url
    }

    /// A drag was dropped somewhere: the edits are out.
    func dragLanded() {
        document.markExported()
        changed()
    }

    // MARK: Closing

    /// Close, asking first if there are changes not copied or saved.
    public func requestClose() {
        canvas.marks.endTextEditing()
        guard document.dirty else {
            close()
            return
        }
        guard !asking else { return }
        asking = true
        let sheet = DiscardSheet(palette: palette) { [weak self] choice in
            self?.answered(choice)
        }
        self.sheet = sheet
        if window.isVisible {
            window.beginSheet(sheet.window)
        }
    }

    func answered(_ choice: DiscardSheet.Choice) {
        if let sheet, window.attachedSheet === sheet.window {
            window.endSheet(sheet.window)
        }
        sheet = nil
        asking = false
        switch choice {
        case .saveAndClose:
            if save() { close() }
        case .discard:
            close()
        case .cancel:
            break
        }
    }

    /// The person chose Discard.
    public func discardAndClose() {
        answered(.discard)
    }

    public func saveAndClose() {
        answered(.saveAndClose)
    }

    public func cancelClose() {
        answered(.cancel)
    }

    /// Closes now, whatever is unsaved (shutdown has already saved it beside the shot).
    public func close() {
        guard !isClosed else { return }
        isClosed = true
        closing = true
        canvas.marks.endTextEditing()
        if let sheet, window.attachedSheet === sheet.window { window.endSheet(sheet.window) }
        sheet = nil
        NotificationCenter.default.removeObserver(self)
        window.orderOut(nil)
        window.close()
        redaction.clear()
        hooks.closed(self)
    }

    public func windowShouldClose(_ sender: NSWindow) -> Bool {
        if closing { return true }
        requestClose()
        return false
    }

    public func windowDidResize(_ notification: Notification) {
        if fitting { fit() }
    }

    public func windowDidChangeBackingProperties(_ notification: Notification) {
        canvas.marks.needsDisplay = true
    }

    /// Brings this editor forward again (it was opened twice).
    public func bringForward() {
        Activation.bringForward()
        if window.isMiniaturized { window.deminiaturize(nil) }
        window.makeKeyAndOrderFront(nil)
    }

    // MARK: Zoom

    /// The whole export in view, never bigger than its real size.
    public func fit() {
        let clip = scroll.contentView.bounds.size
        let available = CGSize(width: max(scroll.frame.width - 48, 40), height: max(scroll.frame.height - 48, 40))
        let size = canvas.frame.size
        guard size.width > 0, size.height > 0, clip.width > 0 else { return }
        let m = min(1, available.width / size.width, available.height / size.height)
        scroll.magnification = max(scroll.minMagnification, m)
        fitting = true
        footer.refresh()
    }

    public func realSize() {
        scroll.magnification = 1
        fitting = false
        footer.refresh()
    }

    public func zoom(by factor: CGFloat) {
        let centre = CGPoint(x: scroll.contentView.bounds.midX, y: scroll.contentView.bounds.midY)
        scroll.setMagnification(scroll.magnification * factor, centeredAt: centre)
        fitting = false
        footer.refresh()
    }

    @objc private func magnifyStarted() {
        fitting = false
    }

    @objc private func magnified() {
        footer.refresh()
    }
}

/// The window's content: the toolbar on top, the footer below, the shot between. It follows the appearance.
final class RootView: NSView {
    unowned let editor: EditorWindow

    init(editor: EditorWindow) {
        self.editor = editor
        super.init(frame: .zero)
        wantsLayer = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override func layout() {
        super.layout()
        let b = bounds
        editor.toolbar.frame = CGRect(x: 0, y: b.maxY - ToolbarView.height, width: b.width, height: ToolbarView.height)
        editor.footer.frame = CGRect(x: 0, y: 0, width: b.width, height: FooterView.height)
        editor.scroll.frame = CGRect(x: 0, y: FooterView.height, width: b.width,
                                     height: max(0, b.height - ToolbarView.height - FooterView.height))
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        editor.applyPalette()
    }
}

/// Bringing Shots forward. Shots has no Dock icon, and macOS 14's polite `activate()` is refused on this Mac when
/// another app is in front, so the editor would open behind it, without the keyboard. The older call still brings
/// it forward; it is reached through a protocol so its deprecation doesn't warn on every build.
enum Activation {
    static func bringForward() {
        (NSApplication.shared as ForcedActivation).activate(ignoringOtherApps: true)
    }
}

private protocol ForcedActivation {
    func activate(ignoringOtherApps flag: Bool)
}

extension NSApplication: ForcedActivation {}
