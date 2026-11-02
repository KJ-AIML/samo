// Shots' settings and its recent list, kept in its `data/` folder (`settings.json`, `recent.json`). Only file paths
// are kept, never images: shots live where you saved them.

import Foundation

/// A global shortcut, in Carbon's terms (what `RegisterEventHotKey` takes).
public struct Shortcut: Codable, Equatable, Hashable {
    public var keyCode: UInt32
    public var modifiers: UInt32

    public static let command: UInt32 = 0x0100
    public static let shift: UInt32 = 0x0200
    public static let option: UInt32 = 0x0800
    public static let control: UInt32 = 0x1000

    public init(keyCode: UInt32, modifiers: UInt32) {
        self.keyCode = keyCode
        self.modifiers = modifiers
    }

    /// Digits' and letters' virtual key codes on an ANSI keyboard.
    static let keyNames: [UInt32: String] = [
        0x12: "1", 0x13: "2", 0x14: "3", 0x15: "4", 0x17: "5", 0x16: "6", 0x1A: "7", 0x1C: "8", 0x19: "9", 0x1D: "0",
        0: "A", 11: "B", 8: "C", 2: "D", 14: "E", 3: "F", 5: "G", 4: "H", 34: "I", 38: "J", 40: "K", 37: "L",
        46: "M", 45: "N", 31: "O", 35: "P", 12: "Q", 15: "R", 1: "S", 17: "T", 32: "U", 9: "V", 13: "W", 7: "X",
        16: "Y", 6: "Z",
    ]

    public static let wholeScreen = Shortcut(keyCode: 0x12, modifiers: command | shift)
    public static let area = Shortcut(keyCode: 0x13, modifiers: command | shift)

    /// `⇧⌘2`, in Apple's order: ⌃ ⌥ ⇧ ⌘.
    public var display: String {
        var s = ""
        if modifiers & Shortcut.control != 0 { s += "⌃" }
        if modifiers & Shortcut.option != 0 { s += "⌥" }
        if modifiers & Shortcut.shift != 0 { s += "⇧" }
        if modifiers & Shortcut.command != 0 { s += "⌘" }
        return s + (Shortcut.keyNames[keyCode] ?? "key \(keyCode)")
    }

    /// A global shortcut needs ⌘, ⌃ or ⌥: with only ⇧, or none, it would take a key you type.
    public var canBeGlobal: Bool {
        modifiers & (Shortcut.command | Shortcut.control | Shortcut.option) != 0
    }
}

public struct Settings: Codable, Equatable {
    /// First, the shot is copied, saved, or both (R19: before any window opens).
    public enum After: String, Codable, CaseIterable {
        case copy, save, both
    }

    /// Then it opens in the editor, is pinned, or nothing more.
    public enum Then: String, Codable, CaseIterable {
        case edit, pin, nothing
    }

    public enum Format: String, Codable, CaseIterable {
        case png, jpeg
    }

    public var after: After = .both
    public var then: Then = .edit
    /// Where shots are saved; `~` is the home folder.
    public var folder: String = "~/Pictures/Shots"
    public var format: Format = .png
    /// Full Retina resolution, or 1x (half the pixels on a Retina display).
    public var retina: Bool = true
    public var sound: Bool = false
    /// By capture mode: `screen`, `area`, and, only if chosen, `text`, `colour`, `ruler`.
    public var shortcuts: [String: Shortcut] = ["screen": .wholeScreen, "area": .area]

    /// Everything a shortcut can start, in order: Shottr's two keys are set; the rest wait to be chosen, so Shots
    /// takes nothing more from the Mac (R18).
    public static let modes: [(key: String, label: String)] = [
        ("screen", "Whole screen"), ("area", "An area"), ("text", "Copy text"), ("colour", "Pick a colour"),
        ("ruler", "Measure"),
    ]

    public init() {}

