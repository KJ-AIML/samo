// Watching the clipboard (ADR-005 C1, R21). macOS has no "something was copied" signal, so the watcher is asked
// twice a second; each time it reads one counter. Only when the counter changed does it read the kinds of thing on
// the clipboard, and only when those pass the secret checks does it read the contents (R22).

import Foundation

/// What was copied, as the watcher read it.
public struct Capture: Equatable {
    public var kind: Item.Kind
    /// Plain text, or the files' paths, one a line.
    public var text: String
    /// Rich text or an image, as copied.
    public var data: Data?
    public var dataType: String?
    public var files: [String]
    /// The app in front when it was noticed.
    public var source: String?

    public init(kind: Item.Kind, text: String, data: Data? = nil, dataType: String? = nil, files: [String] = [],
                source: String? = nil) {
        self.kind = kind
        self.text = text
        self.data = data
        self.dataType = dataType
        self.files = files
        self.source = source
    }
}

public final class Watcher {
    /// How often to look: twice a second, as Maccy does.
    public static let interval: TimeInterval = 0.5

    private let pasteboard: Pasteboard
    private let frontmost: () -> String?
    private var lastCount: Int
    /// Counts Clips itself made by pasting: not new copies.
    private var ownCounts: Set<Int> = []
    public var ignoredApps: Set<String>
    /// Ignore the next copy (once).
    public var ignoreNext = false
    /// Keep nothing until then.
    public var pausedUntil: Date?

    public init(pasteboard: Pasteboard, frontmost: @escaping () -> String?,
                ignoredApps: Set<String> = Set(Filters.defaultIgnoredApps.map(\.bundleId))) {
        self.pasteboard = pasteboard
        self.frontmost = frontmost
        self.ignoredApps = ignoredApps
        lastCount = pasteboard.changeCount
    }

    /// Whatever is on the clipboard now isn't new (watching starts or starts again).
    public func skipCurrent() {
        lastCount = pasteboard.changeCount
    }

    /// Clips put this on the clipboard itself (to paste it).
    public func own(_ count: Int) {
        ownCounts.insert(count)
    }

    public func paused(at now: Date) -> Bool {
        pausedUntil.map { now < $0 } ?? false
    }

    /// One look: a capture if something new, allowed and readable was copied.
    public func tick(now: Date = Date()) -> Capture? {
        let count = pasteboard.changeCount
        guard count != lastCount else { return nil }
        lastCount = count
        if ownCounts.remove(count) != nil { return nil }
        if paused(at: now) { return nil }
        if ignoreNext {
            ignoreNext = false
            return nil
        }
        let types = pasteboard.types
        let app = frontmost()
        guard Filters.allows(types: types, app: app, ignoredApps: ignoredApps) else { return nil }
        return read(types: types, source: app)
    }

    /// The contents, read only now: files, an image, rich text with its plain text, or plain text.
    private func read(types: [String], source: String?) -> Capture? {
        if types.contains(ClipType.fileURL) {
            let files = pasteboard.files()
            if !files.isEmpty {
                return Capture(kind: .files, text: files.joined(separator: "\n"), files: files, source: source)
            }
        }
        for imageType in [ClipType.png, ClipType.tiff] where types.contains(imageType) {
            if let data = pasteboard.data(forType: imageType), !data.isEmpty {
                return Capture(kind: .image, text: "", data: data, dataType: imageType, source: source)
            }
        }
        let text = types.contains(ClipType.text) ? pasteboard.string(forType: ClipType.text) : nil
        for richType in [ClipType.rtf, ClipType.html] where types.contains(richType) {
            if let data = pasteboard.data(forType: richType), !data.isEmpty, let text, !text.isEmpty {
                return Capture(kind: .rich, text: text, data: data, dataType: richType, source: source)
            }
        }
        if let text, !text.isEmpty {
            return Capture(kind: .text, text: text, source: source)
        }
        return nil
    }
}
