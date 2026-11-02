// Pasting where you were (ADR-005 C3, R24). When the list opens, Clips remembers the app in front. A pick puts the
// item on the clipboard, closes the list, hands the keyboard back to that app, and presses ⌘V only once that app is
// in front again. Nothing presses ⌘V but a pick. Without Accessibility, or if that app never comes back, it stops at
// the clipboard and says so.

import Foundation

public final class PasteFlow {
    /// What the flow needs from the Mac; tests stand in for all of it.
    public struct Mac {
        /// The app in front, by bundle id or process id.
        public var front: () -> Int32?
        /// Asks `pid` to come forward.
        public var activate: (Int32) -> Void
        /// Accessibility is allowed for Clips.
        public var trusted: () -> Bool
        /// Presses ⌘V.
        public var pressPaste: () -> Void
        /// Runs `block` after `seconds` on the main thread.
        public var after: (TimeInterval, @escaping () -> Void) -> Void

        public init(front: @escaping () -> Int32?, activate: @escaping (Int32) -> Void, trusted: @escaping () -> Bool,
                    pressPaste: @escaping () -> Void, after: @escaping (TimeInterval, @escaping () -> Void) -> Void) {
            self.front = front
            self.activate = activate
            self.trusted = trusted
            self.pressPaste = pressPaste
            self.after = after
        }
    }

    public enum Outcome: Equatable {
        case pasted
        /// On the clipboard only: Accessibility isn't allowed, or the app never came back.
        case copiedOnly(String)
    }

    /// How long to wait for the app to be in front again.
    public static let patience: TimeInterval = 0.6
    static let step: TimeInterval = 0.02

    private let mac: Mac
    /// The app that was in front when the list opened.
    public private(set) var previous: Int32?

    public init(mac: Mac) {
        self.mac = mac
    }

    /// The list is opening: remember who to paste into.
    public func remember(_ pid: Int32?) {
        previous = pid
    }

    /// A pick: the item is already on the clipboard and the list closed. Paste into the remembered app.
    public func picked(done: @escaping (Outcome) -> Void) {
        guard let target = previous else {
            done(.copiedOnly("Copied: press ⌘V"))
            return
        }
        previous = nil
        guard mac.trusted() else {
            done(.copiedOnly("Copied: press ⌘V (allow Accessibility for Clips to paste for you)"))
            return
        }
        mac.activate(target)
        wait(for: target, left: PasteFlow.patience, done: done)
    }

    private func wait(for target: Int32, left: TimeInterval, done: @escaping (Outcome) -> Void) {
        if mac.front() == target {
            // A moment for the app's window to take the keyboard back.
            mac.after(0.05) {
                self.mac.pressPaste()
                done(.pasted)
            }
            return
        }
        guard left > 0 else {
            done(.copiedOnly("Copied: press ⌘V"))
            return
        }
        mac.after(PasteFlow.step) { self.wait(for: target, left: left - PasteFlow.step, done: done) }
    }

    /// The list closed without a pick.
    public func cancelled() {
        previous = nil
    }
}
