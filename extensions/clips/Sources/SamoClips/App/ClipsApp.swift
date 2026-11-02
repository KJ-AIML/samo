// Clips, put together: its session with Samo, the watcher (twice a second), the history on disk, ⇧⌘C and the list,
// and pasting. Everything here runs on the main thread.

import AppKit
import ClipsCore
import ClipsUI
import ImageIO
import SamoKit

final class ClipsApp {
    static let shortcutId: UInt32 = 1

    let dataDir: URL
    let shared: Bool
    private(set) var session: Session?
    private let store: Store
    private(set) var history: History
    private var settings: ClipsSettings
    private let board = SystemPasteboard()
    private var watcher: Watcher?
    private var timer: Timer?
    private let paste = MacPaste.flow()
    private var thumbnails: [UUID: CGImage] = [:]
    private(set) var refused = false
    private var confirmClear = false
    private var stopped = false

    private lazy var list = ListWindow(hooks: ListHooks(
        items: { [unowned self] in self.history.items },
        thumbnail: { [unowned self] item in self.thumbnail(item) },
        pick: { [unowned self] item, plain in self.pick(item, plain: plain) },
        pin: { [unowned self] item in self.pin(item) },
        delete: { [unowned self] item in self.delete(item) },
        closed: { [unowned self] picked in if !picked { self.paste.cancelled() } }
    ))

    init(dataDir: URL, shared: Bool) {
        self.dataDir = dataDir
        self.shared = shared
        store = Store(dir: dataDir)
        history = store.load()
        settings = ClipsSettings.load(dataDir: dataDir)
    }

    func start(out: Output) {
        let session = Session(out: out, shared: shared, sharedNote: ClipsPage.sharedNote, handlers: Session.Handlers(
            page: { [unowned self] in self.page() },
            setting: { [unowned self] key, on in self.setting(key, on) },
            action: { [unowned self] id, row in self.action(id, row: row) },
            choice: { [unowned self] key, value in self.choice(key, value) },
            shutdown: { [unowned self] in self.shutdown() }
        ))
        self.session = session
        watcher = Watcher(pasteboard: board, frontmost: { NSWorkspace.shared.frontmostApplication?.bundleIdentifier })
        if settings.watching { startWatching() }
        registerShortcut()
        session.start()
        session.out.send(ClipsPage.badge(history))
    }

    private func registerShortcut() {
        guard let shortcut = settings.shortcut else { return }
        refused = !HotKeys.shared.register(id: ClipsApp.shortcutId, shortcut) { [weak self] in
            self?.toggleList()
        }
    }

    // MARK: Watching

    /// Twice a second, one counter read (R21); a little tolerance lets macOS group the wake-ups. A copy made while
    /// Clips wasn't watching isn't picked up when it starts.
    private func startWatching() {
        guard timer == nil else { return }
        watcher?.skipCurrent()
        let timer = Timer(timeInterval: Watcher.interval, repeats: true) { [weak self] _ in self?.tick() }
        timer.tolerance = 0.1
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    private func stopWatching() {
        timer?.invalidate()
        timer = nil
    }

    private func setting(_ key: String, _ on: Bool) {
        guard key == "watching" else { return }
        settings.watching = on
        try? settings.save(dataDir: dataDir)
        if on { startWatching() } else { stopWatching() }
    }

    private func tick() {
        guard let capture = watcher?.tick() else { return }
        let added = history.add(capture, at: Date())
        try? store.keep(capture, as: added, in: &history)
        for gone in added.dropped {
            store.delete(gone)
            thumbnails[gone.id] = nil
        }
        save()
    }

    private func save() {
        try? store.save(history)
        if list.isOpen { list.reload() }
        session?.refresh()
        session?.out.send(ClipsPage.badge(history))
    }

    // MARK: The page

    func page() -> [Widget] {
        let running = NSWorkspace.shared.runningApplications.compactMap(\.bundleIdentifier)
        return ClipsPage.build(PageState(
            paste: ClipsApp.pasteAccess(), accessibility: AXIsProcessTrusted(), shared: shared, history: history,
            refused: refused, clash: ClipsPage.runningClipboardApp(bundleIds: running),
            pausedUntil: watcher?.pausedUntil, ignoreNext: watcher?.ignoreNext ?? false, confirmClear: confirmClear,
            appName: ClipsApp.appName, shortcut: settings.shortcut, watching: settings.watching
        ))
    }

    /// Paste from Other Apps, as macOS reports it for Clips (macOS 26 and later ask; before, it was always allowed).
    static func pasteAccess() -> PasteAccess {
        if #available(macOS 26.0, *) {
            switch NSPasteboard.general.accessBehavior {
            case .alwaysAllow: return .allowed
            case .alwaysDeny: return .denied
            default: return .asks
            }
        }
        return .allowed
    }

