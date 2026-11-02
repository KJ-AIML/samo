// The worker: an accessory app (no Dock icon, no menu bar of its own) that talks to Samo over stdin and stdout and
// does everything visible on the main thread. Its menu is never shown; it is there so ⌘C, ⌘V, ⌘A and ⌘X reach the
// list's search field. It has no Quit: Clips ends when Samo says so.

import AppKit
import ClipsCore
import SamoKit

final class WorkerApp: NSObject, NSApplicationDelegate {
    private let shared: Bool
    private let executable: String
    private var app: ClipsApp?

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
        appItem.submenu = NSMenu(title: "Clips")
        main.addItem(appItem)
        let edit = NSMenu(title: "Edit")
        let items: [(String, Selector, String)] = [
            ("Cut", #selector(NSText.cut(_:)), "x"),
            ("Copy", #selector(NSText.copy(_:)), "c"),
            ("Paste", #selector(NSText.paste(_:)), "v"),
            ("Select All", #selector(NSText.selectAll(_:)), "a"),
        ]
        for (title, action, key) in items {
            edit.addItem(NSMenuItem(title: title, action: action, keyEquivalent: key))
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
            ?? FileManager.default.temporaryDirectory.appendingPathComponent("samo-clips")
        let app = ClipsApp(dataDir: dataDir, shared: shared)
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
                // Samo has gone: the shortcut and the list go with Clips.
                DispatchQueue.main.async { NSApp.terminate(nil) }
            }
        )
    }
}
