// What happens to a shot once taken (R19): copy and save first, before any window opens, and say so if either
// fails. A pin opens only after that.

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
    func write(_ data: Data, to url: URL) throws
}

public struct CaptureOutcome: Equatable {
    public var savedTo: URL?
    public var bytes: Int
    public var copied: Bool
    public var pinned: Bool
    public var problems: [String]

    /// The note shown near the pointer.
    public var note: String {
        if let problem = problems.first { return problem }
        switch (copied, savedTo != nil, pinned) {
        case (true, true, _): return "Copied and saved"
        case (true, false, _): return "Copied"
        case (false, true, _): return "Saved to \(savedTo?.deletingLastPathComponent().lastPathComponent ?? "Shots")"
        case (false, false, true): return "Pinned"
        case (false, false, false): return "Captured"
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

    public init(image: CGImage, scale: CGFloat, takenAt: Date) {
        self.image = image
        self.scale = scale
        self.takenAt = takenAt
    }
}

public enum AfterCapture {
    public static func run(
        _ shot: Shot,
        settings: Settings,
        clipboard: Clipboard,
        disk: Disk,
        openPin: (Shot) -> Void
    ) -> CaptureOutcome {
        var outcome = CaptureOutcome(savedTo: nil, bytes: 0, copied: false, pinned: false, problems: [])
        let image = settings.retina ? shot.image : (Encoding.scaled(shot.image, by: 1 / max(shot.scale, 1)) ?? shot.image)
        if settings.after == .copy || settings.after == .both {
            if let png = Encoding.data(image, format: .png), clipboard.put(png: png) {
                outcome.copied = true
            } else {
                outcome.problems.append("Couldn't copy the shot")
            }
        }
        if settings.after == .save || settings.after == .both {
            do {
                guard let data = Encoding.data(image, format: settings.format) else {
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
        if settings.after == .pin {
            openPin(shot)
            outcome.pinned = true
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
}
