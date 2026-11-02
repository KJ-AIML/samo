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
    private let board = SystemPasteboard()
    private var watcher: Watcher?
    private var timer: Timer?
    private let paste = MacPaste.flow()
    private var thumbnails: [UUID: CGImage] = [:]
    private(set) var refused = false
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
    }

    func start(out: Output) {
        let session = Session(out: out, shared: shared, sharedNote: ClipsPage.sharedNote, handlers: Session.Handlers(
            page: { ClipsPage.starting() },
            shutdown: { [unowned self] in self.shutdown() }
        ))
        self.session = session
        let watcher = Watcher(pasteboard: board, frontmost: { NSWorkspace.shared.frontmostApplication?.bundleIdentifier })
        self.watcher = watcher
        // Twice a second, one counter read (R21); a little tolerance lets macOS group the wake-ups.
        let timer = Timer(timeInterval: Watcher.interval, repeats: true) { [weak self] _ in self?.tick() }
        timer.tolerance = 0.1
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
        registerShortcut()
        session.start()
    }

    private func registerShortcut() {
        refused = !HotKeys.shared.register(id: ClipsApp.shortcutId, ClipsShortcut.standard) { [weak self] in
            self?.toggleList()
        }
    }

    // MARK: Watching

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
