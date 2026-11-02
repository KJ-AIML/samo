// Clips' page in Samo (ADR-005 C5), drawn by Samo with its own widgets: whether macOS lets Clips read and paste, the
// pinned and latest items with their buttons, the shortcut and whether it works, and what Clips keeps.

import Foundation
import SamoKit

/// Paste from Other Apps, as macOS reports it for Clips.
public enum PasteAccess: Equatable {
    case allowed
    /// macOS asks each time Clips reads (the default).
    case asks
    case denied
}

public struct PageState {
    public var paste: PasteAccess
    public var accessibility: Bool
    public var shared: Bool
    public var history: History
    /// macOS refused ⇧⌘C.
    public var refused: Bool
    /// A clipboard app running that may also take ⇧⌘C.
    public var clash: String?
    public var pausedUntil: Date?
    public var ignoreNext: Bool
    /// Clear was asked for once: the page asks to be sure.
    public var confirmClear: Bool
    /// An app's name from its bundle id.
    public var appName: (String) -> String?
    public var now: Date

    public init(
        paste: PasteAccess, accessibility: Bool, shared: Bool, history: History, refused: Bool = false,
        clash: String? = nil, pausedUntil: Date? = nil, ignoreNext: Bool = false, confirmClear: Bool = false,
        appName: @escaping (String) -> String? = { _ in nil }, now: Date = Date()
    ) {
        self.paste = paste
        self.accessibility = accessibility
        self.shared = shared
        self.history = history
        self.refused = refused
        self.clash = clash
        self.pausedUntil = pausedUntil
        self.ignoreNext = ignoreNext
        self.confirmClear = confirmClear
        self.appName = appName
        self.now = now
    }
}

public enum ClipsPage {
    /// What the first log says if the worker couldn't be made its own app.
    public static let sharedNote = "Clips couldn't start as its own app, so Paste from Other Apps and Accessibility "
        + "would be Samo's, and every extension's"

    /// How many of the latest items the page lists.
    public static let latest = 20

    /// Clipboard apps that may also take ⇧⌘C, by bundle id prefix.
    public static let clipboardApps: [(bundlePrefix: String, name: String)] = [
        ("org.p0deje.Maccy", "Maccy"),
        ("com.wiheads.paste", "Paste"),
        ("com.raycast.macos", "Raycast"),
        ("com.runningwithcrayons.Alfred", "Alfred"),
    ]

    /// The first known clipboard app among the running apps' bundle ids.
    public static func runningClipboardApp(bundleIds: [String]) -> String? {
        clipboardApps.first { app in bundleIds.contains { $0.hasPrefix(app.bundlePrefix) } }?.name
    }

    /// The page before anything is known.
    public static func starting() -> [Widget] {
        [
            .section(title: "CLIPBOARD", about: nil),
            .note(text: "Clips keeps what you copy on this Mac. Press ⇧⌘C to find it.", kind: .plain),
        ]
    }

