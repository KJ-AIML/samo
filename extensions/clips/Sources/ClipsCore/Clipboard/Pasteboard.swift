// The clipboard as Clips sees it (ADR-005): a counter that changes with every copy, the kinds of thing on it, and
// their contents. Reading the counter and the kinds asks macOS for nothing; reading contents is what "Paste from
// Other Apps" governs, so Clips reads contents only after the kinds pass its secret checks (R22).

import AppKit
import Foundation

public enum ClipType {
    public static let text = "public.utf8-plain-text"
    public static let rtf = "public.rtf"
    public static let html = "public.html"
    public static let png = "public.png"
    public static let tiff = "public.tiff"
    public static let fileURL = "public.file-url"
}

public protocol Pasteboard: AnyObject {
    var changeCount: Int { get }
    var types: [String] { get }
    func data(forType type: String) -> Data?
    func string(forType type: String) -> String?
    /// The files on the clipboard, as paths.
    func files() -> [String]
    /// Puts an item back on the clipboard (to paste it): its text, its rich text or image, or its files. Returns the
    /// new change count, so the watcher can tell Clips' own copy from yours.
    func put(text: String?, data: Data?, dataType: String?, files: [String]) -> Int
}

/// The real clipboard: this Mac's general pasteboard, written for this Mac only (not Universal Clipboard).
public final class SystemPasteboard: Pasteboard {
    private let board = NSPasteboard.general

    public init() {}

    public var changeCount: Int { board.changeCount }
    public var types: [String] { (board.types ?? []).map(\.rawValue) }

    public func data(forType type: String) -> Data? {
        board.data(forType: NSPasteboard.PasteboardType(type))
    }

    public func string(forType type: String) -> String? {
        board.string(forType: NSPasteboard.PasteboardType(type))
    }

    public func files() -> [String] {
        let urls = board.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL]
        return (urls ?? []).map(\.path)
    }

    public func put(text: String?, data: Data?, dataType: String?, files: [String]) -> Int {
        board.prepareForNewContents(with: .currentHostOnly)
        if !files.isEmpty {
            board.writeObjects(files.map { URL(fileURLWithPath: $0) as NSURL })
        } else {
            if let data, let dataType {
                board.setData(data, forType: NSPasteboard.PasteboardType(dataType))
            }
            if let text {
                board.setString(text, forType: .string)
            }
        }
        return board.changeCount
    }
}
