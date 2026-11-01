// Shots, put together: its settings and recent list, Screen Recording, its shortcuts, capture, and its page in
// Samo. Everything here runs on the main thread.

import AppKit
import ImageIO
import ShotsCore

final class ShotsApp {
    private(set) var settings: Settings
    private(set) var recent: Recent
    let dataDir: URL
    let permission = Permission()
    let shared: Bool
    private(set) var session: Session?
    private var capture: CaptureController?
    private let store = ShotStore()
    private lazy var pins = Pins(store: store)
    private var stopped = false
    /// Shortcuts macOS refused, by capture mode.
    private(set) var refused: Set<String> = []

    init(dataDir: URL, shared: Bool) {
        self.dataDir = dataDir
        self.shared = shared
        settings = Settings.load(dataDir: dataDir)
        recent = Recent.load(dataDir: dataDir)
        recent.forgetMissing()
    }

    func start(out: Output) {
        let session = Session(out: out, shared: shared, handlers: Session.Handlers(
            page: { [unowned self] in self.page() },
            setting: { [unowned self] key, value in self.setting(key, value) },
            action: { [unowned self] id, row in self.action(id, row: row) },
            choice: { [unowned self] key, value in self.choice(key, value) },
            shutdown: { [unowned self] in self.shutdown() }
        ))
        self.session = session
        capture = CaptureController(
            permission: permission,
            settings: { [unowned self] in self.settings },
            out: out,
            openPin: { [unowned self] shot in self.pins.open(shot) },
            onPermission: { [unowned self] in self.session?.refresh() },
            onShot: { [unowned self] shot, outcome in self.captured(shot, outcome) }
        )
        registerShortcuts()
        session.start()
        session.out.send(Page.badge(recent))
    }

    private func registerShortcuts() {
        HotKeys.shared.unregisterAll()
        refused = []
        let modes: [(id: UInt32, mode: String, capture: CaptureController.Mode)] = [
            (1, "screen", .screen), (2, "area", .area),
        ]
        for entry in modes {
            guard let shortcut = settings.shortcuts[entry.mode] else { continue }
            let ok = HotKeys.shared.register(id: entry.id, shortcut) { [unowned self] in
                self.capture?.start(entry.capture, fromPage: false)
            }
            if !ok { refused.insert(entry.mode) }
        }
    }

    private func captured(_ shot: Shot, _ outcome: CaptureOutcome) {
        if let url = outcome.savedTo {
            recent.add(RecentShot(path: url.path, bytes: outcome.bytes, at: shot.takenAt))
            try? recent.save(dataDir: dataDir)
        }
        refresh()
    }

    // MARK: From Samo

    private func setting(_ key: String, _ value: Bool) {
        if settings.apply(setting: key, value) {
            try? settings.save(dataDir: dataDir)
        }
    }

    private func choice(_ key: String, _ value: String) {
        if settings.apply(choice: key, value) {
            try? settings.save(dataDir: dataDir)
        }
    }

    private func action(_ id: String, row: String?) {
        if let row {
            recentAction(id, path: row)
            return
        }
        switch id {
        case "area": capture?.start(.area, fromPage: true)
        case "window": capture?.start(.window, fromPage: true)
        case "screen": capture?.start(.screen, fromPage: true)
        case "allow": permission.request()
        case "open-settings": Links.openScreenRecordingSettings()
        case "restart":
            shutdown()
            exit(restartExitCode)
        case "change-shortcuts", "change-folder": settingsWindow.show()
        default: break
        }
    }

    /// Copy, Reveal or Pin on a recent shot, by its path.
    private func recentAction(_ id: String, path: String) {
        let url = URL(fileURLWithPath: path)
        guard FileManager.default.fileExists(atPath: path) else {
            recent.forgetMissing()
            try? recent.save(dataDir: dataDir)
            Note.show("That shot has been moved or deleted", problem: true)
            return
        }
        switch id {
        case "reveal":
            NSWorkspace.shared.activateFileViewerSelecting([url])
        case "copy":
            if let data = try? Data(contentsOf: url), let image = NSImage(data: data),
               let tiff = image.tiffRepresentation, let rep = NSBitmapImageRep(data: tiff),
               let png = rep.representation(using: .png, properties: [:]), SystemClipboard().put(png: png) {
                Note.show("Copied", problem: false)
            } else {
                Note.show("Couldn't copy that shot", problem: true)
            }
        case "pin":
            guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
                  let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
            else {
                Note.show("Couldn't open that shot", problem: true)
                return
            }
            let shot = Shot(image: image, scale: NSScreen.main?.backingScaleFactor ?? 2, takenAt: Date())
            shot.savedTo = url
            pins.open(shot)
        default:
            break
        }
    }

    /// Off, update, uninstall, or Samo quitting: nothing unsaved is lost, then shortcuts and windows go.
    func shutdown() {
        guard !stopped else { return }
        stopped = true
        let saved = store.saveUnsaved(settings: settings, disk: FileDisk())
        if !saved.isEmpty {
            for url in saved {
                recent.add(RecentShot(path: url.path, bytes: 0, at: Date()))
            }
            try? recent.save(dataDir: dataDir)
            session?.out.send(.log(event: "saved-unsaved", detail: saved.map(\.path).joined(separator: "\n")))
        }
        pins.closeAll()
        HotKeys.shared.unregisterAll()
    }

    // MARK: The page

    private lazy var settingsWindow = SettingsWindow(
        get: { [unowned self] in self.settings },
        set: { [unowned self] new in
            self.settings = new
            try? new.save(dataDir: self.dataDir)
            self.registerShortcuts()
            self.refresh()
        },
        pauseShortcuts: { [unowned self] paused in
            if paused {
                HotKeys.shared.unregisterAll()
            } else {
                self.registerShortcuts()
            }
        }
    )

    func refresh() {
        session?.refresh()
        session?.out.send(Page.badge(recent))
    }

    func page() -> [Widget] {
        let running = NSWorkspace.shared.runningApplications.compactMap(\.bundleIdentifier)
        let symbolic = UserDefaults(suiteName: "com.apple.symbolichotkeys")?.dictionary(forKey: "AppleSymbolicHotKeys")
        let clashes = Conflicts.clashes(
            ours: settings.shortcuts,
            macOS: Conflicts.macScreenshotShortcuts(symbolicHotKeys: symbolic),
            apps: Conflicts.runningScreenshotApps(bundleIds: running)
        )
        let home = URL(fileURLWithPath: Settings.home, isDirectory: true)
        let desktopSynced = FileManager.default.fileExists(
            atPath: home.appendingPathComponent("Library/Mobile Documents/com~apple~CloudDocs/Desktop").path
        )
        return Page.build(PageState(
            access: permission.state, shared: shared, settings: settings, recent: recent, refused: refused,
            clashes: clashes, folderInICloud: Page.inICloud(settings.folderURL, home: home, desktopSynced: desktopSynced)
        ))
    }
}
