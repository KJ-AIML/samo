// Clips' shortcut: Maccy's usual ⇧⌘C (KJ, ADR-005).

import SamoKit

public enum ClipsShortcut {
    /// C is key code 8 on an ANSI keyboard.
    public static let standard = Shortcut(keyCode: 8, modifiers: Shortcut.command | Shortcut.shift)
}
