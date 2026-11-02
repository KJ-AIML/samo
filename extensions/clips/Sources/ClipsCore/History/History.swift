// The history (ADR-005 C1): newest first, a repeat moving to the top and counting once more, pins kept whatever the
// limit. Only what's needed to list and search is in memory; rich text and images are files (R21).

import CryptoKit
import Foundation

public struct Item: Codable, Equatable, Identifiable {
    public enum Kind: String, Codable {
        case text, rich, image, files
    }

    public var id: UUID
    public var kind: Kind
    /// Plain text (its first 64 KB if longer: the rest is in `textFile`), or files' paths, one a line.
    public var text: String
    /// The whole text, when it is longer than 64 KB.
    public var textFile: String?
    /// Rich text or an image, in `items/`.
    public var dataFile: String?
    public var dataType: String?
    public var files: [String]
    public var bytes: Int
    public var first: Date
    public var last: Date
    public var copies: Int
    public var pinned: Bool
    /// The app it was copied in.
    public var source: String?
    /// What makes two copies the same.
    public var fingerprint: String

    /// The first line, as a list row shows it.
    public var title: String {
        switch kind {
        case .image: return "Image"
        case .files:
            let names = files.map { ($0 as NSString).lastPathComponent }
            return names.count == 1 ? names[0] : "\(names.count) files: " + names.joined(separator: ", ")
        case .text, .rich:
            let line = text.split(whereSeparator: \.isNewline).first.map(String.init) ?? text
            return line.trimmingCharacters(in: .whitespaces)
        }
    }

    static func fingerprint(_ capture: Capture) -> String {
        var hash = SHA256()
        hash.update(data: Data(capture.kind.rawValue.utf8))
        hash.update(data: Data(capture.text.utf8))
        if let data = capture.data { hash.update(data: data) }
        return hash.finalize().map { String(format: "%02x", $0) }.joined()
    }
}

public struct History: Codable, Equatable {
    public static let defaultLimit = 200
    public static let inlineText = 64 * 1024

    /// How many unpinned items are kept.
    public var limit = History.defaultLimit
    /// Pinned items first (in the order pinned), then the rest, newest first.
    public private(set) var items: [Item] = []

    public init() {}

    public var pinned: [Item] { items.filter(\.pinned) }
    public var recent: [Item] { items.filter { !$0.pinned } }

    public func item(_ id: UUID) -> Item? {
        items.first { $0.id == id }
    }

    /// What adding a capture did: the item (new or repeated), whether it is new, and what fell off the end.
    public struct Added {
        public var item: Item
        public var isNew: Bool
        public var dropped: [Item]
    }

    /// Adds a capture copied at `now`. A repeat moves to the top of its group and counts once more.
    public mutating func add(_ capture: Capture, at now: Date) -> Added {
        let print = Item.fingerprint(capture)
        if let i = items.firstIndex(where: { $0.fingerprint == print }) {
            items[i].last = now
            items[i].copies += 1
            items[i].source = capture.source ?? items[i].source
            let item = items[i]
            // A pin stays where it was pinned; anything else goes back to the top.
            if !item.pinned {
                items.remove(at: i)
                insert(item)
            }
            return Added(item: item, isNew: false, dropped: [])
        }
        let text = History.prefix(capture.text, bytes: History.inlineText)
        let item = Item(
            id: UUID(), kind: capture.kind, text: text, textFile: nil, dataFile: nil, dataType: capture.dataType,
            files: capture.files, bytes: (capture.data?.count ?? 0) + capture.text.utf8.count, first: now, last: now,
            copies: 1, pinned: false, source: capture.source, fingerprint: print
        )
        insert(item)
        return Added(item: item, isNew: true, dropped: trim())
    }

    /// At most `bytes` of `text`, cut between characters (never through a Thai character, or an emoji).
    static func prefix(_ text: String, bytes: Int) -> String {
        guard text.utf8.count > bytes else { return text }
        var end = text.utf8.index(text.utf8.startIndex, offsetBy: bytes)
        while String.Index(end, within: text) == nil, end > text.utf8.startIndex {
            end = text.utf8.index(before: end)
        }
        guard let cut = String.Index(end, within: text) else { return "" }
        return String(text[..<cut])
    }

    /// Sets where an item's contents were written.
    public mutating func setFiles(_ id: UUID, text: String?, data: String?) {
        guard let i = items.firstIndex(where: { $0.id == id }) else { return }
        items[i].textFile = text
        items[i].dataFile = data
    }

    /// At the top of the unpinned items, just below the pins.
    private mutating func insert(_ item: Item) {
        items.insert(item, at: items.firstIndex { !$0.pinned } ?? items.endIndex)
    }

    /// Drops the oldest unpinned items beyond the limit; returns them, so their files can go too.
    public mutating func trim() -> [Item] {
        var dropped: [Item] = []
        while recent.count > limit, let i = items.lastIndex(where: { !$0.pinned }) {
            dropped.append(items.remove(at: i))
        }
        return dropped
    }

    /// Pins or unpins; a pin goes to the end of the pins, an unpin to the top of the rest.
    public mutating func pin(_ id: UUID, _ on: Bool) {
        guard let i = items.firstIndex(where: { $0.id == id }), items[i].pinned != on else { return }
        var item = items.remove(at: i)
        item.pinned = on
        insert(item)
    }

    @discardableResult
    public mutating func delete(_ id: UUID) -> Item? {
        guard let i = items.firstIndex(where: { $0.id == id }) else { return nil }
        return items.remove(at: i)
    }

    /// Clears everything but the pins; returns what went.
    public mutating func clear() -> [Item] {
        let gone = recent
        items.removeAll { !$0.pinned }
        return gone
    }
}
