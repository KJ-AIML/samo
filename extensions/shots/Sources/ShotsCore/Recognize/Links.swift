// The one file in Shots that turns text into a URL (R16, checked by the network gate): links are handed to macOS,
// which opens them in the browser or System Settings. Shots itself never goes online. Only web links found in a
// shot can be opened, and only on a click; anything else (`javascript:`, `file:`, an app's own scheme) can only be
// copied.

import AppKit

public enum Links {
    /// System Settings → Privacy & Security → Screen Recording.
    public static func openScreenRecordingSettings() {
        if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture") {
            NSWorkspace.shared.open(url)
        }
    }

    /// `text` as a web link (`http` or `https`, with a host), or `nil`.
    public static func web(_ text: String) -> URL? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.contains(where: \.isWhitespace),
              let url = URL(string: trimmed),
              let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https",
              let host = url.host, !host.isEmpty
        else { return nil }
        return url
    }

    /// Opens a web link in the browser, after a click. `false` for anything that isn't one.
    @discardableResult
    public static func open(_ text: String) -> Bool {
        guard let url = web(text) else { return false }
        return NSWorkspace.shared.open(url)
    }
}