    /// Field by field, so one value Shots doesn't know never resets the rest. Shots 0.1.0's `"after": "pin"`
    /// (pin, without copying or saving) becomes both, then pin.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let after = (try? c.decodeIfPresent(String.self, forKey: .after)) ?? nil
        if after == "pin" {
            self.after = .both
            then = .pin
        } else {
            self.after = after.flatMap(After.init(rawValue:)) ?? .both
            then = ((try? c.decodeIfPresent(Then.self, forKey: .then)) ?? nil) ?? .edit
        }
        folder = ((try? c.decodeIfPresent(String.self, forKey: .folder)) ?? nil) ?? folder
        format = ((try? c.decodeIfPresent(Format.self, forKey: .format)) ?? nil) ?? format
        retina = ((try? c.decodeIfPresent(Bool.self, forKey: .retina)) ?? nil) ?? retina
        sound = ((try? c.decodeIfPresent(Bool.self, forKey: .sound)) ?? nil) ?? sound
        shortcuts = ((try? c.decodeIfPresent([String: Shortcut].self, forKey: .shortcuts)) ?? nil) ?? shortcuts
    }

    public var folderURL: URL {
        URL(fileURLWithPath: Settings.expand(folder), isDirectory: true)
    }

    /// The home folder: `$HOME`, which Samo sets for its helpers, else the account's. (`expandingTildeInPath`
    /// reads the account's, so a Shots run under another home would save into the real one.)
    public static var home: String {
        if let home = ProcessInfo.processInfo.environment["HOME"], !home.isEmpty { return home }
        return NSHomeDirectory()
    }

    /// `~` and `~/…` as paths in `home`.
    public static func expand(_ path: String, home: String = Settings.home) -> String {
        if path == "~" { return home }
        if path.hasPrefix("~/") { return home + path.dropFirst() }
        return path
    }

    /// A path in `home` as `~/…`, the way the folder is shown and kept.
    public static func abbreviate(_ path: String, home: String = Settings.home) -> String {
        if path == home { return "~" }
        if path.hasPrefix(home + "/") { return "~" + path.dropFirst(home.count) }
        return path
    }

    public static func load(dataDir: URL) -> Settings {
        guard let data = try? Data(contentsOf: dataDir.appendingPathComponent("settings.json")),
              let settings = try? JSONDecoder().decode(Settings.self, from: data)
        else { return Settings() }
        return settings
    }

    public func save(dataDir: URL) throws {
        let data = try JSONEncoder().encode(self)
        try FileManager.default.createDirectory(at: dataDir, withIntermediateDirectories: true)
        try data.write(to: dataDir.appendingPathComponent("settings.json"), options: .atomic)
    }
}

/// A saved shot in the recent list.
public struct RecentShot: Codable, Equatable {
    public var path: String
    public var bytes: Int
    public var at: Date

    public init(path: String, bytes: Int, at: Date) {
        self.path = path
        self.bytes = bytes
        self.at = at
    }
}

/// The last 20 shots saved, newest first.
public struct Recent: Codable, Equatable {
    public static let keep = 20
    public private(set) var shots: [RecentShot] = []

    public init() {}

    public mutating func add(_ shot: RecentShot) {
        shots.removeAll { $0.path == shot.path }
        shots.insert(shot, at: 0)
        if shots.count > Recent.keep {
            shots.removeLast(shots.count - Recent.keep)
        }
    }

    /// Drops shots whose files are gone (deleted, or moved away).
    public mutating func forgetMissing(exists: (String) -> Bool = { FileManager.default.fileExists(atPath: $0) }) {
        shots.removeAll { !exists($0.path) }
    }

    /// How many were taken since `start` (today's count, for Samo's quick panel).
    public func count(since start: Date) -> Int {
        shots.filter { $0.at >= start }.count
    }

    public static func load(dataDir: URL) -> Recent {
        guard let data = try? Data(contentsOf: dataDir.appendingPathComponent("recent.json")),
              let recent = try? JSONDecoder().decode(Recent.self, from: data)
        else { return Recent() }
        return recent
    }

    public func save(dataDir: URL) throws {
        let data = try JSONEncoder().encode(self)
        try FileManager.default.createDirectory(at: dataDir, withIntermediateDirectories: true)
        try data.write(to: dataDir.appendingPathComponent("recent.json"), options: .atomic)
    }
}
