// Shots kept in memory only while a window shows them (R17), and none lost when Shots stops (R19): whatever is
// still open and was never saved is saved as `… (unsaved).png`, and so are edits not yet copied or saved, beside
// the shot and never over it.

import CoreGraphics
import SamoKit
import Foundation

public final class ShotStore {
    private struct Entry {
        let shot: Shot
        var holders: Int
        /// The editor's export while it has changes not yet copied or saved; `nil` when it has none.
        var edits: (() -> CGImage?)?
    }

    private var entries: [ObjectIdentifier: Entry] = [:]

    public init() {}

    /// A window now shows `shot`.
    public func retain(_ shot: Shot) {
        let key = ObjectIdentifier(shot)
        entries[key, default: Entry(shot: shot, holders: 0, edits: nil)].holders += 1
    }

    /// An editor showing `shot` hands over how to get its unsaved edits, or `nil` when it closes.
    public func setEdits(for shot: Shot, _ edits: (() -> CGImage?)?) {
        entries[ObjectIdentifier(shot)]?.edits = edits
    }

    /// A window showing `shot` closed; the last one lets it go.
    public func release(_ shot: Shot) {
        let key = ObjectIdentifier(shot)
        guard var entry = entries[key] else { return }
        entry.holders -= 1
        entries[key] = entry.holders > 0 ? entry : nil
    }

    public var live: Int { entries.count }

    /// Saves, as `<name> (unsaved).png`, every shown shot that was never saved (into the folder) and every
    /// editor's unsaved edits (beside the shot's file, never over it). Returns where.
    @discardableResult
    public func saveUnsaved(settings: Settings, disk: Disk) -> [URL] {
        var saved: [URL] = []
        for entry in entries.values {
            let shot = entry.shot
            let image: CGImage
            if let edited = entry.edits?() {
                image = edited
            } else if shot.savedTo == nil {
                image = shot.image
            } else {
                continue
            }
            guard let data = Encoding.data(image, format: .png, scale: shot.scale) else { continue }
            let folder = shot.savedTo?.deletingLastPathComponent() ?? settings.folderURL
            let stem = shot.savedTo.map { $0.deletingPathExtension().lastPathComponent }
                ?? (FileNaming.name(at: shot.takenAt, format: .png) as NSString).deletingPathExtension
            do {
                try disk.makeFolder(folder)
                let url = FileNaming.unique("\(stem) (unsaved).png", in: folder, exists: disk.exists)
                try disk.write(data, to: url)
                if shot.savedTo == nil { shot.savedTo = url }
                saved.append(url)
            } catch {
                continue
            }
        }
        return saved
    }
}

public enum PinMath {
    /// A pin's opacity after a scroll: between 20 % and 100 %.
    public static func opacity(_ current: Double, scrolledBy delta: Double) -> Double {
        min(1, max(0.2, current + delta * 0.02))
    }

    /// A pin's size: the shot at its real size in points, shrunk to fit 80 % of the screen.
    public static func size(pixels: CGSize, scale: CGFloat, screen: CGSize) -> CGSize {
        let points = CGSize(width: pixels.width / max(scale, 1), height: pixels.height / max(scale, 1))
        let fit = min(1, (screen.width * 0.8) / max(points.width, 1), (screen.height * 0.8) / max(points.height, 1))
        return CGSize(width: (points.width * fit).rounded(), height: (points.height * fit).rounded())
    }
}
