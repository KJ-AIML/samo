// The worker: an accessory app (no Dock icon, no menu bar of its own) that talks to Samo over stdin and stdout and
// does everything visible on the main thread.

import AppKit
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

    func applicationDidFinishLaunching(_ notification: Notification) {
        Fonts.register(directory: Fonts.directory(forExecutable: URL(fileURLWithPath: executable)))
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