    public static func build(_ s: PageState) -> [Widget] {
        var page: [Widget] = [.section(title: "PERMISSIONS", about: nil)]
        let pasteRow: RowItem
        switch s.paste {
        case .allowed:
            pasteRow = RowItem(lamp: .done, title: "Paste from Other Apps", detail: "Allowed: Clips sees each copy")
        case .asks:
            pasteRow = RowItem(lamp: .needsYou, title: "Paste from Other Apps",
                               detail: "macOS asks each time: choose Always Allow for Clips in System Settings")
        case .denied:
            pasteRow = RowItem(lamp: .error, title: "Paste from Other Apps",
                               detail: "Not allowed: Clips can't see what you copy")
        }
        let axRow = s.accessibility
            ? RowItem(lamp: .done, title: "Accessibility", detail: "Allowed: picking an item pastes it for you")
            : RowItem(lamp: .needsYou, title: "Accessibility", detail: "Not yet: picking copies, then you press ⌘V")
        page.append(.rows([pasteRow, axRow]))
        if s.paste != .allowed {
            page.append(.button(action: "open-paste-settings", label: "Open Paste from Other Apps", kind: .plain))
        }
        if !s.accessibility {
            page.append(.button(action: "allow-accessibility", label: "Allow Accessibility", kind: .plain))
        }
        if s.shared {
            page.append(.note(text: sharedNote, kind: .error))
        }

        let pins = s.history.pinned
        if !pins.isEmpty {
            page.append(.section(title: "PINNED", about: "\(pins.count)"))
            page.append(.rows(pins.map { row($0, s) }))
        }
        let recent = s.history.recent
        page.append(.section(title: "RECENT", about: recent.isEmpty ? nil : "\(recent.count) kept"))
        if recent.isEmpty {
            page.append(.note(text: "Nothing copied yet. Copy something, then press ⇧⌘C.", kind: .plain))
        } else {
            page.append(.rows(recent.prefix(latest).map { row($0, s) }))
        }

        page.append(.section(title: "SHORTCUT", about: nil))
        let keys = ClipsShortcut.standard.display
        let shortcut: RowItem
        if s.refused {
            shortcut = RowItem(lamp: .error, title: "Open the list", detail: "macOS refused \(keys)", trailing: keys)
        } else if let clash = s.clash {
            shortcut = RowItem(lamp: .needsYou, title: "Open the list",
                               detail: "May clash with \(clash), which is running", trailing: keys)
        } else {
            shortcut = RowItem(lamp: .done, title: "Open the list", detail: "Ready", trailing: keys)
        }
        page.append(.rows([shortcut]))
        page.append(.note(text: "In the list: type to search, Return or ⌘1–⌘9 to paste, ⌥P to pin, ⌥⌫ to delete.",
                          kind: .plain))

        page.append(.section(title: "WHAT CLIPS KEEPS", about: nil))
        page.append(.choice(key: "limit", label: "Keep", options: [
            ChoiceOption(id: "50", label: "50"), ChoiceOption(id: "200", label: "200"),
            ChoiceOption(id: "500", label: "500"),
        ], selected: "\(s.history.limit)"))
        if let until = s.pausedUntil, s.now < until {
            page.append(.note(text: "Paused until \(time(until)): nothing copied is kept.", kind: .plain))
            page.append(.button(action: "resume", label: "Resume now", kind: .plain))
        } else {
            page.append(.button(action: "pause", label: "Pause for 15 minutes", kind: .plain))
        }
        if s.ignoreNext {
            page.append(.note(text: "The next copy won't be kept.", kind: .plain))
        } else {
            page.append(.button(action: "ignore-next", label: "Ignore the next copy", kind: .plain))
        }
        if s.confirmClear {
            page.append(.note(text: "Clear \(recent.count) items? Pins stay. This can't be undone.", kind: .plain))
            page.append(.button(action: "clear-confirm", label: "Clear \(recent.count) items", kind: .plain))
            page.append(.button(action: "clear-cancel", label: "Keep them", kind: .link))
        } else if !recent.isEmpty {
            page.append(.button(action: "clear", label: "Clear history…", kind: .link))
        }
        page.append(.note(
            text: "Never kept: what password managers mark as secret, and copies made in 1Password, Bitwarden, "
                + "Keychain Access or Passwords. Kept only on this Mac; uninstalling deletes it.",
            kind: .plain
        ))
        return page
    }

    static func time(_ date: Date) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "HH:mm"
        return f.string(from: date)
    }

    /// `Text · 2 copies · Safari · 14:05`.
    static func row(_ item: Item, _ s: PageState) -> RowItem {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = Calendar.current.isDate(item.last, inSameDayAs: s.now) ? "HH:mm" : "MMM d, HH:mm"
        var parts: [String] = [
            ["text": "Text", "rich": "Rich text", "image": "Image", "files": "Files"][item.kind.rawValue] ?? "",
        ]
        if item.kind == .image, item.bytes > 0 {
            parts.append(ByteCountFormatter.string(fromByteCount: Int64(item.bytes), countStyle: .file))
        }
        if item.copies > 1 { parts.append("\(item.copies) copies") }
        if let app = item.source.flatMap(s.appName) { parts.append(app) }
        parts.append(f.string(from: item.last))
        let title = item.title.count > 80 ? String(item.title.prefix(79)) + "…" : item.title
        return RowItem(
            id: item.id.uuidString, lamp: item.pinned ? .done : .off, title: title.isEmpty ? " " : title,
            detail: parts.joined(separator: " · "),
            actions: [RowAction(id: "copy", label: "Copy"),
                      RowAction(id: item.pinned ? "unpin" : "pin", label: item.pinned ? "Unpin" : "Pin"),
                      RowAction(id: "delete", label: "Delete")]
        )
    }

    /// The quick panel's line in Samo.
    public static func badge(_ history: History) -> HelperMsg {
        let n = history.items.count
        return .badge(count: 0, summary: n == 0 ? "Clips" : "Clips · \(n) \(n == 1 ? "item" : "items")")
    }
}

extension History {
    /// A new limit (50, 200 or 500): returns what fell off.
    public mutating func setLimit(_ value: Int) -> [Item] {
        limit = value
        return trim()
    }
}
