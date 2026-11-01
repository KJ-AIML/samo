// Shots' page in Samo (ADR-004 S6), drawn by Samo with its own widgets: whether Screen Recording is allowed, the
// capture buttons, recent shots with their buttons, the shortcuts and whether each works, and what happens after a
// capture.

import Foundation

public struct PageState {
    public var access: ScreenAccess
    public var shared: Bool
    public var settings: Settings
    public var recent: Recent
    /// Capture modes whose shortcut macOS refused.
    public var refused: Set<String>
    /// Capture modes whose shortcut may clash, with the words why.
    public var clashes: [String: String]
    /// The folder is inside iCloud Drive: shots saved there are uploaded.
    public var folderInICloud: Bool
    public var now: Date

    public init(
        access: ScreenAccess, shared: Bool, settings: Settings, recent: Recent, refused: Set<String> = [],
        clashes: [String: String] = [:], folderInICloud: Bool = false, now: Date = Date()
    ) {
        self.access = access
        self.shared = shared
        self.settings = settings
        self.recent = recent
        self.refused = refused
        self.clashes = clashes
        self.folderInICloud = folderInICloud
        self.now = now
    }
}

public enum Page {
    static let modes: [(key: String, label: String)] = [("screen", "Whole screen"), ("area", "An area")]

    public static func build(_ s: PageState) -> [Widget] {
        var page: [Widget] = [.section(title: "SCREEN RECORDING", about: nil)]
        switch s.access {
        case .allowed:
            page.append(.note(text: "Allowed for Shots. Nothing it captures leaves your Mac.", kind: .plain))
        case .notYet:
            page.append(.note(
                text: "Shots needs Screen Recording to capture what's on screen. macOS asks once, for Shots; nothing "
                    + "it captures leaves your Mac.",
                kind: .plain
            ))
            page.append(.button(action: "allow", label: "Allow Screen Recording", kind: .plain))
        case .askedRestart:
            page.append(.note(
                text: "Allow Shots in System Settings → Privacy & Security → Screen Recording, then restart it.",
                kind: .plain
            ))
            page.append(.button(action: "open-settings", label: "Open System Settings", kind: .plain))
            page.append(.button(action: "restart", label: "Restart Shots", kind: .plain))
        }
        if s.shared {
            page.append(.note(
                text: "Screen Recording is shared with Samo: Shots couldn't start as its own app, so allowing it "
                    + "lets Samo and every extension see the screen.",
                kind: .error
            ))
        }

        page.append(.section(title: "CAPTURE", about: nil))
        page.append(.button(action: "area", label: "Capture an area", kind: .plain))
        page.append(.button(action: "window", label: "Capture a window", kind: .plain))
        page.append(.button(action: "screen", label: "Capture the screen", kind: .plain))

        let today = s.recent.count(since: Calendar.current.startOfDay(for: s.now))
        page.append(.section(title: "RECENT", about: today > 0 ? "\(today) today" : nil))
        if s.recent.shots.isEmpty {
            page.append(.note(text: "Shots you save appear here.", kind: .plain))
        } else {
            page.append(.rows(s.recent.shots.map { recentRow($0, now: s.now) }))
        }

        page.append(.section(title: "SHORTCUTS", about: nil))
        page.append(.rows(modes.map { mode in
            let shortcut = s.settings.shortcuts[mode.key]
            let keys = shortcut?.display ?? "none"
            if s.refused.contains(mode.key) {
                return RowItem(lamp: .error, title: mode.label, detail: "macOS refused \(keys); choose another",
                               trailing: keys)
            }
            if let clash = s.clashes[mode.key] {
                return RowItem(lamp: .needsYou, title: mode.label, detail: clash, trailing: keys)
            }
            return RowItem(lamp: shortcut == nil ? .off : .done, title: mode.label,
                           detail: shortcut == nil ? "No shortcut" : "Ready", trailing: keys)
        }))
        page.append(.note(text: "Space in the overlay switches to windows; Esc cancels.", kind: .plain))
        page.append(.button(action: "change-shortcuts", label: "Change shortcuts…", kind: .link))

        page.append(.section(title: "AFTER A CAPTURE", about: nil))
        page.append(.choice(key: "after", label: "Then", options: [
            ChoiceOption(id: "copy", label: "Copy"), ChoiceOption(id: "save", label: "Save"),
            ChoiceOption(id: "both", label: "Both"), ChoiceOption(id: "pin", label: "Pin"),
        ], selected: s.settings.after.rawValue))
        page.append(.choice(key: "format", label: "Saved as", options: [
            ChoiceOption(id: "png", label: "PNG"), ChoiceOption(id: "jpeg", label: "JPEG"),
        ], selected: s.settings.format.rawValue))
        page.append(.toggle(key: "retina", label: "Full Retina resolution", on: s.settings.retina,
                            note: "Off saves half the pixels on a Retina display"))
        page.append(.toggle(key: "sound", label: "Shutter sound", on: s.settings.sound, note: nil))
        page.append(.note(text: "Saved to \(s.settings.folder)", kind: .plain))
        if s.folderInICloud {
            page.append(.note(text: "That folder is in iCloud Drive: shots saved there are uploaded.", kind: .error))
        }
        page.append(.button(action: "change-folder", label: "Change folder…", kind: .link))
        return page
    }

