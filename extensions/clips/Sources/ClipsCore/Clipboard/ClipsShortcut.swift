// Clips' shortcut: Maccy's usual ⇧⌘C (KJ, ADR-005), kept in `data/settings.json`. It can be left empty (no
// shortcut), which is saved as empty rather than forgotten, so it doesn't come back as ⇧⌘C.

import Foundation
import SamoKit

public enum ClipsShortcut {
    /// C is key code 8 on an ANSI keyboard.
    public static let standard = Shortcut(keyCode: 8, modifiers: Shortcut.command | Shortcut.shift)
}

public struct ClipsSettings: Codable, Equatable {
    /// The shortcut that opens the list; `nil` for none.
    public var shortcut: Shortcut? = ClipsShortcut.standard
    /// Keep what you copy. Off, Clips doesn't look at the clipboard at all, until it is on again.
    public var watching = true

    public init() {}

    private enum CodingKeys: String, CodingKey {
        case shortcut, watching
    }

    /// A missing key is the default; an empty shortcut (`null`) is no shortcut.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        watching = ((try? c.decodeIfPresent(Bool.self, forKey: .watching)) ?? nil) ?? true
        guard c.contains(.shortcut) else { return }
        if (try? c.decodeNil(forKey: .shortcut)) == true {
            shortcut = nil
        } else {
            shortcut = (try? c.decode(Shortcut.self, forKey: .shortcut)) ?? ClipsShortcut.standard
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(shortcut, forKey: .shortcut)
        try c.encode(watching, forKey: .watching)
    }

    public static func load(dataDir: URL) -> ClipsSettings {
        guard let data = try? Data(contentsOf: dataDir.appendingPathComponent("settings.json")),
              let settings = try? JSONDecoder().decode(ClipsSettings.self, from: data)
        else { return ClipsSettings() }
        return settings
    }

    public func save(dataDir: URL) throws {
        try FileManager.default.createDirectory(at: dataDir, withIntermediateDirectories: true)
        try JSONEncoder().encode(self).write(to: dataDir.appendingPathComponent("settings.json"), options: .atomic)
    }
}
