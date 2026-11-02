// The history on disk (ADR-005 R23): `history.json`, written whole and atomically, and `items/` for rich text,
// images and long text. All of it is Clips' own `data/` folder, which uninstall deletes.

import Foundation

public final class Store {
    public let dir: URL
    private var items: URL { dir.appendingPathComponent("items", isDirectory: true) }
    private var file: URL { dir.appendingPathComponent("history.json") }

    public init(dir: URL) {
        self.dir = dir
    }

    public func load() -> History {
        guard let data = try? Data(contentsOf: file),
              let history = try? JSONDecoder.dates.decode(History.self, from: data)
        else { return History() }
        return history
    }

    public func save(_ history: History) throws {
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try JSONEncoder.dates.encode(history).write(to: file, options: .atomic)
    }

    /// Writes an item's contents; returns the file's name in `items/`.
    public func write(_ data: Data, id: UUID, ext: String) throws -> String {
        try FileManager.default.createDirectory(at: items, withIntermediateDirectories: true)
        let name = "\(id.uuidString).\(ext)"
        try data.write(to: items.appendingPathComponent(name), options: .atomic)
        return name
    }

    public func read(_ name: String) -> Data? {
        try? Data(contentsOf: items.appendingPathComponent(name))
    }

    public func delete(_ item: Item) {
        for name in [item.textFile, item.dataFile].compactMap({ $0 }) {
            try? FileManager.default.removeItem(at: items.appendingPathComponent(name))
        }
    }

    /// Saves a capture's contents beside a new item: the whole text if it was cut, and its rich text or image.
    public func keep(_ capture: Capture, as added: History.Added, in history: inout History) throws {
        guard added.isNew else { return }
        let id = added.item.id
        let text = capture.text.utf8.count > History.inlineText
            ? try write(Data(capture.text.utf8), id: id, ext: "txt") : nil
        let data = try capture.data.map { try write($0, id: id, ext: Store.ext(capture.dataType)) }
        history.setFiles(id, text: text, data: data)
    }

    static func ext(_ type: String?) -> String {
        switch type {
        case ClipType.png: return "png"
        case ClipType.tiff: return "tiff"
        case ClipType.rtf: return "rtf"
        case ClipType.html: return "html"
        default: return "bin"
        }
    }

    /// An item's whole text (read from its file when it was too long to keep in the list).
    public func fullText(_ item: Item) -> String {
        guard let name = item.textFile, let data = read(name) else { return item.text }
        return String(decoding: data, as: UTF8.self)
    }
}

extension JSONEncoder {
    static var dates: JSONEncoder {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        return encoder
    }
}

extension JSONDecoder {
    static var dates: JSONDecoder {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }
}
