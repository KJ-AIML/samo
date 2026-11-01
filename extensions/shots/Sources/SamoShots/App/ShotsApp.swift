// Shots, put together: its settings and recent list, Screen Recording, its shortcuts, capture, and its page in
// Samo. Everything here runs on the main thread.

import AppKit
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
        session?.refresh()
    }

    // MARK: From Samo

    private func setting(_ key: String, _ value: Bool) {
        switch key {
        case "sound": settings.sound = value
        case "retina": settings.retina = value
        default: return
        }
        try? settings.save(dataDir: dataDir)
    }

    private func choice(_ key: String, _ value: String) {
        switch key {
        case "after": settings.after = Settings.After(rawValue: value) ?? settings.after
        case "format": settings.format = Settings.Format(rawValue: value) ?? settings.format
        default: return
        }
        try? settings.save(dataDir: dataDir)
    }

    private func action(_ id: String, row: String?) {
        switch id {
        case "area": capture?.start(.area, fromPage: true)
        case "window": capture?.start(.window, fromPage: true)
        case "screen": capture?.start(.screen, fromPage: true)
        case "allow": permission.request()
        case "open-settings": Links.openScreenRecordingSettings()
        case "restart": NSApp.terminate(self); exit(restartExitCode)
        default: break
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

    func page() -> [Widget] {
        var page: [Widget] = [.section(title: "SCREEN RECORDING", about: nil)]
        switch permission.state {
        case .allowed:
            page.append(.note(text: "Allowed for Shots.", kind: .plain))
        case .notYet:
            page.append(.note(text: "Shots needs Screen Recording to capture what's on screen. macOS asks once; "
                + "nothing leaves your Mac.", kind: .plain))
            page.append(.button(action: "allow", label: "Allow Screen Recording", kind: .plain))
        case .askedRestart:
            page.append(.note(text: "Allow Shots in System Settings → Privacy & Security → Screen Recording, then "
                + "restart Shots.", kind: .plain))
            page.append(.button(action: "open-settings", label: "Open System Settings", kind: .plain))
            page.append(.button(action: "restart", label: "Restart Shots", kind: .plain))
        }
        if shared {
            page.append(.note(text: "Screen Recording is shared with Samo: Shots couldn't start as its own app.",
                              kind: .error))
        }
        page.append(.section(title: "CAPTURE", about: nil))
        page.append(.button(action: "area", label: "Capture an area", kind: .plain))
        page.append(.button(action: "window", label: "Capture a window", kind: .plain))
        page.append(.button(action: "screen", label: "Capture the screen", kind: .plain))
        page.append(.section(title: "RECENT", about: recent.shots.isEmpty ? nil : "\(recent.shots.count)"))
        if recent.shots.isEmpty {
            page.append(.note(text: "Shots you save appear here.", kind: .plain))
        } else {
            page.append(.rows(recent.shots.prefix(Recent.keep).map {
                RowItem(lamp: .done, title: ($0.path as NSString).lastPathComponent)
            }))
        }
        return page
    }
}
