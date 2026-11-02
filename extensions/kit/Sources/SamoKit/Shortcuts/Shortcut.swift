// A global shortcut, in Carbon's terms, as Samo's extensions store, show and record them.

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
    public static let keyNames: [UInt32: String] = [
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

extension Shortcut {
    /// A shortcut from a key press (Cocoa's modifier flags), or `nil` if it would take a key you type: it needs ⌘, ⌃
    /// or ⌥.
    public static func recorded(keyCode: UInt16, cocoaModifiers: UInt) -> Shortcut? {
        let shortcut = Shortcut(keyCode: UInt32(keyCode), modifiers: Shortcut.carbonModifiers(cocoa: Int(cocoaModifiers)))
        return shortcut.canBeGlobal ? shortcut : nil
    }
}

extension Shortcut {
    /// NSEvent modifier flags (as `com.apple.symbolichotkeys` stores them) to Carbon's.
    public static func carbonModifiers(cocoa: Int) -> UInt32 {
        var m: UInt32 = 0
        if cocoa & 0x20000 != 0 { m |= Shortcut.shift }
        if cocoa & 0x40000 != 0 { m |= Shortcut.control }
        if cocoa & 0x80000 != 0 { m |= Shortcut.option }
        if cocoa & 0x100000 != 0 { m |= Shortcut.command }
        return m
    }
}
