// Bringing an extension forward, so its window has the keyboard.

import AppKit

/// Bringing an extension's window forward. Extensions have no Dock icon, and macOS 14's polite `activate()` is refused on this Mac when
/// another app is in front, so a window would open behind it, without the keyboard. The older call still brings
/// it forward; it is reached through a protocol so its deprecation doesn't warn on every build.
public enum Activation {
    public static func bringForward() {
        (NSApplication.shared as ForcedActivation).activate(ignoringOtherApps: true)
    }
}

private protocol ForcedActivation {
    func activate(ignoringOtherApps flag: Bool)
}

extension NSApplication: ForcedActivation {}
