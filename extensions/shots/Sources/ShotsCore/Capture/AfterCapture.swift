// What happens to a shot once taken (R19): copy and save first, before any window opens, and say so if either
// fails. Only then does it open in the editor, or as a pin.

import CoreGraphics
import Foundation

/// The clipboard, for this Mac only (not Universal Clipboard, R16).
public protocol Clipboard {
    func put(png: Data) -> Bool
}

/// The disk.
public protocol Disk {
    func makeFolder(_ url: URL) throws
    func exists(_ url: URL) -> Bool
    /// Writes a new file; never over one.
    func write(_ data: Data, to url: URL) throws
    /// Writes over `url`, all at once (Save in the editor, over the shot's own file).
    func replace(_ data: Data, at url: URL) throws
}

public struct CaptureOutcome: Equatable {
    public var savedTo: URL?
    public var bytes: Int
    public var copied: Bool
    /// What opened after: the editor, a pin, or nothing.
    public var then: Settings.Then
    public var problems: [String]

    /// The note shown near the pointer.
    public var note: String {
        if let problem = problems.first { return problem }
        switch (copied, savedTo != nil) {
        case (true, true): return "Copied and saved"
        case (true, false): return "Copied"
        case (false, true): return "Saved to \(savedTo?.deletingLastPathComponent().lastPathComponent ?? "Shots")"
        case (false, false): return "Captured"
        }
    }
}

/// A shot, kept only while something shows or needs it.
public final class Shot {
    public let image: CGImage
    /// Points to pixels, for showing it at its real size.
    public let scale: CGFloat
    public let takenAt: Date
    public var savedTo: URL?
    /// Where it was on screen (global points, AppKit's), so a pin opens where the shot was taken.
    public var origin: CGPoint?

    public init(image: CGImage, scale: CGFloat, takenAt: Date, origin: CGPoint? = nil) {
        self.image = image
        self.scale = scale
        self.takenAt = takenAt
        self.origin = origin
    }
}

public enum AfterCapture {
    public static func run(
        _ shot: Shot,
        settings: Settings,
        clipboard: Clipboard,
        disk: Disk,
        open: (Shot, Settings.Then) -> Void
    ) -> CaptureOutcome {
        var outcome = CaptureOutcome(savedTo: nil, bytes: 0, copied: false, then: settings.then, problems: [])
        let image = settings.retina ? shot.image : (Encoding.scaled(shot.image, by: 1 / max(shot.scale, 1)) ?? shot.image)
        let density = settings.retina ? shot.scale : 1
        // Encoded once when the clipboard and the file both want PNG (30–70 ms a whole screen).
        var png: Data?
        if settings.after == .copy || settings.after == .both {
            png = Encoding.data(image, format: .png, scale: density)
            if let png, clipboard.put(png: png) {
                outcome.copied = true
            } else {
                outcome.problems.append("Couldn't copy the shot")
            }
        }
        if settings.after == .save || settings.after == .both {
            do {
                let encoded = settings.format == .png ? png : nil
                guard let data = encoded ?? Encoding.data(image, format: settings.format, scale: density) else {
                    throw CocoaError(.fileWriteUnknown)
                }
                try disk.makeFolder(settings.folderURL)
                let name = FileNaming.name(at: shot.takenAt, format: settings.format)
                let url = FileNaming.unique(name, in: settings.folderURL, exists: disk.exists)
                try disk.write(data, to: url)
                outcome.savedTo = url
                outcome.bytes = data.count
                shot.savedTo = url
            } catch {
                outcome.problems.append("Couldn't save the shot to \(settings.folder): \(error.localizedDescription)")
            }
        }
        if settings.then != .nothing {
            open(shot, settings.then)
        }
        return outcome
    }
}

/// The real disk.
public struct FileDisk: Disk {
    public init() {}
    public func makeFolder(_ url: URL) throws {
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    }
    public func exists(_ url: URL) -> Bool {
        FileManager.default.fileExists(atPath: url.path)
    }
    public func write(_ data: Data, to url: URL) throws {
        try data.write(to: url, options: .withoutOverwriting)
    }
    public func replace(_ data: Data, at url: URL) throws {
        try data.write(to: url, options: .atomic)
    }
}