    static func recentRow(_ shot: RecentShot, now: Date) -> RowItem {
        let time = DateFormatter()
        time.locale = Locale(identifier: "en_US_POSIX")
        time.dateFormat = Calendar.current.isDate(shot.at, inSameDayAs: now) ? "HH:mm" : "MMM d, HH:mm"
        let size = shot.bytes > 0
            ? ByteCountFormatter.string(fromByteCount: Int64(shot.bytes), countStyle: .file) + " · " : ""
        return RowItem(
            id: shot.path, lamp: .done, title: (shot.path as NSString).lastPathComponent,
            detail: size + time.string(from: shot.at),
            actions: [RowAction(id: "copy", label: "Copy"), RowAction(id: "reveal", label: "Reveal"),
                      RowAction(id: "pin", label: "Pin")]
        )
    }

    /// The quick panel's line in Samo: nothing waits for you, so no count.
    public static func badge(_ recent: Recent, now: Date = Date()) -> HelperMsg {
        let today = recent.count(since: Calendar.current.startOfDay(for: now))
        return .badge(count: 0, summary: today > 0 ? "Shots · \(today) today" : "Shots")
    }

    /// Whether `folder` is inside iCloud Drive (or the Desktop and Documents while iCloud syncs them).
    public static func inICloud(_ folder: URL, home: URL, desktopSynced: Bool) -> Bool {
        let path = folder.standardizedFileURL.path
        if path.hasPrefix(home.appendingPathComponent("Library/Mobile Documents").path) { return true }
        guard desktopSynced else { return false }
        return ["Desktop", "Documents"].contains { path.hasPrefix(home.appendingPathComponent($0).path) }
    }
}

extension Settings {
    /// A switch from Samo's page. `false` for a key Shots doesn't have.
    @discardableResult
    public mutating func apply(setting key: String, _ value: Bool) -> Bool {
        switch key {
        case "sound": sound = value
        case "retina": retina = value
        default: return false
        }
        return true
    }

    /// A choice from Samo's page. `false` for a key or value Shots doesn't have.
    @discardableResult
    public mutating func apply(choice key: String, _ value: String) -> Bool {
        switch key {
        case "after":
            guard let after = After(rawValue: value) else { return false }
            self.after = after
        case "format":
            guard let format = Format(rawValue: value) else { return false }
            self.format = format
        default:
            return false
        }
        return true
    }
}

extension Shortcut {
    /// A shortcut from a key press (Cocoa's modifier flags), or `nil` if it would take a key you type: it needs ⌘, ⌃
    /// or ⌥.
    public static func recorded(keyCode: UInt16, cocoaModifiers: UInt) -> Shortcut? {
        let shortcut = Shortcut(keyCode: UInt32(keyCode), modifiers: Conflicts.carbonModifiers(cocoa: Int(cocoaModifiers)))
        return shortcut.canBeGlobal ? shortcut : nil
    }
}
