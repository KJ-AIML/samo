// Getting an edited shot out of the editor (ADR-004 S3, R19): copy, save over the shot's own file, save elsewhere,
// drag out as a file, or pin. Each goes through the one renderer, so they all carry the same marks and redactions.
// A failure is said, never swallowed, and leaves the document dirty.

import CoreGraphics
import Foundation

public struct ExportResult: Equatable {
    public var url: URL?
    public var bytes: Int
    public var problem: String?

    static func failed(_ problem: String) -> ExportResult {
        ExportResult(url: nil, bytes: 0, problem: problem)
    }
}

public enum Export {
    /// Pixels per point of what is exported.
    static func density(_ doc: Document, retina: Bool) -> CGFloat {
        retina ? doc.scale : 1
    }

    /// PNG for a `.png` file, JPEG for `.jpg` or `.jpeg`.
    public static func format(of url: URL) -> Settings.Format {
        ["jpg", "jpeg"].contains(url.pathExtension.lowercased()) ? .jpeg : .png
    }

    /// To this Mac's clipboard, as PNG.
    public static func copy(_ doc: Document, shot: Shot, redaction: Redaction, retina: Bool,
                            clipboard: Clipboard) -> String? {
        guard let image = Renderer.export(doc, base: shot.image, redaction: redaction, retina: retina),
              let png = Encoding.data(image, format: .png, scale: density(doc, retina: retina)),
              clipboard.put(png: png)
        else { return "Couldn't copy the shot" }
        doc.markExported()
        return nil
    }

    /// Over the shot's own file, in its format. A shot that was never saved, or whose file is gone, gets a new
    /// name in the folder.
    public static func save(_ doc: Document, shot: Shot, redaction: Redaction, settings: Settings,
                            disk: Disk) -> ExportResult {
        let url: URL
        let over: Bool
        if let saved = shot.savedTo, disk.exists(saved) {
            url = saved
            over = true
        } else {
            do {
                try disk.makeFolder(settings.folderURL)
            } catch {
                return .failed("Couldn't save the shot to \(settings.folder): \(error.localizedDescription)")
            }
            let name = FileNaming.name(at: shot.takenAt, format: settings.format)
            url = FileNaming.unique(name, in: settings.folderURL, exists: disk.exists)
            over = false
        }
        let result = write(doc, shot: shot, redaction: redaction, retina: settings.retina, to: url, over: over,
                           disk: disk)
        if result.problem == nil {
            shot.savedTo = url
            doc.markSaved()
        }
        return result
    }

    /// Somewhere else, chosen in a save panel (which has already asked about replacing a file).
    public static func saveAs(_ doc: Document, shot: Shot, redaction: Redaction, retina: Bool, to url: URL,
                              disk: Disk) -> ExportResult {
        let result = write(doc, shot: shot, redaction: redaction, retina: retina, to: url, over: true, disk: disk)
        if result.problem == nil { doc.markExported() }
        return result
    }

    static func write(_ doc: Document, shot: Shot, redaction: Redaction, retina: Bool, to url: URL, over: Bool,
                      disk: Disk) -> ExportResult {
        let folder = Settings.abbreviate(url.deletingLastPathComponent().path)
        guard let image = Renderer.export(doc, base: shot.image, redaction: redaction, retina: retina),
              let data = Encoding.data(image, format: format(of: url), scale: density(doc, retina: retina))
        else { return .failed("Couldn't save the shot to \(folder)") }
        do {
            if over {
                try disk.replace(data, at: url)
            } else {
                try disk.write(data, to: url)
            }
        } catch {
            return .failed("Couldn't save the shot to \(folder): \(error.localizedDescription)")
        }
        return ExportResult(url: url, bytes: data.count, problem: nil)
    }

    /// The file a drag carries: the shot's own file when it shows exactly this, else a fresh PNG in `folder`
    /// (`data/drag/`), named after the shot.
    public static func dragFile(_ doc: Document, shot: Shot, redaction: Redaction, retina: Bool, folder: URL,
                                disk: Disk) -> URL? {
        if let saved = shot.savedTo, doc.matchesSaved, disk.exists(saved) {
            return saved
        }
        let stem = shot.savedTo.map { $0.deletingPathExtension().lastPathComponent }
            ?? (FileNaming.name(at: shot.takenAt, format: .png) as NSString).deletingPathExtension
        let url = folder.appendingPathComponent(stem + ".png")
        guard let image = Renderer.export(doc, base: shot.image, redaction: redaction, retina: retina),
              let png = Encoding.data(image, format: .png, scale: density(doc, retina: retina))
        else { return nil }
        do {
            try disk.makeFolder(folder)
            try disk.replace(png, at: url)
        } catch {
            return nil
        }
        return url
    }

    /// Empties `data/drag/` (at start and stop: files a drag needed, never left behind).
    public static func clearDrags(_ folder: URL) {
        try? FileManager.default.removeItem(at: folder)
    }

    /// The shot to pin: itself if untouched, else the export, placed where the kept part was on screen.
    public static func pinned(_ doc: Document, shot: Shot, redaction: Redaction) -> Shot? {
        if doc.untouched { return shot }
        guard let image = Renderer.export(doc, base: shot.image, redaction: redaction, retina: true) else { return nil }
        let layout = Renderer.layout(doc)
        let origin = shot.origin.map {
            CGPoint(x: $0.x + (doc.visibleRect.minX - layout.padding) / doc.scale,
                    y: $0.y + (doc.visibleRect.minY - layout.padding) / doc.scale)
        }
        return Shot(image: image, scale: doc.scale, takenAt: shot.takenAt, origin: origin)
    }
}
