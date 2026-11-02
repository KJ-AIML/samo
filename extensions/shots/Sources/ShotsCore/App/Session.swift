// One conversation with Samo: `hello` first, then the page, then whatever Samo asks. Shots' features plug in
// through `handlers`; the session itself knows only the protocol.

import Foundation

public final class Session {
    public struct Handlers {
        public var page: () -> [Widget]
        public var setting: (String, Bool) -> Void
        public var action: (String, String?) -> Void
        public var choice: (String, String) -> Void
        public var shutdown: () -> Void

        public init(
            page: @escaping () -> [Widget],
            setting: @escaping (String, Bool) -> Void = { _, _ in },
            action: @escaping (String, String?) -> Void = { _, _ in },
            choice: @escaping (String, String) -> Void = { _, _ in },
            shutdown: @escaping () -> Void = {}
        ) {
            self.page = page
            self.setting = setting
            self.action = action
            self.choice = choice
            self.shutdown = shutdown
        }
    }

    public let out: Output
    /// The worker couldn't be made its own app, so Screen Recording is Samo's (and so every extension's).
    public let shared: Bool
    public private(set) var dataDir: String = ""
    public private(set) var pageVisible = false
    private let handlers: Handlers

    public init(out: Output, shared: Bool, handlers: Handlers) {
        self.out = out
        self.shared = shared
        self.handlers = handlers
    }

    public func start() {
        out.send(.hello)
        if shared {
            out.send(.log(
                event: "permission",
                detail: "Shots couldn't start as its own app, so Screen Recording is shared with Samo"
            ))
        }
        refresh()
    }

    /// Sends the page again.
    public func refresh() {
        out.send(.view(handlers.page()))
    }

    /// Handles one message; `false` once Shots should quit.
    @discardableResult
    public func handle(_ msg: HostMsg) -> Bool {
        switch msg {
        case let .hello(_, _, dataDir):
            self.dataDir = dataDir
        case let .setting(key, value):
            handlers.setting(key, value)
            refresh()
        case let .action(id, row):
            handlers.action(id, row)
            refresh()
        case let .choice(key, value):
            handlers.choice(key, value)
            refresh()
        case let .visible(page):
            pageVisible = page
        case .shutdown:
            handlers.shutdown()
            return false
        }
        return true
    }
}