    static func appName(_ bundleId: String) -> String? {
        NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleId)
            .map { FileManager.default.displayName(atPath: $0.path) }
            .map { $0.hasSuffix(".app") ? String($0.dropLast(4)) : $0 }
    }

    private func action(_ id: String, row: String?) {
        if id != "clear" { confirmClear = false }
        if let row, let uuid = UUID(uuidString: row), let item = history.item(uuid) {
            switch id {
            case "copy":
                let data = item.dataFile.flatMap(store.read)
                let text: String? = item.kind == .image || item.kind == .files ? nil : store.fullText(item)
                watcher?.own(board.put(text: text, data: data, dataType: data == nil ? nil : item.dataType,
                                       files: item.files))
                Note.show("Copied", problem: false)
            case "pin", "unpin":
                history.pin(uuid, id == "pin")
                save()
            case "delete":
                delete(item)
            default:
                break
            }
            return
        }
        switch id {
        case "open-paste-settings": SystemSettings.open(.pasteFromOtherApps)
        case "allow-accessibility":
            MacPaste.askForAccessibility()
            SystemSettings.open(.accessibility)
        case "pause": watcher?.pausedUntil = Date().addingTimeInterval(15 * 60)
        case "resume": watcher?.pausedUntil = nil
        case "ignore-next": watcher?.ignoreNext = true
        case "clear": confirmClear = true
        case "clear-confirm":
            for gone in history.clear() {
                store.delete(gone)
                thumbnails[gone.id] = nil
            }
            save()
        default: break
        }
    }

    private func choice(_ key: String, _ value: String) {
        guard key == "limit", let limit = Int(value), [50, 200, 500].contains(limit) else { return }
        for gone in history.setLimit(limit) {
            store.delete(gone)
            thumbnails[gone.id] = nil
        }
        save()
    }

    // MARK: The list

    private func toggleList() {
        if list.isOpen {
            list.close(picked: false)
            return
        }
        let me = ProcessInfo.processInfo.processIdentifier
        let front = NSWorkspace.shared.frontmostApplication?.processIdentifier
        paste.remember(front == me ? nil : front)
        list.show(near: NSEvent.mouseLocation)
    }

    /// Puts the item on the clipboard (as Clips' own copy, not a new one), then pastes where you were.
    private func pick(_ item: Item, plain: Bool) {
        let data = plain && item.kind == .rich ? nil : item.dataFile.flatMap(store.read)
        let text: String? = item.kind == .image || item.kind == .files ? nil : store.fullText(item)
        let count = board.put(text: text, data: data, dataType: data == nil ? nil : item.dataType, files: item.files)
        watcher?.own(count)
        paste.picked { outcome in
            if case let .copiedOnly(words) = outcome {
                Note.show(words, problem: false)
            }
        }
    }

    private func pin(_ item: Item) {
        history.pin(item.id, !item.pinned)
        save()
    }

    private func delete(_ item: Item) {
        history.delete(item.id)
        store.delete(item)
        thumbnails[item.id] = nil
        save()
    }

    /// A small picture of an image item, made once.
    private func thumbnail(_ item: Item) -> CGImage? {
        if let cached = thumbnails[item.id] { return cached }
        guard let data = item.dataFile.flatMap(store.read),
              let source = CGImageSourceCreateWithData(data as CFData, nil),
              let thumb = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                  kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 64,
              ] as CFDictionary)
        else { return nil }
        thumbnails[item.id] = thumb
        return thumb
    }

    /// Off, update, uninstall, or Samo quitting: the watch stops, the shortcut and the list go, the history stays.
    func shutdown() {
        guard !stopped else { return }
        stopped = true
        timer?.invalidate()
        timer = nil
        HotKeys.shared.unregister(ClipsApp.shortcutId)
        list.close(picked: false)
        try? store.save(history)
    }
}
