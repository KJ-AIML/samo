// The one file in Shots that turns text into a URL (R16, checked by the network gate): links are handed to macOS,
// which opens them in the browser or System Settings. Shots itself never goes online.

import AppKit

public enum Links {
    /// System Settings → Privacy & Security → Screen Recording.
    public static func openScreenRecordingSettings() {
        if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture") {
            NSWorkspace.shared.open(url)
        }
    }
}
