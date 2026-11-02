// Samo's fonts: IBM Plex Mono from `share/fonts` beside the helper (the same files Samo bundles), Cabinet Grotesk and
// Switzer if this Mac has them, the system's otherwise, as Samo does. No SwiftPM resources: they'd point at the Mac
// that built Shots.

import AppKit
import CoreText

public enum Fonts {
    /// `<package>/share/fonts` for a helper at `<package>/bin/samo-shots`.
    public static func directory(forExecutable executable: URL) -> URL {
        executable.deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("share/fonts", isDirectory: true)
    }

    /// Registers every font file in `directory` for this process. Returns how many registered.
    @discardableResult
    public static func register(directory: URL) -> Int {
        let files = (try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil))?
            .filter { ["woff2", "woff", "ttf", "otf"].contains($0.pathExtension.lowercased()) } ?? []
        var registered = 0
        for file in files where CTFontManagerRegisterFontsForURL(file as CFURL, .process, nil) {
            registered += 1
        }
        return registered
    }

    /// Mono figures: IBM Plex Mono if registered, else the system's mono face.
    public static func mono(_ size: CGFloat, weight: NSFont.Weight = .regular) -> NSFont {
        let name = weight >= .medium ? "IBMPlexMono-Medium" : "IBMPlexMono-Regular"
        return NSFont(name: name, size: size) ?? .monospacedSystemFont(ofSize: size, weight: weight)
    }

    /// Body text: Switzer if installed, else the system font.
    public static func body(_ size: CGFloat, weight: NSFont.Weight = .regular) -> NSFont {
        NSFont(name: "Switzer", size: size) ?? .systemFont(ofSize: size, weight: weight)
    }

    /// Headings: Cabinet Grotesk if installed, else the system font, bold.
    public static func display(_ size: CGFloat) -> NSFont {
        NSFont(name: "Cabinet Grotesk", size: size) ?? .systemFont(ofSize: size, weight: .bold)
    }
}
