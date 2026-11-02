// Clips, put together: its session with Samo and its page. Everything here runs on the main thread.

import AppKit
import ClipsCore
import SamoKit

final class ClipsApp {
    let dataDir: URL
    let shared: Bool
    private(set) var session: Session?
    private var stopped = false

    init(dataDir: URL, shared: Bool) {
        self.dataDir = dataDir
        self.shared = shared
    }

    func start(out: Output) {
        let session = Session(out: out, shared: shared, sharedNote: ClipsPage.sharedNote, handlers: Session.Handlers(
            page: { ClipsPage.starting() },
            shutdown: { [unowned self] in self.shutdown() }
        ))
        self.session = session
        session.start()
    }

    /// Off, update, uninstall, or Samo quitting.
    func shutdown() {
        guard !stopped else { return }
        stopped = true
    }
}
