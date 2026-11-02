// The worker: an accessory app (no Dock icon, no menu bar of its own) that talks to Samo over stdin and stdout and
// does everything visible on the main thread. Its menu is never shown; it is there so ⌘C, ⌘V, ⌘A and ⌘Z reach a
// text mark being typed in the editor. It has no Quit: Shots ends when Samo says so.

import AppKit
import SamoKit
import ShotsCore

final class WorkerApp: NSObject, NSApplicationDelegate {
    private let shared: Bool
    private let executable: String
    private var app: ShotsApp?

    private init(shared: Bool, executable: String) {
        self.shared = shared
        self.executable = executable
    }

    static func run(shared: Bool, executable: String) -> Never {
        let app = NSApplication.shared
        let delegate = WorkerApp(shared: shared, executable: executable)
        app.delegate = delegate
        app.setActivationPolicy(.accessory)
        app.run()
        exit(0)
    }

    static func editMenu() -> NSMenu {
        let main = NSMenu()
        let appItem = NSMenuItem()
        appItem.submenu = NSMenu(title: "Shots")
        main.addItem(appItem)
        let edit = NSMenu(title: "Edit")
        let items: [(String, Selector, String, NSEvent.ModifierFlags)] = [
            ("Undo", Selector(("undo:")), "z", [.command]),
            ("Redo", Selector(("redo:")), "z", [.command, .shift]),
            ("Cut", #selector(NSText.cut(_:)), "x", [.command]),
            ("Copy", #selector(NSText.copy(_:)), "c", [.command]),
            ("Paste", #selector(NSText.paste(_:)), "v", [.command]),
            ("Select All", #selector(NSText.selectAll(_:)), "a", [.command]),
        ]
        for (title, action, key, flags) in items {
            let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
            item.keyEquivalentModifierMask = flags
            edit.addItem(item)
        }
        let editItem = NSMenuItem()
        editItem.submenu = edit
        main.addItem(editItem)
        return main
    }

    func applicationWillTerminate(_ notification: Notification) {
        app?.shutdown()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        Fonts.register(directory: Fonts.directory(forExecutable: URL(fileURLWithPath: executable)))
        NSApp.mainMenu = WorkerApp.editMenu()
        let dataDir = ProcessInfo.processInfo.environment["SAMO_EXT_DATA_DIR"].map { URL(fileURLWithPath: $0) }
            ?? FileManager.default.temporaryDirectory.appendingPathComponent("samo-shots")
        let app = ShotsApp(dataDir: dataDir, shared: shared)
        self.app = app
        app.start(out: LineWriter(handle: .standardOutput))
        guard let session = app.session else { return }
        LineReader.start(
            handle: .standardInput,
            onLine: { line in
                guard let msg = HostMsg.decode(line) else { return }
                DispatchQueue.main.async {
                    if !session.handle(msg) {
                        NSApp.terminate(nil)
                    }
                }
            },
            onEnd: {
                // Samo has gone: shortcuts and windows go with Shots.
                DispatchQueue.main.async { NSApp.terminate(nil) }
            }
        )
    }
}
