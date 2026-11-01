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

    /// Digits' virtual key codes on an ANSI keyboard.
    static let digitKeys: [UInt32: String] = [
        0x12: "1", 0x13: "2", 0x14: "3", 0x15: "4", 0x17: "5", 0x16: "6", 0x1A: "7", 0x1C: "8", 0x19: "9", 0x1D: "0",
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
        return s + (Shortcut.digitKeys[keyCode] ?? "key \(keyCode)")
    }

    /// A global shortcut needs ⌘, ⌃ or ⌥: with only ⇧, or none, it would take a key you type.
    public var canBeGlobal: Bool {
        modifiers & (Shortcut.command | Shortcut.control | Shortcut.option) != 0
    }
}

public struct Settings: Codable, Equatable {
    public enum After: String, Codable, CaseIterable {
        case copy, save, both, pin
    }

    public enum Format: String, Codable, CaseIterable {
        case png, jpeg
    }

    public var after: After = .both
    /// Where shots are saved; `~` is the home folder.
    public var folder: String = "~/Pictures/Shots"
    public var format: Format = .png
    /// Full Retina resolution, or 1x (half the pixels on a Retina display).
    public var retina: Bool = true
    public var sound: Bool = false
    /// By capture mode: `screen`, `area`.
    public var shortcuts: [String: Shortcut] = ["screen": .wholeScreen, "area": .area]

    public init() {}

    public var folderURL: URL {
        URL(fileURLWithPath: (folder as NSString).expandingTildeInPath, isDirectory: true)
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
