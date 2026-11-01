// Clashes, said out loud (R18). macOS lets two apps hold the same global shortcut, fires both, and never says who
// holds it; so Shots looks for the usual suspects itself: other screenshot apps that are running, and macOS's own
// screenshot shortcuts.

import Foundation

public enum Conflicts {
    /// Screenshot apps that take ⌘⇧ shortcuts, by bundle id prefix and name.
    public static let screenshotApps: [(bundlePrefix: String, name: String)] = [
        ("cc.ffitch.shottr", "Shottr"),
        ("pl.maketheweb.cleanshotx", "CleanShot X"),
        ("com.TechSmith.Snagit", "Snagit"),
        ("com.xnapper", "Xnapper"),
    ]

    /// The names of known screenshot apps among the running apps' bundle ids.
    public static func runningScreenshotApps(bundleIds: [String]) -> [String] {
        var names: [String] = []
        for app in screenshotApps where bundleIds.contains(where: { $0.hasPrefix(app.bundlePrefix) }) {
            if !names.contains(app.name) { names.append(app.name) }
        }
        return names
    }

    /// macOS's own screenshot shortcuts (⌘⇧3, ⌃⌘⇧3, ⌘⇧4, ⌃⌘⇧4, ⌘⇧5) that are on, from `com.apple.symbolichotkeys`.
    /// Missing entries are macOS's defaults, which are on.
    public static func macScreenshotShortcuts(symbolicHotKeys: [String: Any]?) -> [Shortcut] {
        let defaults: [String: Shortcut] = [
            "28": Shortcut(keyCode: 0x14, modifiers: Shortcut.command | Shortcut.shift),
            "29": Shortcut(keyCode: 0x14, modifiers: Shortcut.command | Shortcut.shift | Shortcut.control),
            "30": Shortcut(keyCode: 0x15, modifiers: Shortcut.command | Shortcut.shift),
            "31": Shortcut(keyCode: 0x15, modifiers: Shortcut.command | Shortcut.shift | Shortcut.control),
            "184": Shortcut(keyCode: 0x17, modifiers: Shortcut.command | Shortcut.shift),
        ]
        var found: [Shortcut] = []
        for (id, fallback) in defaults.sorted(by: { $0.key < $1.key }) {
            guard let entry = symbolicHotKeys?[id] as? [String: Any] else {
                found.append(fallback)
                continue
            }
            guard (entry["enabled"] as? Bool) ?? (entry["enabled"] as? Int).map({ $0 != 0 }) ?? true else {
                continue
            }
            let parameters = (entry["value"] as? [String: Any])?["parameters"] as? [Int]
            if let p = parameters, p.count == 3, let code = UInt32(exactly: p[1]) {
                found.append(Shortcut(keyCode: code, modifiers: carbonModifiers(cocoa: p[2])))
            } else {
                found.append(fallback)
            }
        }
        return found
    }

    /// NSEvent modifier flags (as `com.apple.symbolichotkeys` stores them) to Carbon's.
    static func carbonModifiers(cocoa: Int) -> UInt32 {
        var m: UInt32 = 0
        if cocoa & 0x20000 != 0 { m |= Shortcut.shift }
        if cocoa & 0x40000 != 0 { m |= Shortcut.control }
        if cocoa & 0x80000 != 0 { m |= Shortcut.option }
        if cocoa & 0x100000 != 0 { m |= Shortcut.command }
        return m
    }

    /// For each of Shots' shortcuts, what it may clash with, in words; empty when nothing is known to.
    public static func clashes(ours: [String: Shortcut], macOS: [Shortcut], apps: [String]) -> [String: String] {
        var said: [String: String] = [:]
        for (mode, shortcut) in ours {
            if macOS.contains(shortcut) {
                said[mode] = "\(shortcut.display) is also macOS's screenshot shortcut"
            } else if let app = apps.first {
                said[mode] = "May clash with \(app), which is running"
            }
        }
        return said
    }
}
