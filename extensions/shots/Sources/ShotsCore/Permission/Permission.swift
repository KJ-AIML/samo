// Screen Recording (ADR-004). macOS applies a new grant only to a process started after it, so once Shots has
// asked, a still-missing grant may just need Shots to restart.

import CoreGraphics
import SamoKit

public enum ScreenAccess: Equatable {
    case allowed
    /// Not asked yet in this run.
    case notYet
    /// Asked, and not (yet) seen as allowed: allow Shots in System Settings, then restart it.
    case askedRestart
}

public final class Permission {
    private let preflight: () -> Bool
    private let ask: () -> Bool
    public private(set) var asked = false

    public init(
        preflight: @escaping () -> Bool = CGPreflightScreenCaptureAccess,
        ask: @escaping () -> Bool = CGRequestScreenCaptureAccess
    ) {
        self.preflight = preflight
        self.ask = ask
    }

    public var state: ScreenAccess {
        if preflight() { return .allowed }
        return asked ? .askedRestart : .notYet
    }

    /// Asks macOS (its prompt appears once; after that it points to System Settings). `true` if allowed now.
    @discardableResult
    public func request() -> Bool {
        asked = true
        return ask()
    }
}
