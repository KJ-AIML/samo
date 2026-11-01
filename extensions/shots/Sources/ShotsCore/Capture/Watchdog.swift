// Waiting on macOS for the screen, with a limit. If ScreenCaptureKit never answers, Shots must not stay busy
// (every shortcut would be ignored until it restarted): after the limit it gives up and says so, and an answer that
// arrives later is ignored.

import Foundation

public final class Watchdog {
    public static let limit: TimeInterval = 8

    private var current = 0
    private var waiting = false

    public init() {}

    /// A wait begins; its token.
    public func begin() -> Int {
        current += 1
        waiting = true
        return current
    }

    /// The answer for `token` arrived. `false` if it is too late (the wait was given up, or another began): ignore it.
    public func answered(_ token: Int) -> Bool {
        guard token == current, waiting else { return false }
        waiting = false
        return true
    }

    /// The limit passed for `token`. `true` if it was still waiting, so the capture should give up now.
    public func expired(_ token: Int) -> Bool {
        guard token == current, waiting else { return false }
        waiting = false
        current += 1
        return true
    }

    public var isWaiting: Bool { waiting }
}
