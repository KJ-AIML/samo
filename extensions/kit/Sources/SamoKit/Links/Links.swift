// The kit's one file that turns text into a URL (R16, checked by the network gate): System Settings' privacy panes,
// opened by macOS. Nothing here goes online.

import AppKit

public enum SystemSettings {
    /// The Privacy & Security panes an extension asks you to visit.
    public enum Pane: String {
        case screenRecording = "Privacy_ScreenCapture"
        case accessibility = "Privacy_Accessibility"
        case pasteFromOtherApps = "Privacy_Pasteboard"
    }

    public static func open(_ pane: Pane) {
        let address = "x-apple.systempreferences:com.apple.preference.security?" + pane.rawValue
        if let url = URL(string: address) {
            NSWorkspace.shared.open(url)
        }
    }
}
